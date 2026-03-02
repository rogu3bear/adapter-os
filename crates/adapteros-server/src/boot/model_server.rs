//! Model Server readiness phase for adapterOS control plane.
//!
//! This module handles the validation of Model Server availability during boot.
//! When `model_server.enabled = true`, the server will block boot until the
//! Model Server is reachable and healthy.
//!
//! # Architecture
//!
//! Workers in Model Server mode connect to a shared Model Server for inference
//! instead of loading models locally. This boot phase ensures the Model Server
//! is available before workers attempt to connect, preventing cascading failures.
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_server::boot::model_server::check_model_server_readiness;
//!
//! // During boot, after config is loaded:
//! check_model_server_readiness(&config).await?;
//! ```
//!
//! # Configuration
//!
//! The phase is controlled by `model_server.enabled` in the configuration.
//! When disabled, the phase is a no-op and returns immediately.
//!
//! # Feature Flag
//!
//! This module requires the `model-server` feature to be enabled for the actual
//! health check implementation. Without the feature, the phase will skip if
//! Model Server is enabled in config and log a warning.

use adapteros_config::EffectiveConfig;
use adapteros_core::{AosError, Result};
use tracing::{debug, warn};

#[cfg(feature = "model-server")]
use tracing::info;

#[cfg(feature = "model-server")]
use adapteros_lora_worker::{ModelServerClient, ModelServerClientConfig};

#[cfg(feature = "model-server")]
use std::time::Duration;

/// Default timeout for each health check attempt
#[cfg(feature = "model-server")]
const HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Default retry interval between health check attempts
#[cfg(feature = "model-server")]
const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Default maximum number of retry attempts
#[cfg(feature = "model-server")]
const MAX_RETRY_ATTEMPTS: u32 = 30;

/// Context returned from the model server readiness phase.
#[derive(Debug, Clone)]
pub struct ModelServerContext {
    /// Whether Model Server mode is enabled
    pub enabled: bool,
    /// Model Server address (if enabled)
    pub server_addr: Option<String>,
}

/// Check Model Server readiness during boot.
///
/// This function performs the following:
/// 1. Checks if Model Server mode is enabled in configuration
/// 2. If disabled, returns immediately (no-op)
/// 3. If enabled, attempts to connect and verify health
/// 4. Retries with backoff until healthy or timeout
///
/// # Arguments
/// * `config` - The effective configuration
///
/// # Returns
/// * `Ok(ModelServerContext)` - Model Server is ready (or disabled)
/// * `Err(AosError::Config)` - Model Server not reachable after retries
///
/// # Errors
/// Returns an error if Model Server is enabled but not reachable after
/// the configured number of retry attempts.
///
/// # Feature Flag
///
/// When compiled without the `model-server` feature:
/// - If `model_server.enabled = false`, returns success (skip)
/// - If `model_server.enabled = true`, returns an error (feature not compiled in)
#[cfg(feature = "model-server")]
pub async fn check_model_server_readiness(config: &EffectiveConfig) -> Result<ModelServerContext> {
    // Check if Model Server mode is enabled
    if !config.model_server.enabled {
        debug!("Model Server mode disabled, skipping readiness check");
        return Ok(ModelServerContext {
            enabled: false,
            server_addr: None,
        });
    }

    let server_addr = config.model_server.server_addr.clone();
    info!(
        server_addr = %server_addr,
        "Model Server mode enabled, checking readiness"
    );

    // Create client with boot-appropriate timeouts
    let client_config = ModelServerClientConfig {
        server_addr: server_addr.clone(),
        connect_timeout: HEALTH_CHECK_TIMEOUT,
        request_timeout: HEALTH_CHECK_TIMEOUT,
        max_retries: 1, // We handle retries in this function
        retry_backoff_base: Duration::from_millis(100),
    };

    let client = ModelServerClient::new(client_config);

    // Status value for HEALTHY from the proto enum
    const STATUS_HEALTHY: i32 = 1;

    // Attempt health check with retries
    for attempt in 1..=MAX_RETRY_ATTEMPTS {
        match client.health().await {
            Ok(health_response) => {
                if health_response.status == STATUS_HEALTHY {
                    info!(
                        server_addr = %server_addr,
                        attempt = attempt,
                        model_id = %health_response.model_id,
                        uptime_seconds = health_response.uptime_seconds,
                        "Model Server ready"
                    );
                    return Ok(ModelServerContext {
                        enabled: true,
                        server_addr: Some(server_addr),
                    });
                } else {
                    warn!(
                        server_addr = %server_addr,
                        attempt = attempt,
                        status = health_response.status,
                        message = %health_response.message,
                        "Model Server reports non-healthy status"
                    );
                }
            }
            Err(e) => {
                if attempt < MAX_RETRY_ATTEMPTS {
                    debug!(
                        server_addr = %server_addr,
                        attempt = attempt,
                        max_attempts = MAX_RETRY_ATTEMPTS,
                        error = %e,
                        "Model Server health check failed, retrying"
                    );
                } else {
                    warn!(
                        server_addr = %server_addr,
                        attempt = attempt,
                        error = %e,
                        "Model Server health check failed (final attempt)"
                    );
                }
            }
        }

        if attempt < MAX_RETRY_ATTEMPTS {
            tokio::time::sleep(RETRY_INTERVAL).await;
        }
    }

    // All retries exhausted
    Err(AosError::Config(format!(
        "Model Server at {} not reachable after {}s. \
         Ensure Model Server is running and accessible. \
         Set model_server.enabled = false to disable this check.",
        server_addr, MAX_RETRY_ATTEMPTS
    )))
}

/// Fallback implementation when model-server feature is not enabled.
///
/// If Model Server mode is enabled in config but the feature isn't compiled in,
/// this returns an error to prevent silent misconfiguration.
#[cfg(not(feature = "model-server"))]
pub async fn check_model_server_readiness(config: &EffectiveConfig) -> Result<ModelServerContext> {
    if !config.model_server.enabled {
        debug!("Model Server mode disabled, skipping readiness check");
        return Ok(ModelServerContext {
            enabled: false,
            server_addr: None,
        });
    }

    // Model Server is enabled but feature is not compiled in
    warn!(
        server_addr = %config.model_server.server_addr,
        "Model Server mode enabled in config but 'model-server' feature not compiled in"
    );

    Err(AosError::Config(
        "Model Server mode is enabled in configuration but the 'model-server' feature \
         is not compiled in. Either disable model_server.enabled in config or rebuild \
         with --features model-server"
            .to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_server_context_debug() {
        let ctx = ModelServerContext {
            enabled: true,
            server_addr: Some("http://127.0.0.1:50051".to_string()),
        };
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("enabled: true"));
        assert!(debug_str.contains("127.0.0.1:50051"));
    }

    #[test]
    fn test_model_server_context_clone() {
        let ctx = ModelServerContext {
            enabled: false,
            server_addr: None,
        };
        let cloned = ctx.clone();
        assert!(!cloned.enabled);
        assert!(cloned.server_addr.is_none());
    }
}
