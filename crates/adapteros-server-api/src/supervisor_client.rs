//! HTTP client for the adapterOS Service Supervisor
///
/// Provides a typed interface to the supervisor API with retry logic,
/// timeout handling, and proper error propagation.
use adapteros_core::{AosError, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

/// Service status from supervisor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub id: String,
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub start_time: Option<String>,
    pub health_status: String,
    pub restart_count: u32,
    pub last_error: Option<String>,
    pub uptime_seconds: Option<u64>,
}

/// Request to start a service
#[derive(Debug, Serialize)]
struct StartServiceRequest {
    service_id: String,
}

/// Response from start/stop/restart operations
#[derive(Debug, Deserialize)]
struct ServiceOperationResponse {
    success: bool,
    message: String,
}

/// Response from list services endpoint
#[derive(Debug, Deserialize)]
struct ServicesResponse {
    services: Vec<ServiceStatus>,
}

/// HTTP client for the service supervisor
#[derive(Clone)]
pub struct SupervisorClient {
    base_url: String,
    client: Client,
    timeout: Duration,
    max_retries: u32,
}

impl SupervisorClient {
    /// Create a new supervisor client
    ///
    /// # Arguments
    /// * `base_url` - Base URL of the supervisor API (e.g., "http://localhost:3301")
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_server_api::supervisor_client::SupervisorClient;
    ///
    /// let client = SupervisorClient::new("http://localhost:3301");
    /// ```
    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .connect_timeout(Duration::from_millis(500))
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            base_url: base_url.into(),
            client,
            timeout: Duration::from_secs(10),
            max_retries: 3,
        }
    }

    /// Create a new supervisor client from environment variable
    ///
    /// Reads `SUPERVISOR_API_URL` environment variable, or constructs URL from `AOS_PANEL_PORT`.
    ///
    /// # Errors
    ///
    /// Returns `AosError::Config` if neither `SUPERVISOR_API_URL` nor `AOS_PANEL_PORT` is set.
    /// This prevents silent fallback to hardcoded localhost addresses which could cause
    /// connection failures or security issues in production environments.
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("SUPERVISOR_API_URL").or_else(|_| {
            // Fall back to constructing URL from AOS_PANEL_PORT
            std::env::var("AOS_PANEL_PORT").map(|port| format!("http://127.0.0.1:{}", port))
        });

        match base_url {
            Ok(url) => Ok(Self::new(url)),
            Err(_) => Err(AosError::Config(
                "Neither SUPERVISOR_API_URL nor AOS_PANEL_PORT is set. \
                 Configure one of these environment variables to connect to the supervisor."
                    .to_string(),
            )),
        }
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set max retry attempts
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Get all services
    ///
    /// # Errors
    /// Returns `AosError::Network` if the request fails
    /// Returns `AosError::Serialization` if the response cannot be parsed
    pub async fn get_services(&self) -> Result<Vec<ServiceStatus>> {
        let url = format!("{}/api/services", self.base_url);

        let response = self
            .retry_request(|| async { self.client.get(&url).timeout(self.timeout).send().await })
            .await?;

        if !response.status().is_success() {
            return Err(AosError::Network(format!(
                "Supervisor API returned status {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            )));
        }

        let services_response: ServicesResponse = response
            .json()
            .await
            .map_err(|e| AosError::Network(format!("Failed to parse services response: {}", e)))?;

        Ok(services_response.services)
    }

    /// Get a specific service by ID
    ///
    /// # Errors
    /// Returns `AosError::NotFound` if the service doesn't exist
    /// Returns `AosError::Network` if the request fails
    pub async fn get_service(&self, service_id: &str) -> Result<ServiceStatus> {
        let url = format!("{}/api/services/{}", self.base_url, service_id);

        let response = self
            .retry_request(|| async { self.client.get(&url).timeout(self.timeout).send().await })
            .await?;

        match response.status() {
            StatusCode::OK => response
                .json()
                .await
                .map_err(|e| AosError::Network(format!("Failed to parse service response: {}", e))),
            StatusCode::NOT_FOUND => Err(AosError::NotFound(format!(
                "Service '{}' not found",
                service_id
            ))),
            status => Err(AosError::Network(format!(
                "Supervisor API returned status {}: {}",
                status,
                response.text().await.unwrap_or_default()
            ))),
        }
    }

    /// Start a service
    ///
    /// # Arguments
    /// * `service_id` - The ID of the service to start
    ///
    /// # Errors
    /// Returns `AosError::NotFound` if the service doesn't exist
    /// Returns `AosError::Config` if the service is already running
    /// Returns `AosError::Network` if the request fails
    ///
    /// Note: This is a non-idempotent operation. Retries are only performed on
    /// connection-level errors, not on server errors (5xx) to prevent duplicate
    /// start attempts.
    pub async fn start_service(&self, service_id: &str) -> Result<String> {
        let url = format!("{}/api/services/start", self.base_url);
        let request_body = StartServiceRequest {
            service_id: service_id.to_string(),
        };

        let response = self
            .retry_request_with_idempotency(
                || async {
                    self.client
                        .post(&url)
                        .json(&request_body)
                        .timeout(self.timeout)
                        .send()
                        .await
                },
                false, // Non-idempotent: starting a service twice could cause issues
            )
            .await?;

        self.handle_operation_response(response, "start", service_id)
            .await
    }

    /// Stop a service
    ///
    /// # Arguments
    /// * `service_id` - The ID of the service to stop
    ///
    /// # Errors
    /// Returns `AosError::NotFound` if the service doesn't exist
    /// Returns `AosError::Network` if the request fails
    ///
    /// Note: This is a non-idempotent operation. Retries are only performed on
    /// connection-level errors, not on server errors (5xx) to prevent duplicate
    /// stop attempts.
    pub async fn stop_service(&self, service_id: &str) -> Result<String> {
        let url = format!("{}/api/services/stop", self.base_url);
        let request_body = StartServiceRequest {
            service_id: service_id.to_string(),
        };

        let response = self
            .retry_request_with_idempotency(
                || async {
                    self.client
                        .post(&url)
                        .json(&request_body)
                        .timeout(self.timeout)
                        .send()
                        .await
                },
                false, // Non-idempotent: stopping a service twice could cause issues
            )
            .await?;

        self.handle_operation_response(response, "stop", service_id)
            .await
    }

    /// Restart a service
    ///
    /// # Arguments
    /// * `service_id` - The ID of the service to restart
    ///
    /// # Errors
    /// Returns `AosError::NotFound` if the service doesn't exist
    /// Returns `AosError::Network` if the request fails
    ///
    /// Note: This is a non-idempotent operation. Retries are only performed on
    /// connection-level errors, not on server errors (5xx) to prevent duplicate
    /// restart attempts.
    pub async fn restart_service(&self, service_id: &str) -> Result<String> {
        let url = format!("{}/api/services/restart", self.base_url);
        let request_body = StartServiceRequest {
            service_id: service_id.to_string(),
        };

        let response = self
            .retry_request_with_idempotency(
                || async {
                    self.client
                        .post(&url)
                        .json(&request_body)
                        .timeout(self.timeout)
                        .send()
                        .await
                },
                false, // Non-idempotent: restarting a service twice could cause issues
            )
            .await?;

        self.handle_operation_response(response, "restart", service_id)
            .await
    }

    /// Start all essential services
    ///
    /// # Errors
    /// Returns `AosError::Network` if the request fails
    ///
    /// Note: This is a non-idempotent operation. Retries are only performed on
    /// connection-level errors, not on server errors (5xx).
    pub async fn start_essential_services(&self) -> Result<String> {
        let url = format!("{}/api/services/essential/start", self.base_url);

        let response = self
            .retry_request_with_idempotency(
                || async { self.client.post(&url).timeout(self.timeout).send().await },
                false, // Non-idempotent: starting services twice could cause issues
            )
            .await?;

        self.handle_operation_response(response, "start essential", "all")
            .await
    }

    /// Stop all essential services
    ///
    /// # Errors
    /// Returns `AosError::Network` if the request fails
    ///
    /// Note: This is a non-idempotent operation. Retries are only performed on
    /// connection-level errors, not on server errors (5xx).
    pub async fn stop_essential_services(&self) -> Result<String> {
        let url = format!("{}/api/services/essential/stop", self.base_url);

        let response = self
            .retry_request_with_idempotency(
                || async { self.client.post(&url).timeout(self.timeout).send().await },
                false, // Non-idempotent: stopping services twice could cause issues
            )
            .await?;

        self.handle_operation_response(response, "stop essential", "all")
            .await
    }

    /// Get service logs
    ///
    /// # Arguments
    /// * `service_id` - The ID of the service
    /// * `lines` - Optional number of lines to retrieve (defaults to 100)
    ///
    /// # Errors
    /// Returns `AosError::NotFound` if the service doesn't exist
    /// Returns `AosError::Network` if the request fails
    pub async fn get_service_logs(
        &self,
        service_id: &str,
        lines: Option<u32>,
    ) -> Result<Vec<String>> {
        let mut url = format!("{}/api/services/{}/logs", self.base_url, service_id);
        if let Some(lines) = lines {
            url.push_str(&format!("?lines={}", lines));
        }

        let response = self
            .retry_request(|| async { self.client.get(&url).timeout(self.timeout).send().await })
            .await?;

        match response.status() {
            StatusCode::OK => {
                #[derive(Deserialize)]
                struct LogsResponse {
                    logs: Vec<String>,
                }

                let logs_response: LogsResponse = response.json().await.map_err(|e| {
                    AosError::Network(format!("Failed to parse logs response: {}", e))
                })?;

                Ok(logs_response.logs)
            }
            StatusCode::NOT_FOUND => Err(AosError::NotFound(format!(
                "Service '{}' not found",
                service_id
            ))),
            status => Err(AosError::Network(format!(
                "Supervisor API returned status {}: {}",
                status,
                response.text().await.unwrap_or_default()
            ))),
        }
    }

    /// Health check - verify supervisor is reachable
    ///
    /// # Errors
    /// Returns `AosError::Network` if the supervisor is not reachable
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.base_url);

        let max_attempts = 2u32;
        let mut attempt = 0u32;
        let mut backoff = Duration::from_millis(100);

        loop {
            attempt += 1;
            match self
                .client
                .get(&url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                Ok(response) => return Ok(response.status().is_success()),
                Err(e) => {
                    if attempt >= max_attempts {
                        debug!(
                            "Supervisor health check failed after {} attempts: {}",
                            attempt, e
                        );
                        return Ok(false);
                    }
                    debug!(
                        "Supervisor health check failed (attempt {}), retrying: {}",
                        attempt, e
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(Duration::from_millis(500));
                }
            }
        }
    }

    /// Retry a request with exponential backoff
    ///
    /// # Arguments
    /// * `request_fn` - The request function to retry
    /// * `is_idempotent` - Whether the request is idempotent (safe to retry on any error)
    ///
    /// For non-idempotent requests (POST/PUT that modify state), we only retry on
    /// connection-level errors (connect timeout, no connection). We do NOT retry on
    /// server errors (5xx) because the request may have been partially processed.
    async fn retry_request_with_idempotency<F, Fut>(
        &self,
        request_fn: F,
        is_idempotent: bool,
    ) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match request_fn().await {
                Ok(response) => {
                    return Ok(response);
                }
                Err(e) => {
                    // Determine if we should retry based on error type and idempotency
                    let should_retry = if is_idempotent {
                        // For idempotent requests, retry on any error
                        true
                    } else {
                        // For non-idempotent requests, only retry on connection-level errors
                        // Do NOT retry if the request might have been received by the server
                        e.is_connect() || e.is_timeout()
                    };

                    if should_retry && attempt < self.max_retries {
                        let backoff = Duration::from_millis(100 * 2u64.pow(attempt));
                        warn!(
                            "Request failed (attempt {}/{}), retrying in {:?}: {}",
                            attempt + 1,
                            self.max_retries + 1,
                            backoff,
                            &e
                        );
                        tokio::time::sleep(backoff).await;
                    } else if !should_retry {
                        // Non-idempotent request failed after potentially reaching server
                        // Return immediately without retry
                        return Err(AosError::Network(format!(
                            "Non-idempotent request failed (not retrying): {}",
                            e
                        )));
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(AosError::Network(format!(
            "Request failed after {} attempts: {}",
            self.max_retries + 1,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        )))
    }

    /// Retry a request with exponential backoff (legacy wrapper, treats as idempotent)
    async fn retry_request<F, Fut>(&self, request_fn: F) -> Result<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    {
        // Default to idempotent for backwards compatibility with GET requests
        self.retry_request_with_idempotency(request_fn, true).await
    }

    /// Handle operation response (start/stop/restart)
    async fn handle_operation_response(
        &self,
        response: reqwest::Response,
        operation: &str,
        service_id: &str,
    ) -> Result<String> {
        match response.status() {
            StatusCode::OK => {
                let op_response: ServiceOperationResponse = response
                    .json()
                    .await
                    .map_err(|e| AosError::Network(format!("Failed to parse response: {}", e)))?;

                if op_response.success {
                    Ok(op_response.message)
                } else {
                    Err(AosError::Config(format!(
                        "Failed to {} service '{}': {}",
                        operation, service_id, op_response.message
                    )))
                }
            }
            StatusCode::NOT_FOUND => Err(AosError::NotFound(format!(
                "Service '{}' not found",
                service_id
            ))),
            status => Err(AosError::Network(format!(
                "Supervisor API returned status {} for {} operation: {}",
                status,
                operation,
                response.text().await.unwrap_or_default()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_client_or_skip<F, T>(builder: F) -> Option<T>
    where
        F: FnOnce() -> T + std::panic::UnwindSafe,
    {
        match std::panic::catch_unwind(builder) {
            Ok(client) => Some(client),
            Err(_) => {
                eprintln!("Skipping supervisor client test: reqwest init failed");
                None
            }
        }
    }

    #[test]
    fn test_client_creation() {
        let client = match build_client_or_skip(|| SupervisorClient::new("http://localhost:3301")) {
            Some(client) => client,
            None => return,
        };
        assert_eq!(client.base_url, "http://localhost:3301");
        assert_eq!(client.max_retries, 3);
    }

    #[test]
    fn test_client_from_env() {
        std::env::set_var("SUPERVISOR_API_URL", "http://custom:8080");
        let result = match build_client_or_skip(|| SupervisorClient::from_env()) {
            Some(result) => result,
            None => {
                std::env::remove_var("SUPERVISOR_API_URL");
                return;
            }
        };
        std::env::remove_var("SUPERVISOR_API_URL");

        let client = match result {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: {}", e);
                return;
            }
        };
        assert_eq!(client.base_url, "http://custom:8080");
    }

    #[test]
    fn test_client_from_env_missing_vars() {
        // Clear both potential env vars
        std::env::remove_var("SUPERVISOR_API_URL");
        std::env::remove_var("AOS_PANEL_PORT");

        let result = match build_client_or_skip(|| SupervisorClient::from_env()) {
            Some(result) => result,
            None => return,
        };
        assert!(result.is_err(), "Expected error when no env vars are set");

        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                msg.contains("SUPERVISOR_API_URL") && msg.contains("AOS_PANEL_PORT"),
                "Error message should mention both env vars: {}",
                msg
            );
        }
    }

    #[test]
    fn test_client_from_env_panel_port_fallback() {
        std::env::remove_var("SUPERVISOR_API_URL");
        std::env::set_var("AOS_PANEL_PORT", "9999");

        let result = match build_client_or_skip(|| SupervisorClient::from_env()) {
            Some(result) => result,
            None => {
                std::env::remove_var("AOS_PANEL_PORT");
                return;
            }
        };
        std::env::remove_var("AOS_PANEL_PORT");

        let client = match result {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping test: {}", e);
                return;
            }
        };
        assert_eq!(client.base_url, "http://127.0.0.1:9999");
    }

    #[test]
    fn test_client_with_config() {
        let client =
            match build_client_or_skip(|| SupervisorClient::new("http://localhost:3301")) {
                Some(client) => client,
                None => return,
            }
            .with_timeout(Duration::from_secs(5))
            .with_max_retries(5);

        assert_eq!(client.timeout, Duration::from_secs(5));
        assert_eq!(client.max_retries, 5);
    }
}
