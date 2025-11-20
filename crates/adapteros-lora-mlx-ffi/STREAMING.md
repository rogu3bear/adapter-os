# Token-by-Token Streaming for MLX Backend

Comprehensive streaming support for real-time inference with the MLX backend.

## Features

### Core Streaming

- **Token-Level Streaming**: Emit tokens as they're generated, not after full completion
- **Server-Sent Events (SSE)**: OpenAI-compatible SSE formatting for HTTP streaming
- **Backpressure Control**: Channel-based flow control prevents memory exhaustion
- **Client Disconnect Detection**: Graceful cancellation when client disconnects

### UTF-8 Token Healing

- **Partial Character Buffering**: Handles tokens split across UTF-8 character boundaries
- **Smart Reconstruction**: Buffers incomplete sequences until full character available
- **Emoji Support**: Properly handles multi-byte emoji characters (👍, 🚀, etc.)
- **Automatic Flush**: Safely handles incomplete sequences at end of generation

### Stop Sequence Detection

- **Sliding Window**: Efficient detection across token boundaries
- **Multiple Sequences**: Support for multiple stop sequences simultaneously
- **Cross-Boundary Detection**: Detects sequences split across multiple tokens
- **Configurable**: Custom stop sequences per request

### Performance Optimizations

- **First-Token Latency**: Optimized for minimal time-to-first-token
- **KV Cache Management**: Incremental cache updates avoid recomputation
- **Pipelined Operations**: Token generation and decoding happen in parallel
- **Zero-Copy Where Possible**: Minimizes data copying during streaming

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                  MLXStreamingGenerator                       │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   │
│  │  Token Gen   │──>│ UTF-8 Healer │──>│ Stop Detect  │   │
│  └──────────────┘   └──────────────┘   └──────────────┘   │
│         │                   │                   │           │
│         v                   v                   v           │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              TokenStream (mpsc channel)              │  │
│  └──────────────────────────────────────────────────────┘  │
│                            │                                │
└────────────────────────────┼────────────────────────────────┘
                             │
                             v
                    ┌─────────────────┐
                    │  SSE Formatter  │
                    └─────────────────┘
                             │
                             v
                    HTTP Streaming Response
```

## Usage

### Basic Streaming

```rust
use adapteros_lora_mlx_ffi::streaming::{
    MLXStreamingGenerator, StreamingConfig, StreamEvent,
};
use adapteros_core::B3Hash;

// Configure streaming
let config = StreamingConfig {
    max_tokens: 512,
    temperature: 0.7,
    stop_sequences: vec!["</s>".to_string()],
    enable_utf8_healing: true,
    ..Default::default()
};

// Create generator with deterministic seed
let base_seed = B3Hash::hash(b"model-hash");
let mut generator = MLXStreamingGenerator::new(config, base_seed, num_layers);

// Create channel
let (tx, mut rx) = tokio::sync::mpsc::channel(100);

// Token generation closure
let generate_fn = |step: usize, seed: &B3Hash| -> Result<(u32, Vec<u8>)> {
    // Run model forward pass
    let logits = model.forward(&tokens, step)?;

    // Sample next token
    let token_id = sample_token(&logits, temperature)?;

    // Decode to bytes
    let token_bytes = tokenizer.decode(token_id)?;

    Ok((token_id, token_bytes))
};

// Start generation (spawns background task)
tokio::spawn(async move {
    generator.generate(generate_fn, tx).await
});

// Consume stream
while let Some(event) = rx.recv().await {
    match event {
        StreamEvent::Token { text, .. } => {
            print!("{}", text); // Real-time output
        }
        StreamEvent::Done { finish_reason, .. } => {
            println!("\nGeneration complete: {:?}", finish_reason);
            break;
        }
        StreamEvent::Error { message, .. } => {
            eprintln!("Error: {}", message);
            break;
        }
        StreamEvent::KeepAlive => continue,
    }
}
```

### Backend Integration

```rust
use adapteros_lora_mlx_ffi::MLXFFIBackend;

// Create backend
let backend = MLXFFIBackend::from_path("path/to/model")?;

// Start streaming generation
let config = StreamingConfig::default();
let mut stream = backend.generate_streaming(prompt_tokens, config).await?;

// Consume via futures Stream trait
use futures::StreamExt;
while let Some(event) = stream.next().await {
    match event {
        StreamEvent::Token { text, .. } => print!("{}", text),
        StreamEvent::Done { .. } => break,
        _ => continue,
    }
}
```

### SSE HTTP Endpoint

```rust
use axum::{response::sse::{Event, Sse}, routing::post, Router};
use adapteros_lora_mlx_ffi::streaming::{SSEFormatter, StreamEvent};
use futures::stream::Stream;

async fn streaming_endpoint(
    request: Json<InferenceRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel(100);

    // Start generation
    tokio::spawn(async move {
        let mut generator = create_generator(request);
        generator.generate(generate_fn, tx).await
    });

    // Convert to SSE stream
    let stream = ReceiverStream::new(rx).map(|event| {
        let sse = SSEFormatter::format(&event);
        Ok(Event::default().data(sse))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

## Configuration

### StreamingConfig

```rust
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
```

### Defaults

```rust
StreamingConfig {
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
```

## Event Types

### StreamEvent::Token

Emitted for each generated token:

```rust
StreamEvent::Token {
    text: String,           // Decoded token text
    token_id: u32,          // Token ID
    delta_us: u64,          // Time since last token (μs)
    elapsed_us: u64,        // Time since generation start (μs)
}
```

### StreamEvent::Done

Emitted when generation completes:

```rust
StreamEvent::Done {
    finish_reason: FinishReason,  // Stop | Length | Cancelled | Error
    total_tokens: usize,          // Total tokens generated
    total_time_us: u64,           // Total time (μs)
    tokens_per_sec: f32,          // Throughput
}
```

### StreamEvent::Error

Emitted on errors:

```rust
StreamEvent::Error {
    message: String,       // Error message
    code: String,          // Error code
}
```

### StreamEvent::KeepAlive

Periodic heartbeat to prevent connection timeout.

## UTF-8 Token Healing

### Problem

Language models often produce tokens that split multi-byte UTF-8 characters:

```
Emoji: 👍 (U+1F44D)
UTF-8: [0xF0, 0x9F, 0x91, 0x8D]

Token 1: [0xF0, 0x9F]     <- Invalid UTF-8!
Token 2: [0x91, 0x8D]     <- Invalid UTF-8!
```

### Solution

UTF8TokenHealer buffers incomplete sequences:

```rust
let mut healer = UTF8TokenHealer::new(true);

// Token 1: Partial sequence
let result1 = healer.process(&[0xF0, 0x9F])?;
assert_eq!(result1, None);  // Buffered

// Token 2: Complete sequence
let result2 = healer.process(&[0x91, 0x8D])?;
assert_eq!(result2, Some("👍".to_string()));  // Emitted!
```

### Features

- **Automatic Buffering**: Incomplete sequences held until complete
- **Partial Emission**: Emits valid prefix, buffers invalid suffix
- **Flush on End**: Handles remaining bytes at generation end
- **Zero Overhead**: Disabled when not needed

## Stop Sequence Detection

### Sliding Window Algorithm

Efficient detection across token boundaries:

```rust
let mut detector = StopSequenceDetector::new(vec!["</s>".to_string()]);

detector.check("Hello ");   // false
detector.check("world <");  // false
detector.check("/");        // false
detector.check("s>");       // true! (detected "</ s >" across tokens)
```

### Implementation

```
Window: [w, o, r, l, d,  , <]  max_len=4
Token:  "/"
Window: [o, r, l, d,  , <, /]  (shifted)
Token:  "s>"
Window: [l, d,  , <, /, s, >]  (shifted)
Check:  "ld </s>" contains "</s>"? YES!
```

### Performance

- O(1) per token (amortized)
- Memory: O(max_sequence_length)
- No regex overhead

## Performance Characteristics

### First-Token Latency

Optimizations:

1. **Prefill Caching**: Cache prompt processing
2. **Async Spawning**: Generation runs in background immediately
3. **Channel Buffering**: Pre-allocated channel reduces allocation overhead
4. **Zero-Copy Decoding**: Avoid string copies where possible

Typical latency: **< 5ms** (excluding model forward pass)

### Throughput

Optimizations:

1. **KV Cache**: Avoid recomputing past tokens
2. **Pipelined Generation**: Decode while generating next token
3. **Batch-Free**: Single-token generation minimizes padding waste
4. **Backpressure**: Channel buffer prevents unbounded memory growth

Typical throughput: **Limited by model forward pass**, not streaming overhead

### Memory Usage

- Channel buffer: `channel_buffer * sizeof(StreamEvent)` ≈ 100 KB default
- UTF-8 healer: ≤ 4 bytes (longest UTF-8 sequence)
- Stop detector: `max_sequence_length * sizeof(char)` ≈ 100 bytes
- KV cache: Model-dependent (managed separately)

## Testing

### Unit Tests

```bash
cargo test -p adapteros-lora-mlx-ffi --test streaming_tests
```

Tests cover:
- UTF-8 healing (emoji, mixed ASCII/UTF-8, invalid sequences)
- Stop sequence detection (simple, multiple, cross-boundary)
- SSE formatting (token, done, error, keep-alive)
- KV cache management (basic, multi-layer, clear)
- Streaming config (defaults, custom)

### Integration Tests

```bash
cargo test -p adapteros-lora-mlx-ffi --test streaming_tests -- --ignored
```

### Benchmarks

```bash
cargo test -p adapteros-lora-mlx-ffi --test streaming_tests benchmark
```

Measures:
- First-token latency
- Token throughput (tokens/sec)
- UTF-8 healing overhead
- Stop detection performance

## Examples

### Run Streaming Example

```bash
cargo run --example streaming_inference --features experimental-backends
```

Demonstrates:
- Real-time token generation
- SSE formatting
- UTF-8 healing
- Stop sequence detection
- Latency measurements

### Run SSE Server Example

```bash
cargo run --example streaming_server --features experimental-backends
```

Starts HTTP server with `/v1/chat/completions` endpoint for testing with:

```bash
curl -N -X POST http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "prompt": "Explain quantum computing:",
    "max_tokens": 100,
    "stream": true
  }'
```

## Troubleshooting

### Tokens Not Appearing

**Problem**: No tokens received from stream

**Solutions**:
1. Check channel buffer size (increase if generation is fast)
2. Verify generate_fn is being called
3. Check for errors in generation task
4. Enable tracing: `RUST_LOG=adapteros_lora_mlx_ffi=debug`

### Garbled Text

**Problem**: Invalid UTF-8 characters in output

**Solutions**:
1. Enable UTF-8 healing: `enable_utf8_healing: true`
2. Check tokenizer decode implementation
3. Verify token bytes are valid UTF-8

### Generation Hangs

**Problem**: Stream stops producing tokens

**Solutions**:
1. Check token_timeout setting
2. Verify stop sequences aren't triggering prematurely
3. Check model forward pass for deadlocks
4. Monitor channel capacity

### High Latency

**Problem**: First token takes too long

**Solutions**:
1. Profile model forward pass (likely bottleneck)
2. Check KV cache is enabled
3. Verify prompt is prefilled once
4. Monitor channel overhead

## API Reference

### MLXStreamingGenerator

```rust
impl MLXStreamingGenerator {
    pub fn new(
        config: StreamingConfig,
        base_seed: B3Hash,
        num_layers: usize,
    ) -> Self;

    pub async fn generate<F>(
        &mut self,
        generate_token_fn: F,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()>
    where
        F: FnMut(usize, &B3Hash) -> Result<(u32, Vec<u8>)>;

    pub fn kv_cache(&self) -> &KVCacheManager;
    pub fn kv_cache_mut(&mut self) -> &mut KVCacheManager;
}
```

### TokenStream

```rust
impl TokenStream {
    pub fn new(receiver: mpsc::Receiver<StreamEvent>) -> Self;

    pub async fn next_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Option<StreamEvent>;
}

impl Stream for TokenStream {
    type Item = StreamEvent;
    // Implements futures::Stream trait
}
```

### UTF8TokenHealer

```rust
impl UTF8TokenHealer {
    pub fn new(enabled: bool) -> Self;

    pub fn process(
        &mut self,
        token_bytes: &[u8],
    ) -> Result<Option<String>>;

    pub fn flush(&mut self) -> Result<Option<String>>;
}
```

### StopSequenceDetector

```rust
impl StopSequenceDetector {
    pub fn new(sequences: Vec<String>) -> Self;

    pub fn check(&mut self, text: &str) -> bool;
}
```

### SSEFormatter

```rust
impl SSEFormatter {
    pub fn format(event: &StreamEvent) -> String;
}
```

## Citations

- OpenAI Streaming API: https://platform.openai.com/docs/api-reference/streaming
- Server-Sent Events: https://html.spec.whatwg.org/multipage/server-sent-events.html
- UTF-8 Specification: https://www.rfc-editor.org/rfc/rfc3629
- KV Cache: Attention Is All You Need (Vaswani et al., 2017)

---

**Copyright:** © 2025 JKCA / James KC Auchterlonie. All rights reserved.

**Author:** James KC Auchterlonie

**Version:** 0.1.0 (Alpha)
