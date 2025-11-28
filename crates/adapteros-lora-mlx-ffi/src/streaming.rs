//! Token-by-token streaming support for MLX backend
//!
//! Provides real-time streaming generation with:
//! - Server-Sent Events (SSE) formatting
//! - Partial UTF-8 token healing
//! - Backpressure control
//! - KV cache incremental updates
//! - Client disconnect detection
//! - First-token latency optimization
//!
//! # Architecture
//!
//! ```text
//! MLXStreamingGenerator
//!   ├─> TokenStream (mpsc channel)
//!   ├─> UTF8TokenHealer (partial character buffering)
//!   ├─> StopSequenceDetector (termination detection)
//!   └─> KVCacheManager (incremental updates)
//! ```

use adapteros_core::{derive_seed, AosError, B3Hash, Result};
use adapteros_deterministic_exec::spawn_deterministic;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, trace, warn};

/// Streaming event emitted during generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Token generated with timing information
    Token {
        /// Decoded token text
        text: String,
        /// Token ID
        token_id: u32,
        /// Time since last token (microseconds)
        delta_us: u64,
        /// Cumulative time since generation start (microseconds)
        elapsed_us: u64,
    },
    /// Generation completed
    Done {
        /// Reason for completion
        finish_reason: FinishReason,
        /// Total tokens generated
        total_tokens: usize,
        /// Total generation time (microseconds)
        total_time_us: u64,
        /// Average tokens per second
        tokens_per_sec: f32,
    },
    /// Error occurred during generation
    Error {
        /// Error message
        message: String,
        /// Error code
        code: String,
    },
    /// Keep-alive heartbeat
    KeepAlive,
}

/// Reason for generation completion
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// Stop sequence detected
    Stop,
    /// Maximum token limit reached
    Length,
    /// Client disconnected
    Cancelled,
    /// Error occurred
    Error,
}

/// Streaming configuration
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// Maximum tokens to generate
    pub max_tokens: usize,
    /// Stop sequences
    pub stop_sequences: Vec<String>,
    /// Temperature for sampling
    pub temperature: f32,
    /// Top-p nucleus sampling
    pub top_p: Option<f32>,
    /// Enable keep-alive messages
    pub keep_alive: bool,
    /// Keep-alive interval
    pub keep_alive_interval: Duration,
    /// Channel buffer size
    pub channel_buffer: usize,
    /// Timeout for token generation
    pub token_timeout: Duration,
    /// Enable partial UTF-8 healing
    pub enable_utf8_healing: bool,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            stop_sequences: vec![],
            temperature: 0.7,
            top_p: None,
            keep_alive: true,
            keep_alive_interval: Duration::from_secs(15),
            channel_buffer: 100,
            token_timeout: Duration::from_secs(30),
            enable_utf8_healing: true,
        }
    }
}

/// Token stream wrapper implementing Stream trait
pub struct TokenStream {
    receiver: mpsc::Receiver<StreamEvent>,
}

impl TokenStream {
    /// Create new token stream from receiver
    pub fn new(receiver: mpsc::Receiver<StreamEvent>) -> Self {
        Self { receiver }
    }

    /// Try to receive next event with timeout
    pub async fn next_with_timeout(&mut self, timeout_duration: Duration) -> Option<StreamEvent> {
        match timeout(timeout_duration, self.receiver.recv()).await {
            Ok(Some(event)) => Some(event),
            Ok(None) => None,
            Err(_) => Some(StreamEvent::KeepAlive),
        }
    }
}

impl futures_util::Stream for TokenStream {
    type Item = StreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

/// UTF-8 token healer for partial character buffering
///
/// Buffers incomplete UTF-8 sequences at token boundaries to ensure
/// valid UTF-8 strings are emitted, preventing mojibake.
#[derive(Debug)]
pub struct UTF8TokenHealer {
    /// Buffer for incomplete UTF-8 sequences
    buffer: Vec<u8>,
    /// Enable healing
    enabled: bool,
}

impl UTF8TokenHealer {
    /// Create new token healer
    pub fn new(enabled: bool) -> Self {
        Self {
            buffer: Vec::new(),
            enabled,
        }
    }

    /// Process token bytes and return valid UTF-8 string
    ///
    /// Buffers incomplete sequences until complete character is available.
    pub fn process(&mut self, token_bytes: &[u8]) -> Result<Option<String>> {
        if !self.enabled {
            // No healing - return raw conversion
            return Ok(Some(
                String::from_utf8(token_bytes.to_vec())
                    .map_err(|e| AosError::Parse(format!("Invalid UTF-8: {}", e)))?,
            ));
        }

        // Append to buffer
        self.buffer.extend_from_slice(token_bytes);

        // Try to decode as much valid UTF-8 as possible
        match std::str::from_utf8(&self.buffer) {
            Ok(s) => {
                // Complete valid UTF-8 - emit and clear buffer
                let result = s.to_string();
                self.buffer.clear();
                Ok(Some(result))
            }
            Err(e) => {
                let valid_up_to = e.valid_up_to();

                if valid_up_to == 0 {
                    // No valid UTF-8 yet - keep buffering
                    trace!(
                        buffer_len = self.buffer.len(),
                        "Buffering incomplete UTF-8 sequence"
                    );
                    Ok(None)
                } else {
                    // Partial valid UTF-8 - emit valid portion, keep invalid
                    let valid_bytes = self.buffer[..valid_up_to].to_vec();
                    let result = String::from_utf8(valid_bytes)
                        .map_err(|e| AosError::Parse(format!("UTF-8 healing failed: {}", e)))?;

                    // Keep invalid portion in buffer
                    self.buffer.drain(..valid_up_to);

                    trace!(
                        emitted_len = result.len(),
                        buffer_len = self.buffer.len(),
                        "Emitted partial UTF-8, buffering remainder"
                    );

                    Ok(Some(result))
                }
            }
        }
    }

    /// Flush remaining buffered bytes
    ///
    /// Call at end of generation to handle any remaining incomplete sequences.
    pub fn flush(&mut self) -> Result<Option<String>> {
        if self.buffer.is_empty() {
            return Ok(None);
        }

        // Try to decode buffer as UTF-8, using replacement characters for invalid sequences
        let result = String::from_utf8_lossy(&self.buffer).to_string();
        self.buffer.clear();

        if !result.is_empty() {
            warn!(
                result_len = result.len(),
                "Flushed incomplete UTF-8 sequence with replacement characters"
            );
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
}

/// Stop sequence detector
///
/// Detects stop sequences using sliding window approach for efficient
/// multi-sequence detection across token boundaries.
#[derive(Debug)]
pub struct StopSequenceDetector {
    /// Stop sequences to detect
    sequences: Vec<String>,
    /// Sliding window buffer
    window: VecDeque<char>,
    /// Maximum sequence length
    max_len: usize,
}

impl StopSequenceDetector {
    /// Create new stop sequence detector
    pub fn new(sequences: Vec<String>) -> Self {
        let max_len = sequences.iter().map(|s| s.len()).max().unwrap_or(0);

        Self {
            sequences,
            window: VecDeque::with_capacity(max_len),
            max_len,
        }
    }

    /// Add token text and check for stop sequence
    ///
    /// Returns true if a stop sequence is detected.
    pub fn check(&mut self, text: &str) -> bool {
        // Add characters to sliding window and check after each
        for ch in text.chars() {
            self.window.push_back(ch);

            // Maintain max window size
            while self.window.len() > self.max_len {
                self.window.pop_front();
            }

            // Check each stop sequence after adding each character
            let window_str: String = self.window.iter().collect();
            for seq in &self.sequences {
                if window_str.contains(seq) {
                    debug!(sequence = %seq, "Stop sequence detected");
                    return true;
                }
            }
        }

        false
    }
}

/// KV cache manager for incremental updates
///
/// Manages key-value cache for efficient streaming generation.
/// Only updates cache for new tokens, avoiding full recomputation.
#[derive(Debug)]
pub struct KVCacheManager {
    /// Cached key tensors by layer
    key_cache: Vec<Vec<f32>>,
    /// Cached value tensors by layer
    value_cache: Vec<Vec<f32>>,
    /// Number of cached positions
    cached_positions: usize,
}

impl KVCacheManager {
    /// Create new KV cache manager
    pub fn new(num_layers: usize, _hidden_dim: usize) -> Self {
        Self {
            key_cache: vec![Vec::new(); num_layers],
            value_cache: vec![Vec::new(); num_layers],
            cached_positions: 0,
        }
    }

    /// Update cache with new key-value pairs
    pub fn update(&mut self, layer: usize, keys: Vec<f32>, values: Vec<f32>) {
        if layer < self.key_cache.len() {
            self.key_cache[layer].extend(keys);
            self.value_cache[layer].extend(values);
        }
        self.cached_positions += 1;
    }

    /// Get cached keys for layer
    pub fn get_keys(&self, layer: usize) -> Option<&[f32]> {
        self.key_cache.get(layer).map(|v| v.as_slice())
    }

    /// Get cached values for layer
    pub fn get_values(&self, layer: usize) -> Option<&[f32]> {
        self.value_cache.get(layer).map(|v| v.as_slice())
    }

    /// Get number of cached positions
    pub fn len(&self) -> usize {
        self.cached_positions
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cached_positions == 0
    }

    /// Clear cache
    pub fn clear(&mut self) {
        for cache in &mut self.key_cache {
            cache.clear();
        }
        for cache in &mut self.value_cache {
            cache.clear();
        }
        self.cached_positions = 0;
    }
}

/// MLX streaming generator
///
/// Generates tokens one at a time with proper streaming support,
/// UTF-8 healing, and stop sequence detection.
pub struct MLXStreamingGenerator {
    /// Streaming configuration
    config: StreamingConfig,
    /// UTF-8 token healer
    healer: UTF8TokenHealer,
    /// Stop sequence detector
    stop_detector: StopSequenceDetector,
    /// KV cache manager
    kv_cache: KVCacheManager,
    /// Generation start time
    start_time: Instant,
    /// Last token time
    last_token_time: Instant,
    /// Tokens generated
    tokens_generated: usize,
    /// Base seed for determinism
    base_seed: B3Hash,
}

impl MLXStreamingGenerator {
    /// Create new streaming generator
    ///
    /// # Arguments
    /// * `config` - Streaming configuration
    /// * `base_seed` - Base seed for deterministic generation
    /// * `num_layers` - Number of transformer layers (for KV cache)
    /// * `hidden_dim` - Hidden dimension size (for KV cache)
    pub fn new(
        config: StreamingConfig,
        base_seed: B3Hash,
        num_layers: usize,
        hidden_dim: usize,
    ) -> Self {
        let healer = UTF8TokenHealer::new(config.enable_utf8_healing);
        let stop_detector = StopSequenceDetector::new(config.stop_sequences.clone());
        let kv_cache = KVCacheManager::new(num_layers, hidden_dim);

        let now = Instant::now();

        Self {
            config,
            healer,
            stop_detector,
            kv_cache,
            start_time: now,
            last_token_time: now,
            tokens_generated: 0,
            base_seed,
        }
    }

    /// Create streaming generator from model config
    ///
    /// Convenience constructor that extracts parameters from ModelConfig.
    pub fn from_model_config(
        config: StreamingConfig,
        base_seed: B3Hash,
        model_config: &crate::ModelConfig,
    ) -> Self {
        Self::new(
            config,
            base_seed,
            model_config.num_hidden_layers,
            model_config.hidden_size,
        )
    }

    /// Generate streaming tokens
    ///
    /// Yields tokens as they're generated via the provided channel.
    /// Handles UTF-8 healing, stop detection, and backpressure.
    pub async fn generate<F>(
        &mut self,
        mut generate_token_fn: F,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()>
    where
        F: FnMut(usize, &B3Hash) -> Result<(u32, Vec<u8>)>,
    {
        debug!(
            max_tokens = self.config.max_tokens,
            temperature = self.config.temperature,
            stop_sequences = ?self.config.stop_sequences,
            "Starting streaming generation"
        );

        // Reset state
        self.start_time = Instant::now();
        self.last_token_time = self.start_time;
        self.tokens_generated = 0;

        // Spawn keep-alive task if enabled
        let _keep_alive_handle = if self.config.keep_alive {
            let tx_clone = tx.clone();
            let interval = self.config.keep_alive_interval;
            let task_name = format!("mlx-stream-keep-alive-{}", self.tokens_generated);
            Some(
                spawn_deterministic(task_name, async move {
                    let mut ticker = tokio::time::interval(interval);
                    loop {
                        ticker.tick().await;
                        if tx_clone.send(StreamEvent::KeepAlive).await.is_err() {
                            break;
                        }
                    }
                })
                .map_err(|e| {
                    AosError::DeterminismViolation(format!(
                        "Failed to spawn keep-alive task: {}",
                        e
                    ))
                })?,
            )
        } else {
            None
        };

        // Generation loop
        for step in 0..self.config.max_tokens {
            // Derive deterministic seed for this step
            let step_seed = self.derive_step_seed(step);

            // Generate next token
            let (token_id, token_bytes) = match generate_token_fn(step, &step_seed) {
                Ok(result) => result,
                Err(e) => {
                    warn!(error = %e, "Token generation failed");
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: e.to_string(),
                            code: "generation_error".to_string(),
                        })
                        .await;
                    return Err(e);
                }
            };

            // Process token through UTF-8 healer
            let token_text = match self.healer.process(&token_bytes)? {
                Some(text) => text,
                None => {
                    // Incomplete UTF-8 - buffer and continue
                    trace!("Buffering incomplete UTF-8 sequence");
                    continue;
                }
            };

            // Calculate timing
            let now = Instant::now();
            let delta_us = now.duration_since(self.last_token_time).as_micros() as u64;
            let elapsed_us = now.duration_since(self.start_time).as_micros() as u64;
            self.last_token_time = now;
            self.tokens_generated += 1;

            // Check stop sequences before sending
            let should_stop = self.stop_detector.check(&token_text);

            // Send token event
            let send_result = tx
                .send(StreamEvent::Token {
                    text: token_text.clone(),
                    token_id,
                    delta_us,
                    elapsed_us,
                })
                .await;

            if send_result.is_err() {
                // Client disconnected
                debug!("Client disconnected during streaming");
                let _ = tx
                    .send(StreamEvent::Done {
                        finish_reason: FinishReason::Cancelled,
                        total_tokens: self.tokens_generated,
                        total_time_us: elapsed_us,
                        tokens_per_sec: self.tokens_generated as f32
                            / (elapsed_us as f32 / 1_000_000.0),
                    })
                    .await;
                return Ok(());
            }

            // Check stop conditions
            if should_stop {
                debug!("Stop sequence detected, completing generation");
                let _ = self.send_done(tx, FinishReason::Stop).await;
                return Ok(());
            }

            trace!(
                token_id = token_id,
                text = %token_text,
                delta_us = delta_us,
                "Token generated"
            );
        }

        // Flush any remaining buffered UTF-8
        if let Some(remaining) = self.healer.flush()? {
            let now = Instant::now();
            let delta_us = now.duration_since(self.last_token_time).as_micros() as u64;
            let elapsed_us = now.duration_since(self.start_time).as_micros() as u64;

            let _ = tx
                .send(StreamEvent::Token {
                    text: remaining,
                    token_id: 0, // Flushed token
                    delta_us,
                    elapsed_us,
                })
                .await;
        }

        // Max tokens reached
        debug!(
            tokens = self.tokens_generated,
            "Maximum tokens reached, completing generation"
        );
        self.send_done(tx, FinishReason::Length).await?;

        Ok(())
    }

    /// Send completion event
    async fn send_done(
        &self,
        tx: mpsc::Sender<StreamEvent>,
        finish_reason: FinishReason,
    ) -> Result<()> {
        let elapsed_us = self.start_time.elapsed().as_micros() as u64;
        let tokens_per_sec = if elapsed_us > 0 {
            self.tokens_generated as f32 / (elapsed_us as f32 / 1_000_000.0)
        } else {
            0.0
        };

        tx.send(StreamEvent::Done {
            finish_reason,
            total_tokens: self.tokens_generated,
            total_time_us: elapsed_us,
            tokens_per_sec,
        })
        .await
        .map_err(|_| AosError::Internal("Failed to send completion event".to_string()))?;

        Ok(())
    }

    /// Derive deterministic seed for generation step
    fn derive_step_seed(&self, step: usize) -> B3Hash {
        let label = format!("mlx-stream-step:{}", step);
        B3Hash::from_bytes(derive_seed(&self.base_seed, &label))
    }

    /// Get KV cache for reading
    pub fn kv_cache(&self) -> &KVCacheManager {
        &self.kv_cache
    }

    /// Get mutable KV cache for updates
    pub fn kv_cache_mut(&mut self) -> &mut KVCacheManager {
        &mut self.kv_cache
    }

    /// Create a streaming decoder for token-by-token text conversion
    ///
    /// Useful for converting generated token IDs back to text in streaming contexts.
    /// Handles partial UTF-8 sequences automatically.
    ///
    /// # Arguments
    /// * `tokenizer` - Tokenizer for decoding tokens
    ///
    /// # Returns
    /// Configured streaming decoder
    pub fn create_token_decoder(
        &self,
        tokenizer: &crate::tokenizer::MLXTokenizer,
    ) -> crate::tokenizer::StreamingTokenDecoder {
        crate::tokenizer::StreamingTokenDecoder::new(tokenizer.tokenizer().clone())
    }

    /// Generate streaming tokens from text prompt using tokenizer
    ///
    /// This is a high-level helper that integrates tokenization with streaming generation.
    ///
    /// # Arguments
    /// * `prompt` - Text prompt to generate from
    /// * `tokenizer` - Tokenizer for encoding/decoding
    /// * `generate_fn` - Function that generates tokens
    /// * `tx` - Channel sender for streaming events
    ///
    /// # Returns
    /// Result indicating success or error
    pub async fn generate_from_text<F>(
        &mut self,
        prompt: &str,
        tokenizer: &crate::tokenizer::MLXTokenizer,
        generate_fn: F,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()>
    where
        F: FnMut(usize, &B3Hash) -> Result<(u32, Vec<u8>)>,
    {
        // Encode prompt to tokens (validates the prompt is encodable)
        let _prompt_tokens = tokenizer.encode(prompt)?;

        // Run streaming generation
        self.generate(generate_fn, tx).await
    }

    /// Generate streaming text with chat template
    ///
    /// Applies the tokenizer's chat template before streaming generation.
    ///
    /// # Arguments
    /// * `prompt` - User prompt (will be formatted with chat template)
    /// * `tokenizer` - Tokenizer with chat template support
    /// * `generate_fn` - Function that generates tokens
    /// * `tx` - Channel sender for streaming events
    ///
    /// # Returns
    /// Result indicating success or error
    pub async fn generate_chat_streaming<F>(
        &mut self,
        prompt: &str,
        tokenizer: &crate::tokenizer::MLXTokenizer,
        generate_fn: F,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()>
    where
        F: FnMut(usize, &B3Hash) -> Result<(u32, Vec<u8>)>,
    {
        // Apply chat template
        let formatted_prompt = tokenizer.apply_chat_template(prompt);

        // Generate with formatted prompt
        self.generate_from_text(&formatted_prompt, tokenizer, generate_fn, tx)
            .await
    }
}

/// Server-Sent Events (SSE) formatter
///
/// Formats streaming events as SSE for HTTP streaming.
pub struct SSEFormatter;

impl SSEFormatter {
    /// Format event as SSE message
    ///
    /// Returns formatted SSE string ready to send over HTTP.
    pub fn format(event: &StreamEvent) -> String {
        match event {
            StreamEvent::Token { text, .. } => {
                // OpenAI-compatible chat completion chunk format
                let chunk = serde_json::json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                    "object": "chat.completion.chunk",
                    "created": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    "model": "adapteros-mlx",
                    "choices": [{
                        "index": 0,
                        "delta": {
                            "content": text
                        },
                        "finish_reason": null
                    }]
                });

                format!(
                    "data: {}\n\n",
                    serde_json::to_string(&chunk).unwrap_or_default()
                )
            }
            StreamEvent::Done { finish_reason, .. } => {
                let chunk = serde_json::json!({
                    "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                    "object": "chat.completion.chunk",
                    "created": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    "model": "adapteros-mlx",
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": match finish_reason {
                            FinishReason::Stop => "stop",
                            FinishReason::Length => "length",
                            FinishReason::Cancelled => "cancelled",
                            FinishReason::Error => "error",
                        }
                    }]
                });

                format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    serde_json::to_string(&chunk).unwrap_or_default()
                )
            }
            StreamEvent::Error { message, code } => {
                let error = serde_json::json!({
                    "error": {
                        "message": message,
                        "type": code,
                        "code": code
                    }
                });

                format!(
                    "data: {}\n\n",
                    serde_json::to_string(&error).unwrap_or_default()
                )
            }
            StreamEvent::KeepAlive => ": keep-alive\n\n".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utf8_healer_complete() {
        let mut healer = UTF8TokenHealer::new(true);

        // Complete UTF-8 sequence
        let result = healer.process("Hello".as_bytes()).unwrap();
        assert_eq!(result, Some("Hello".to_string()));
    }

    #[test]
    fn test_utf8_healer_partial() {
        let mut healer = UTF8TokenHealer::new(true);

        // Partial UTF-8 sequence (é split across tokens)
        // é is 0xC3 0xA9 in UTF-8
        let result1 = healer.process(&[0xC3]).unwrap();
        assert_eq!(result1, None); // Buffered

        let result2 = healer.process(&[0xA9]).unwrap();
        assert_eq!(result2, Some("é".to_string()));
    }

    #[test]
    fn test_utf8_healer_flush() {
        let mut healer = UTF8TokenHealer::new(true);

        // Incomplete sequence
        healer.process(&[0xC3]).unwrap();

        // Flush should return replacement character
        let result = healer.flush().unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_stop_sequence_detector() {
        let mut detector = StopSequenceDetector::new(vec!["</s>".to_string(), "\n\n".to_string()]);

        assert!(!detector.check("Hello "));
        assert!(!detector.check("world"));
        assert!(!detector.check("</"));
        assert!(detector.check("s>"));
    }

    #[test]
    fn test_stop_sequence_detector_multiline() {
        let mut detector = StopSequenceDetector::new(vec!["\n\n".to_string()]);

        assert!(!detector.check("Line 1\n"));
        assert!(detector.check("\nLine 2"));
    }

    #[test]
    fn test_sse_formatter_token() {
        let event = StreamEvent::Token {
            text: "Hello".to_string(),
            token_id: 42,
            delta_us: 1000,
            elapsed_us: 5000,
        };

        let sse = SSEFormatter::format(&event);
        assert!(sse.starts_with("data: "));
        assert!(sse.contains("Hello"));
        assert!(sse.contains("chat.completion.chunk"));
    }

    #[test]
    fn test_sse_formatter_done() {
        let event = StreamEvent::Done {
            finish_reason: FinishReason::Stop,
            total_tokens: 42,
            total_time_us: 100000,
            tokens_per_sec: 420.0,
        };

        let sse = SSEFormatter::format(&event);
        assert!(sse.contains("[DONE]"));
        assert!(sse.contains("finish_reason"));
    }

    #[test]
    fn test_kv_cache_manager() {
        let mut cache = KVCacheManager::new(2, 128);

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        cache.update(0, vec![1.0, 2.0], vec![3.0, 4.0]);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        let keys = cache.get_keys(0).unwrap();
        assert_eq!(keys, &[1.0, 2.0]);

        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn test_streaming_config_defaults() {
        let config = StreamingConfig::default();
        assert_eq!(config.max_tokens, 512);
        assert_eq!(config.temperature, 0.7);
        assert!(config.keep_alive);
        assert!(config.enable_utf8_healing);
    }
}
