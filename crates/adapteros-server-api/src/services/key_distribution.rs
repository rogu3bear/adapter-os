//! Key distribution service for pushing key updates to workers.
//!
//! This service handles the control plane side of key rotation:
//! 1. Generates new signing keys
//! 2. Creates signed key update messages
//! 3. Pushes updates to workers via UDS
//!
//! ## Security Model
//!
//! Key updates are signed by the OLD key to prove authenticity:
//! - Workers trust the current key
//! - Update is signed by the current (old) key
//! - Workers verify signature before accepting new key
//! - Old key remains valid for grace period
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_server_api::services::key_distribution::KeyDistributionService;
//!
//! // Create service with current signing key
//! let service = KeyDistributionService::new(
//!     signing_key,
//!     worker_registry,
//!     300, // grace period in seconds
//! );
//!
//! // Rotate key and distribute to workers
//! let receipt = service.rotate_and_distribute().await?;
//! ```

use std::sync::Arc;
use std::time::Duration;

use ed25519_dalek::{SigningKey, VerifyingKey};
use futures_util::future::join_all;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::{debug, error, info, warn};

use adapteros_boot::{
    derive_kid_from_verifying_key, KeyUpdateRequest, KeyUpdateResponse,
    DEFAULT_ROTATION_GRACE_PERIOD_SECS,
};

/// Result of a key rotation operation.
#[derive(Debug, Clone)]
pub struct KeyRotationReceipt {
    /// Key ID of the old (now deprecated) key
    pub old_kid: String,
    /// Key ID of the new current key
    pub new_kid: String,
    /// Number of workers that received the update
    pub workers_updated: usize,
    /// Number of workers that failed to receive the update
    pub workers_failed: usize,
    /// Grace period in seconds
    pub grace_period_secs: u64,
}

/// Per-worker update result.
#[derive(Debug, Clone)]
pub struct WorkerUpdateResult {
    /// Worker ID
    pub worker_id: String,
    /// Whether the update succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// New key count on worker (if success)
    pub key_count: Option<usize>,
}

/// Worker registry interface for discovering workers.
pub trait WorkerRegistry: Send + Sync {
    /// Get all worker UDS socket paths.
    fn get_worker_sockets(&self) -> Vec<WorkerEndpoint>;
}

/// Worker endpoint information.
#[derive(Debug, Clone)]
pub struct WorkerEndpoint {
    /// Worker ID
    pub worker_id: String,
    /// UDS socket path
    pub socket_path: String,
}

/// Service for distributing key updates to workers.
pub struct KeyDistributionService {
    /// Current signing key (will become "old" key after rotation)
    current_signing_key: Arc<SigningKey>,
    /// Current key ID
    current_kid: String,
    /// Worker registry for discovering workers
    worker_registry: Arc<dyn WorkerRegistry>,
    /// Default grace period for old keys
    default_grace_period_secs: u64,
    /// Timeout for worker communication
    worker_timeout: Duration,
}

impl KeyDistributionService {
    /// Create a new key distribution service.
    ///
    /// # Arguments
    ///
    /// * `signing_key` - Current signing key for generating tokens
    /// * `worker_registry` - Registry for discovering workers
    /// * `grace_period_secs` - Default grace period for old keys
    pub fn new(
        signing_key: Arc<SigningKey>,
        worker_registry: Arc<dyn WorkerRegistry>,
        grace_period_secs: u64,
    ) -> Self {
        let kid = derive_kid_from_verifying_key(&signing_key.verifying_key());
        Self {
            current_signing_key: signing_key,
            current_kid: kid,
            worker_registry,
            default_grace_period_secs: grace_period_secs,
            worker_timeout: Duration::from_secs(30),
        }
    }

    /// Create with default grace period.
    pub fn with_defaults(
        signing_key: Arc<SigningKey>,
        worker_registry: Arc<dyn WorkerRegistry>,
    ) -> Self {
        Self::new(
            signing_key,
            worker_registry,
            DEFAULT_ROTATION_GRACE_PERIOD_SECS,
        )
    }

    /// Get the current key ID.
    pub fn current_kid(&self) -> &str {
        &self.current_kid
    }

    /// Rotate the signing key and distribute to all workers.
    ///
    /// This:
    /// 1. Generates a new signing key
    /// 2. Creates a signed update message (signed by old key)
    /// 3. Pushes the update to all registered workers
    /// 4. Returns a receipt with results
    ///
    /// # Returns
    ///
    /// Receipt containing the old/new key IDs and per-worker results.
    pub async fn rotate_and_distribute(&mut self) -> Result<KeyRotationReceipt, anyhow::Error> {
        self.rotate_and_distribute_with_grace(self.default_grace_period_secs)
            .await
    }

    /// Rotate with custom grace period.
    pub async fn rotate_and_distribute_with_grace(
        &mut self,
        grace_period_secs: u64,
    ) -> Result<KeyRotationReceipt, anyhow::Error> {
        // Generate new signing key
        let new_signing_key = SigningKey::generate(&mut rand::thread_rng());
        let new_verifying_key = new_signing_key.verifying_key();
        let new_kid = derive_kid_from_verifying_key(&new_verifying_key);

        let old_kid = self.current_kid.clone();

        info!(
            old_kid = %old_kid,
            new_kid = %new_kid,
            grace_period_secs = grace_period_secs,
            "Initiating key rotation"
        );

        // Create signed update request
        let update_request = KeyUpdateRequest::new(
            &self.current_signing_key,
            &old_kid,
            &new_verifying_key,
            &new_kid,
            grace_period_secs,
        )?;

        // Get all worker endpoints
        let workers = self.worker_registry.get_worker_sockets();
        let worker_count = workers.len();

        info!(
            worker_count = worker_count,
            "Distributing key update to workers"
        );

        // Distribute to all workers concurrently
        let results = self.distribute_to_workers(&workers, &update_request).await;

        // Count successes and failures
        let (successes, failures): (Vec<_>, Vec<_>) = results.iter().partition(|r| r.success);

        let workers_updated = successes.len();
        let workers_failed = failures.len();

        // Log failures
        for failure in &failures {
            warn!(
                worker_id = %failure.worker_id,
                error = ?failure.error,
                "Failed to distribute key update to worker"
            );
        }

        // Update internal state only if at least one worker succeeded
        if workers_updated > 0 {
            self.current_signing_key = Arc::new(new_signing_key);
            self.current_kid = new_kid.clone();

            info!(
                new_kid = %new_kid,
                workers_updated = workers_updated,
                workers_failed = workers_failed,
                "Key rotation completed"
            );
        } else if worker_count > 0 {
            error!(
                workers_failed = workers_failed,
                "Key rotation failed - no workers received update"
            );
            return Err(anyhow::anyhow!(
                "Key rotation failed: no workers received update"
            ));
        } else {
            // No workers registered - still update internal state
            self.current_signing_key = Arc::new(new_signing_key);
            self.current_kid = new_kid.clone();

            warn!("Key rotation completed but no workers are registered");
        }

        Ok(KeyRotationReceipt {
            old_kid,
            new_kid: self.current_kid.clone(),
            workers_updated,
            workers_failed,
            grace_period_secs,
        })
    }

    /// Distribute key update to all workers.
    async fn distribute_to_workers(
        &self,
        workers: &[WorkerEndpoint],
        update_request: &KeyUpdateRequest,
    ) -> Vec<WorkerUpdateResult> {
        let futures: Vec<_> = workers
            .iter()
            .map(|worker| self.send_update_to_worker(worker, update_request))
            .collect();

        join_all(futures).await
    }

    /// Send key update to a single worker.
    async fn send_update_to_worker(
        &self,
        worker: &WorkerEndpoint,
        update_request: &KeyUpdateRequest,
    ) -> WorkerUpdateResult {
        debug!(
            worker_id = %worker.worker_id,
            socket_path = %worker.socket_path,
            "Sending key update to worker"
        );

        match self
            .send_update_to_worker_inner(worker, update_request)
            .await
        {
            Ok(response) => {
                if response.success {
                    debug!(
                        worker_id = %worker.worker_id,
                        new_kid = ?response.new_kid,
                        key_count = response.key_count,
                        "Worker accepted key update"
                    );
                    WorkerUpdateResult {
                        worker_id: worker.worker_id.clone(),
                        success: true,
                        error: None,
                        key_count: Some(response.key_count),
                    }
                } else {
                    WorkerUpdateResult {
                        worker_id: worker.worker_id.clone(),
                        success: false,
                        error: response.error,
                        key_count: Some(response.key_count),
                    }
                }
            }
            Err(e) => WorkerUpdateResult {
                worker_id: worker.worker_id.clone(),
                success: false,
                error: Some(e.to_string()),
                key_count: None,
            },
        }
    }

    /// Inner implementation of worker update.
    async fn send_update_to_worker_inner(
        &self,
        worker: &WorkerEndpoint,
        update_request: &KeyUpdateRequest,
    ) -> Result<KeyUpdateResponse, anyhow::Error> {
        // Connect to worker UDS
        let mut stream = tokio::time::timeout(
            self.worker_timeout,
            UnixStream::connect(&worker.socket_path),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Connection timeout"))?
        .map_err(|e| anyhow::anyhow!("Connection failed: {}", e))?;

        // Serialize request
        let body = serde_json::to_string(update_request)?;

        // Build HTTP request
        let http_request = format!(
            "POST /key/update HTTP/1.1\r\n\
             Host: localhost\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            body.len(),
            body
        );

        // Send request
        tokio::time::timeout(
            self.worker_timeout,
            stream.write_all(http_request.as_bytes()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Write timeout"))?
        .map_err(|e| anyhow::anyhow!("Write failed: {}", e))?;

        // Read response (simple HTTP parsing)
        let mut response_buf = vec![0u8; 4096];
        let n = tokio::time::timeout(self.worker_timeout, stream.read(&mut response_buf))
            .await
            .map_err(|_| anyhow::anyhow!("Read timeout"))?
            .map_err(|e| anyhow::anyhow!("Read failed: {}", e))?;

        let response_str = String::from_utf8_lossy(&response_buf[..n]);

        // Find JSON body (after \r\n\r\n)
        let body_start = response_str
            .find("\r\n\r\n")
            .ok_or_else(|| anyhow::anyhow!("Invalid HTTP response"))?
            + 4;

        let json_body = &response_str[body_start..];
        let response: KeyUpdateResponse = serde_json::from_str(json_body).map_err(|e| {
            anyhow::anyhow!("Failed to parse response: {} (body: {})", e, json_body)
        })?;

        Ok(response)
    }

    /// Get the new signing key after rotation (for updating AppState).
    pub fn signing_key(&self) -> Arc<SigningKey> {
        Arc::clone(&self.current_signing_key)
    }

    /// Get the new verifying key after rotation.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.current_signing_key.verifying_key()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockWorkerRegistry {
        workers: Vec<WorkerEndpoint>,
    }

    impl WorkerRegistry for MockWorkerRegistry {
        fn get_worker_sockets(&self) -> Vec<WorkerEndpoint> {
            self.workers.clone()
        }
    }

    #[test]
    fn test_service_creation() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let registry = Arc::new(MockWorkerRegistry { workers: vec![] });

        let service = KeyDistributionService::new(Arc::new(signing_key), registry, 300);

        assert!(!service.current_kid().is_empty());
    }

    #[test]
    fn test_kid_derivation() {
        let signing_key = SigningKey::generate(&mut rand::thread_rng());
        let expected_kid = derive_kid_from_verifying_key(&signing_key.verifying_key());

        let registry = Arc::new(MockWorkerRegistry { workers: vec![] });
        let service = KeyDistributionService::new(Arc::new(signing_key), registry, 300);

        assert_eq!(service.current_kid(), expected_kid);
    }
}
