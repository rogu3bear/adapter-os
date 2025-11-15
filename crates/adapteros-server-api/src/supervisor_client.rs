///! HTTP client for the AdapterOS Service Supervisor
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
    /// Reads `SUPERVISOR_API_URL` environment variable, defaults to `http://localhost:3301`
    pub fn from_env() -> Self {
        let base_url = std::env::var("SUPERVISOR_API_URL")
            .unwrap_or_else(|_| "http://localhost:3301".to_string());
        Self::new(base_url)
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

        let services_response: ServicesResponse = response.json().await.map_err(|e| {
            AosError::Serialization(format!("Failed to parse services response: {}", e))
        })?;

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
            StatusCode::OK => response.json().await.map_err(|e| {
                AosError::Serialization(format!("Failed to parse service response: {}", e))
            }),
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
    pub async fn start_service(&self, service_id: &str) -> Result<String> {
        let url = format!("{}/api/services/start", self.base_url);
        let request_body = StartServiceRequest {
            service_id: service_id.to_string(),
        };

        let response = self
            .retry_request(|| async {
                self.client
                    .post(&url)
                    .json(&request_body)
                    .timeout(self.timeout)
                    .send()
                    .await
            })
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
    pub async fn stop_service(&self, service_id: &str) -> Result<String> {
        let url = format!("{}/api/services/stop", self.base_url);
        let request_body = StartServiceRequest {
            service_id: service_id.to_string(),
        };

        let response = self
            .retry_request(|| async {
                self.client
                    .post(&url)
                    .json(&request_body)
                    .timeout(self.timeout)
                    .send()
                    .await
            })
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
    pub async fn restart_service(&self, service_id: &str) -> Result<String> {
        let url = format!("{}/api/services/restart", self.base_url);
        let request_body = StartServiceRequest {
            service_id: service_id.to_string(),
        };

        let response = self
            .retry_request(|| async {
                self.client
                    .post(&url)
                    .json(&request_body)
                    .timeout(self.timeout)
                    .send()
                    .await
            })
            .await?;

        self.handle_operation_response(response, "restart", service_id)
            .await
    }

    /// Start all essential services
    ///
    /// # Errors
    /// Returns `AosError::Network` if the request fails
    pub async fn start_essential_services(&self) -> Result<String> {
        let url = format!("{}/api/services/essential/start", self.base_url);

        let response = self
            .retry_request(|| async { self.client.post(&url).timeout(self.timeout).send().await })
            .await?;

        self.handle_operation_response(response, "start essential", "all")
            .await
    }

    /// Stop all essential services
    ///
    /// # Errors
    /// Returns `AosError::Network` if the request fails
    pub async fn stop_essential_services(&self) -> Result<String> {
        let url = format!("{}/api/services/essential/stop", self.base_url);

        let response = self
            .retry_request(|| async { self.client.post(&url).timeout(self.timeout).send().await })
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
                    AosError::Serialization(format!("Failed to parse logs response: {}", e))
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

        match self
            .client
            .get(&url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                debug!("Supervisor health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Retry a request with exponential backoff
    async fn retry_request<F, Fut>(&self, request_fn: F) -> Result<reqwest::Response>
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
                    last_error = Some(e);

                    if attempt < self.max_retries {
                        let backoff = Duration::from_millis(100 * 2u64.pow(attempt));
                        warn!(
                            "Request failed (attempt {}/{}), retrying in {:?}: {}",
                            attempt + 1,
                            self.max_retries + 1,
                            backoff,
                            last_error.as_ref().unwrap()
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        Err(AosError::Network(format!(
            "Request failed after {} attempts: {}",
            self.max_retries + 1,
            last_error.unwrap()
        )))
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
                let op_response: ServiceOperationResponse = response.json().await.map_err(|e| {
                    AosError::Serialization(format!("Failed to parse response: {}", e))
                })?;

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

    #[test]
    fn test_client_creation() {
        let client = SupervisorClient::new("http://localhost:3301");
        assert_eq!(client.base_url, "http://localhost:3301");
        assert_eq!(client.max_retries, 3);
    }

    #[test]
    fn test_client_from_env() {
        std::env::set_var("SUPERVISOR_API_URL", "http://custom:8080");
        let client = SupervisorClient::from_env();
        assert_eq!(client.base_url, "http://custom:8080");
        std::env::remove_var("SUPERVISOR_API_URL");
    }

    #[test]
    fn test_client_with_config() {
        let client = SupervisorClient::new("http://localhost:3301")
            .with_timeout(Duration::from_secs(5))
            .with_max_retries(5);

        assert_eq!(client.timeout, Duration::from_secs(5));
        assert_eq!(client.max_retries, 5);
    }
}
