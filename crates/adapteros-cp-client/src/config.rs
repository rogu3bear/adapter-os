//! Configuration for Control Plane client

use std::time::Duration;

use crate::error::{Result, WorkerCpError};

/// Policy for handling repeated heartbeat failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatFailurePolicy {
    /// Continue serving even if heartbeats fail (log warnings only)
    Continue,
    /// Exit the worker process after N consecutive failures
    ExitAfter(u32),
}

impl Default for HeartbeatFailurePolicy {
    fn default() -> Self {
        HeartbeatFailurePolicy::Continue
    }
}

/// Configuration for the Control Plane client
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the control plane (e.g., "http://127.0.0.1:8080")
    pub base_url: String,

    /// Timeout for worker registration requests
    pub registration_timeout: Duration,

    /// Timeout for status notification requests
    pub status_timeout: Duration,

    /// Timeout for heartbeat requests
    pub heartbeat_timeout: Duration,

    /// Timeout for fatal error reporting (panic hook)
    pub fatal_timeout: Duration,

    /// Maximum number of retry attempts for retryable errors
    pub max_retries: u32,

    /// Initial delay between retries (doubles with each attempt)
    pub initial_retry_delay: Duration,

    /// Maximum delay between retries
    pub max_retry_delay: Duration,

    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,

    /// Optional authentication token for future auth support
    pub auth_token: Option<String>,

    /// Policy for handling heartbeat failures
    pub heartbeat_failure_policy: HeartbeatFailurePolicy,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".to_string(),
            registration_timeout: Duration::from_secs(10),
            status_timeout: Duration::from_secs(5),
            heartbeat_timeout: Duration::from_secs(5),
            fatal_timeout: Duration::from_secs(3),
            max_retries: 3,
            initial_retry_delay: Duration::from_millis(100),
            max_retry_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            auth_token: None,
            heartbeat_failure_policy: HeartbeatFailurePolicy::default(),
        }
    }
}

impl ClientConfig {
    /// Create a new builder for ClientConfig
    pub fn builder() -> ClientConfigBuilder {
        ClientConfigBuilder::default()
    }

    /// Create config from environment variables
    ///
    /// Reads:
    /// - `AOS_CP_URL` - Control plane base URL
    /// - `AOS_CP_AUTH_TOKEN` - Optional auth token
    /// - `AOS_CP_MAX_RETRIES` - Max retry attempts
    /// - `AOS_HEARTBEAT_FAILURE_MODE` - "continue" or "exit:N"
    pub fn from_env() -> Result<Self> {
        let mut config = Self::default();

        if let Ok(url) = std::env::var("AOS_CP_URL") {
            config.base_url = url;
        }

        if let Ok(token) = std::env::var("AOS_CP_AUTH_TOKEN") {
            config.auth_token = Some(token);
        }

        if let Ok(retries) = std::env::var("AOS_CP_MAX_RETRIES") {
            config.max_retries = retries
                .parse()
                .map_err(|e| WorkerCpError::Config(format!("Invalid max_retries: {}", e)))?;
        }

        if let Ok(mode) = std::env::var("AOS_HEARTBEAT_FAILURE_MODE") {
            config.heartbeat_failure_policy = parse_heartbeat_policy(&mode)?;
        }

        Ok(config)
    }
}

/// Builder for ClientConfig
#[derive(Debug, Default)]
pub struct ClientConfigBuilder {
    config: ClientConfig,
}

impl ClientConfigBuilder {
    /// Set the control plane base URL
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.config.base_url = url.into();
        self
    }

    /// Set the registration timeout
    pub fn registration_timeout(mut self, timeout: Duration) -> Self {
        self.config.registration_timeout = timeout;
        self
    }

    /// Set the status notification timeout
    pub fn status_timeout(mut self, timeout: Duration) -> Self {
        self.config.status_timeout = timeout;
        self
    }

    /// Set the heartbeat timeout
    pub fn heartbeat_timeout(mut self, timeout: Duration) -> Self {
        self.config.heartbeat_timeout = timeout;
        self
    }

    /// Set the fatal error reporting timeout
    pub fn fatal_timeout(mut self, timeout: Duration) -> Self {
        self.config.fatal_timeout = timeout;
        self
    }

    /// Set the maximum number of retries
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Set the initial retry delay
    pub fn initial_retry_delay(mut self, delay: Duration) -> Self {
        self.config.initial_retry_delay = delay;
        self
    }

    /// Set the maximum retry delay
    pub fn max_retry_delay(mut self, delay: Duration) -> Self {
        self.config.max_retry_delay = delay;
        self
    }

    /// Set the backoff multiplier
    pub fn backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.config.backoff_multiplier = multiplier;
        self
    }

    /// Set the authentication token
    pub fn auth_token(mut self, token: impl Into<String>) -> Self {
        self.config.auth_token = Some(token.into());
        self
    }

    /// Set the heartbeat failure policy
    pub fn heartbeat_failure_policy(mut self, policy: HeartbeatFailurePolicy) -> Self {
        self.config.heartbeat_failure_policy = policy;
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<ClientConfig> {
        // Validate base_url
        if self.config.base_url.is_empty() {
            return Err(WorkerCpError::Config(
                "base_url cannot be empty".to_string(),
            ));
        }

        // Validate base_url format (basic check)
        if !self.config.base_url.starts_with("http://")
            && !self.config.base_url.starts_with("https://")
        {
            return Err(WorkerCpError::Config(
                "base_url must start with http:// or https://".to_string(),
            ));
        }

        Ok(self.config)
    }
}

/// Parse heartbeat failure policy from string
fn parse_heartbeat_policy(s: &str) -> Result<HeartbeatFailurePolicy> {
    let s = s.trim().to_lowercase();

    if s == "continue" {
        return Ok(HeartbeatFailurePolicy::Continue);
    }

    if let Some(n) = s.strip_prefix("exit:") {
        let count: u32 = n
            .parse()
            .map_err(|e| WorkerCpError::Config(format!("Invalid exit count: {}", e)))?;
        return Ok(HeartbeatFailurePolicy::ExitAfter(count));
    }

    Err(WorkerCpError::Config(format!(
        "Invalid heartbeat failure mode: '{}'. Expected 'continue' or 'exit:N'",
        s
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClientConfig::default();
        assert_eq!(config.base_url, "http://127.0.0.1:8080");
        assert_eq!(config.registration_timeout, Duration::from_secs(10));
        assert_eq!(config.max_retries, 3);
        assert_eq!(
            config.heartbeat_failure_policy,
            HeartbeatFailurePolicy::Continue
        );
    }

    #[test]
    fn test_builder() {
        let config = ClientConfig::builder()
            .base_url("http://localhost:9000")
            .max_retries(5)
            .auth_token("secret")
            .heartbeat_failure_policy(HeartbeatFailurePolicy::ExitAfter(10))
            .build()
            .unwrap();

        assert_eq!(config.base_url, "http://localhost:9000");
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.auth_token, Some("secret".to_string()));
        assert_eq!(
            config.heartbeat_failure_policy,
            HeartbeatFailurePolicy::ExitAfter(10)
        );
    }

    #[test]
    fn test_builder_validation() {
        let result = ClientConfig::builder().base_url("").build();
        assert!(result.is_err());

        let result = ClientConfig::builder().base_url("not-a-url").build();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_heartbeat_policy() {
        assert_eq!(
            parse_heartbeat_policy("continue").unwrap(),
            HeartbeatFailurePolicy::Continue
        );
        assert_eq!(
            parse_heartbeat_policy("exit:5").unwrap(),
            HeartbeatFailurePolicy::ExitAfter(5)
        );
        assert!(parse_heartbeat_policy("invalid").is_err());
    }
}
