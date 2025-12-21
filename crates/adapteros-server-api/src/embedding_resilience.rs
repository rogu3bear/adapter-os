//! Embedding resilience module providing circuit breaker and timeout protection.
//!
//! This module wraps embedding generation with:
//! - Per-chunk timeout protection
//! - Circuit breaker to prevent cascade failures
//! - Configurable skip-on-failure behavior
//! - Detailed tracking of successful and failed chunks

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{info, warn};

use adapteros_core::AosError;

/// Configuration for embedding resilience behavior
#[derive(Debug, Clone)]
pub struct EmbeddingResilienceConfig {
    /// Maximum consecutive failures before opening circuit
    pub failure_threshold: usize,
    /// Maximum percentage of chunks that can fail before aborting (0.0-1.0)
    pub max_failure_rate: f32,
    /// Timeout for individual embedding operations
    pub embedding_timeout: Duration,
    /// Whether to skip failed chunks or abort the entire batch
    pub skip_on_failure: bool,
    /// Time to wait before attempting to close circuit after opening
    pub circuit_reset_timeout: Duration,
}

impl Default for EmbeddingResilienceConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            max_failure_rate: 0.2,
            embedding_timeout: Duration::from_secs(30),
            skip_on_failure: true,
            circuit_reset_timeout: Duration::from_secs(60),
        }
    }
}

/// Result of processing a single chunk
#[derive(Debug, Clone)]
pub struct ChunkEmbeddingResult {
    pub chunk_index: usize,
    pub embedding: Vec<f32>,
}

/// Information about a failed chunk
#[derive(Debug, Clone)]
pub struct FailedChunk {
    pub chunk_index: usize,
    pub error: String,
    pub is_timeout: bool,
}

/// Result of batch embedding with resilience
#[derive(Debug)]
pub struct EmbeddingBatchResult {
    pub successful: Vec<ChunkEmbeddingResult>,
    pub failed: Vec<FailedChunk>,
    pub circuit_opened: bool,
    pub total_duration: Duration,
}

impl EmbeddingBatchResult {
    pub fn success_rate(&self) -> f32 {
        let total = self.successful.len() + self.failed.len();
        if total == 0 {
            1.0
        } else {
            self.successful.len() as f32 / total as f32
        }
    }

    pub fn is_partial_success(&self) -> bool {
        !self.successful.is_empty() && !self.failed.is_empty()
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

/// Circuit breaker for embedding operations
pub struct EmbeddingCircuitBreaker {
    config: EmbeddingResilienceConfig,
    consecutive_failures: AtomicUsize,
    state: RwLock<CircuitState>,
    last_failure_time: RwLock<Option<Instant>>,
}

impl EmbeddingCircuitBreaker {
    pub fn new(config: EmbeddingResilienceConfig) -> Self {
        Self {
            config,
            consecutive_failures: AtomicUsize::new(0),
            state: RwLock::new(CircuitState::Closed),
            last_failure_time: RwLock::new(None),
        }
    }

    pub async fn state(&self) -> CircuitState {
        let mut state = self.state.write().await;

        // Check if we should transition from Open to HalfOpen
        if *state == CircuitState::Open {
            let last_failure = self.last_failure_time.read().await;
            if let Some(time) = *last_failure {
                if time.elapsed() >= self.config.circuit_reset_timeout {
                    *state = CircuitState::HalfOpen;
                    info!("Embedding circuit breaker transitioning to half-open");
                }
            }
        }

        *state
    }

    pub async fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::SeqCst);
        let mut state = self.state.write().await;
        if *state == CircuitState::HalfOpen {
            *state = CircuitState::Closed;
            info!("Embedding circuit breaker closed after successful operation");
        }
    }

    pub async fn record_failure(&self) {
        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure_time.write().await = Some(Instant::now());

        if failures >= self.config.failure_threshold {
            let mut state = self.state.write().await;
            if *state == CircuitState::Closed {
                *state = CircuitState::Open;
                warn!(
                    failures = failures,
                    threshold = self.config.failure_threshold,
                    "Embedding circuit breaker opened after consecutive failures"
                );
            }
        }
    }

    pub async fn is_open(&self) -> bool {
        self.state().await == CircuitState::Open
    }
}

/// Trait for embedding models (to be implemented by actual models)
#[async_trait::async_trait]
pub trait ResilientEmbeddingModel: Send + Sync {
    async fn encode_text(&self, text: &str) -> Result<Vec<f32>, AosError>;
}

/// Process chunks with circuit breaker protection
pub async fn process_chunks_with_resilience<M: ResilientEmbeddingModel>(
    chunks: &[(usize, String)], // (chunk_index, text)
    embedding_model: &M,
    config: &EmbeddingResilienceConfig,
    circuit_breaker: &EmbeddingCircuitBreaker,
) -> Result<EmbeddingBatchResult, AosError> {
    let start_time = Instant::now();
    let mut successful = Vec::new();
    let mut failed = Vec::new();
    let total_chunks = chunks.len();

    for (chunk_index, text) in chunks {
        // Check circuit breaker state
        if circuit_breaker.is_open().await {
            warn!(
                chunk_index = chunk_index,
                remaining = total_chunks - successful.len() - failed.len(),
                "Circuit breaker open, aborting remaining chunks"
            );
            return Ok(EmbeddingBatchResult {
                successful,
                failed,
                circuit_opened: true,
                total_duration: start_time.elapsed(),
            });
        }

        // Apply timeout to embedding generation
        let result =
            tokio::time::timeout(config.embedding_timeout, embedding_model.encode_text(text)).await;

        match result {
            Ok(Ok(embedding)) => {
                circuit_breaker.record_success().await;
                successful.push(ChunkEmbeddingResult {
                    chunk_index: *chunk_index,
                    embedding,
                });
            }
            Ok(Err(e)) => {
                circuit_breaker.record_failure().await;
                let error_msg = e.to_string();
                warn!(
                    chunk_index = chunk_index,
                    error = %error_msg,
                    "Embedding generation failed"
                );

                failed.push(FailedChunk {
                    chunk_index: *chunk_index,
                    error: error_msg,
                    is_timeout: false,
                });

                // Check failure rate threshold
                let failure_rate = failed.len() as f32 / total_chunks as f32;
                if failure_rate > config.max_failure_rate && !config.skip_on_failure {
                    return Err(AosError::Rag(format!(
                        "Embedding failure rate {:.1}% exceeded threshold {:.1}%",
                        failure_rate * 100.0,
                        config.max_failure_rate * 100.0
                    )));
                }

                if !config.skip_on_failure {
                    return Err(AosError::Rag(format!(
                        "Embedding failed for chunk {}: {}",
                        chunk_index,
                        failed.last().unwrap().error
                    )));
                }
            }
            Err(_) => {
                // Timeout
                circuit_breaker.record_failure().await;
                warn!(
                    chunk_index = chunk_index,
                    timeout_secs = config.embedding_timeout.as_secs(),
                    "Embedding generation timed out"
                );

                failed.push(FailedChunk {
                    chunk_index: *chunk_index,
                    error: format!("Timeout after {:?}", config.embedding_timeout),
                    is_timeout: true,
                });

                if !config.skip_on_failure {
                    return Err(AosError::Rag(format!(
                        "Embedding timed out for chunk {} after {:?}",
                        chunk_index, config.embedding_timeout
                    )));
                }
            }
        }
    }

    Ok(EmbeddingBatchResult {
        successful,
        failed,
        circuit_opened: false,
        total_duration: start_time.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_circuit_breaker_opens_after_threshold() {
        let config = EmbeddingResilienceConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let cb = EmbeddingCircuitBreaker::new(config);

        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Closed);

        cb.record_failure().await;
        assert_eq!(cb.state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_resets_on_success() {
        let config = EmbeddingResilienceConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let cb = EmbeddingCircuitBreaker::new(config);

        cb.record_failure().await;
        cb.record_success().await;
        cb.record_failure().await;

        // Should still be closed because success reset the counter
        assert_eq!(cb.state().await, CircuitState::Closed);
    }
}
