//! Async Control Plane client implementation

use std::sync::Arc;

use reqwest::Client;
use tracing::{debug, info};

use adapteros_api_types::workers::{
    WorkerFatalRequest, WorkerFatalResponse, WorkerHeartbeatRequest, WorkerHeartbeatResponse,
    WorkerRegistrationRequest, WorkerRegistrationResponse, WorkerStatusNotification,
    WorkerStatusResponse,
};

use crate::config::ClientConfig;
use crate::error::{Result, WorkerCpError};
use crate::retry::with_retry;

/// Async HTTP client for worker-to-control-plane communication
///
/// This client provides typed methods for all worker→CP operations:
/// - Registration
/// - Status notifications
/// - Heartbeats
/// - Fatal error reporting
#[derive(Clone)]
pub struct ControlPlaneClient {
    client: Client,
    config: Arc<ClientConfig>,
}

impl ControlPlaneClient {
    /// Create a new Control Plane client with the given configuration
    pub fn new(config: ClientConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(config.registration_timeout) // Default timeout, overridden per-request
            .build()
            .map_err(|e| WorkerCpError::Config(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            config: Arc::new(config),
        })
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    /// Register this worker with the control plane
    ///
    /// This should be called once on worker startup. The control plane validates
    /// the manifest hash against the plan's expected manifest.
    ///
    /// Returns the registration response with heartbeat interval.
    pub async fn register(
        &self,
        req: WorkerRegistrationRequest,
    ) -> Result<WorkerRegistrationResponse> {
        let url = format!("{}/v1/workers/register", self.config.base_url);

        with_retry(&self.config, "register", || {
            let url = url.clone();
            let req = req.clone();
            let client = self.client.clone();
            let config = Arc::clone(&self.config);

            async move {
                debug!(worker_id = %req.worker_id, "Sending registration request");

                let mut request = client
                    .post(&url)
                    .timeout(config.registration_timeout)
                    .json(&req);

                // Add auth header if configured
                if let Some(ref token) = config.auth_token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                let response = request.send().await?;
                let status = response.status();

                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(WorkerCpError::from_status(status.as_u16(), body));
                }

                let reg_response: WorkerRegistrationResponse =
                    response
                        .json()
                        .await
                        .map_err(|e| WorkerCpError::InvalidResponse {
                            message: format!("Failed to parse registration response: {}", e),
                            body: String::new(),
                        })?;

                // Check if registration was rejected
                if !reg_response.accepted {
                    let reason = reg_response
                        .rejection_reason
                        .clone()
                        .unwrap_or_else(|| "Unknown reason".to_string());
                    return Err(WorkerCpError::Rejected { reason });
                }

                info!(
                    worker_id = %reg_response.worker_id,
                    heartbeat_interval = reg_response.heartbeat_interval_secs,
                    "Worker registered successfully"
                );

                Ok(reg_response)
            }
        })
        .await
    }

    /// Notify the control plane of a status change
    ///
    /// Should be called when worker transitions between states:
    /// starting → serving → draining → stopped/crashed
    pub async fn notify_status(
        &self,
        req: WorkerStatusNotification,
    ) -> Result<WorkerStatusResponse> {
        let url = format!("{}/v1/workers/status", self.config.base_url);

        with_retry(&self.config, "notify_status", || {
            let url = url.clone();
            let req = req.clone();
            let client = self.client.clone();
            let config = Arc::clone(&self.config);

            async move {
                debug!(worker_id = %req.worker_id, status = %req.status, "Sending status notification");

                let mut request = client.post(&url).timeout(config.status_timeout).json(&req);

                if let Some(ref token) = config.auth_token {
                    request = request.header("Authorization", format!("Bearer {}", token));
                }

                let response = request.send().await?;
                let status = response.status();

                if !status.is_success() {
                    let body = response.text().await.unwrap_or_default();
                    return Err(WorkerCpError::from_status(status.as_u16(), body));
                }

                let status_response: WorkerStatusResponse = response.json().await.map_err(|e| {
                    WorkerCpError::InvalidResponse {
                        message: format!("Failed to parse status response: {}", e),
                        body: String::new(),
                    }
                })?;

                info!(
                    worker_id = %req.worker_id,
                    status = %req.status,
                    "Status notification sent successfully"
                );

                Ok(status_response)
            }
        })
        .await
    }

    /// Send a heartbeat to the control plane
    ///
    /// Should be called periodically (interval from registration response).
    /// Updates the worker's last_seen_at timestamp on the control plane.
    pub async fn send_heartbeat(
        &self,
        req: WorkerHeartbeatRequest,
    ) -> Result<WorkerHeartbeatResponse> {
        let url = format!("{}/v1/workers/heartbeat", self.config.base_url);

        // No retry for heartbeats - they happen frequently anyway
        debug!(worker_id = %req.worker_id, "Sending heartbeat");

        let mut request = self
            .client
            .post(&url)
            .timeout(self.config.heartbeat_timeout)
            .json(&req);

        if let Some(ref token) = self.config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(WorkerCpError::from_status(status.as_u16(), body));
        }

        let heartbeat_response: WorkerHeartbeatResponse =
            response
                .json()
                .await
                .map_err(|e| WorkerCpError::InvalidResponse {
                    message: format!("Failed to parse heartbeat response: {}", e),
                    body: String::new(),
                })?;

        debug!(
            worker_id = %req.worker_id,
            next_heartbeat_secs = heartbeat_response.next_heartbeat_secs,
            "Heartbeat acknowledged"
        );

        Ok(heartbeat_response)
    }

    /// Report a fatal error to the control plane (async version)
    ///
    /// For panic hook context, use `sync_helper::report_fatal_sync()` instead.
    /// This async version is useful for non-panic fatal errors.
    pub async fn report_fatal(&self, req: WorkerFatalRequest) -> Result<WorkerFatalResponse> {
        let url = format!("{}/v1/workers/fatal", self.config.base_url);

        // No retry for fatal reports - best effort single attempt
        debug!(worker_id = %req.worker_id, reason = %req.reason, "Reporting fatal error");

        let mut request = self
            .client
            .post(&url)
            .timeout(self.config.fatal_timeout)
            .json(&req);

        if let Some(ref token) = self.config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;
        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(WorkerCpError::from_status(status.as_u16(), body));
        }

        let fatal_response: WorkerFatalResponse =
            response
                .json()
                .await
                .map_err(|e| WorkerCpError::InvalidResponse {
                    message: format!("Failed to parse fatal response: {}", e),
                    body: String::new(),
                })?;

        info!(
            worker_id = %req.worker_id,
            incident_id = %fatal_response.incident_id,
            "Fatal error reported"
        );

        Ok(fatal_response)
    }
}

impl std::fmt::Debug for ControlPlaneClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ControlPlaneClient")
            .field("base_url", &self.config.base_url)
            .finish()
    }
}
