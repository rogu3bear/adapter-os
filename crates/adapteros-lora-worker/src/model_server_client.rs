//! Model Server Client
//!
//! gRPC client for connecting workers to the Model Server for shared model inference.
//! Workers connect to the model server instead of loading the model themselves,
//! reducing memory usage significantly.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hyper_util::rt::TokioIo;
use tokio::net::UnixStream;
use tokio::sync::RwLock;
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;
use tracing::{debug, error, info, warn};

use adapteros_core::{AosError, Result};
use adapteros_model_server::proto::{
    model_server_client::ModelServerClient as ProtoClient, DrainRequest, ForwardRequest,
    ForwardResponse, HealthRequest, HealthResponse, ListAdaptersRequest, ListAdaptersResponse,
    LoadAdapterRequest, LoadAdapterResponse, StatusRequest, StatusResponse, UnloadAdapterRequest,
    UnloadAdapterResponse, WarmupRequest, WarmupResponse,
};

/// Configuration for the model server client
#[derive(Debug, Clone)]
pub struct ModelServerClientConfig {
    /// Server address (e.g., "http://127.0.0.1:18085")
    pub server_addr: String,
    /// Unix socket path for UDS transport (preferred in hardened mode)
    pub socket_path: Option<PathBuf>,

    /// Connection timeout
    pub connect_timeout: Duration,

    /// Request timeout
    pub request_timeout: Duration,

    /// Number of retry attempts for transient failures
    pub max_retries: u32,

    /// Retry backoff base delay
    pub retry_backoff_base: Duration,
}

impl Default for ModelServerClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "http://127.0.0.1:18085".to_string(),
            socket_path: None,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_backoff_base: Duration::from_millis(100),
        }
    }
}

impl ModelServerClientConfig {
    /// Create config from socket path (UDS)
    pub fn from_socket_path(socket_path: PathBuf) -> Self {
        Self {
            // Endpoint host is ignored for UDS, but tonic requires a URI.
            server_addr: "http://[::]:50051".to_string(),
            socket_path: Some(socket_path),
            ..Default::default()
        }
    }

    /// Create config with server address
    pub fn with_addr(server_addr: impl Into<String>) -> Self {
        Self {
            server_addr: server_addr.into(),
            socket_path: None,
            ..Default::default()
        }
    }
}

/// Client connection state
enum ConnectionState {
    Disconnected,
    Connected(ProtoClient<Channel>),
    Draining,
}

/// Model Server client for workers
///
/// Provides a high-level interface for workers to communicate with the
/// Model Server for forward passes, KV cache management, and adapter operations.
pub struct ModelServerClient {
    config: ModelServerClientConfig,
    state: RwLock<ConnectionState>,
}

impl ModelServerClient {
    /// Create a new client with configuration
    pub fn new(config: ModelServerClientConfig) -> Self {
        Self {
            config,
            state: RwLock::new(ConnectionState::Disconnected),
        }
    }

    /// Create a new client with default configuration
    pub fn with_defaults() -> Self {
        Self::new(ModelServerClientConfig::default())
    }

    /// Connect to the model server
    pub async fn connect(&self) -> Result<()> {
        let mut state = self.state.write().await;

        if matches!(*state, ConnectionState::Connected(_)) {
            return Ok(());
        }

        if let Some(socket_path) = &self.config.socket_path {
            info!(
                socket_path = %socket_path.display(),
                "Connecting to Model Server over UDS"
            );
        } else {
            info!(
                server_addr = %self.config.server_addr,
                "Connecting to Model Server over TCP"
            );
        }

        let channel = if let Some(socket_path) = &self.config.socket_path {
            let socket_path = socket_path.clone();
            let endpoint = Endpoint::from_shared(self.config.server_addr.clone())
                .map_err(|e| AosError::Config(format!("Invalid model server endpoint: {}", e)))?
                .connect_timeout(self.config.connect_timeout)
                .timeout(self.config.request_timeout);
            endpoint
                .connect_with_connector(service_fn(move |_| {
                    let path = socket_path.clone();
                    async move {
                        let stream = UnixStream::connect(path).await?;
                        Ok::<_, std::io::Error>(TokioIo::new(stream))
                    }
                }))
                .await
                .map_err(|e| {
                    AosError::Internal(format!("Failed to connect to Model Server over UDS: {}", e))
                })?
        } else {
            Endpoint::from_shared(self.config.server_addr.clone())
                .map_err(|e| AosError::Config(format!("Invalid server address: {}", e)))?
                .connect_timeout(self.config.connect_timeout)
                .timeout(self.config.request_timeout)
                .connect()
                .await
                .map_err(|e| {
                    AosError::Internal(format!("Failed to connect to Model Server: {}", e))
                })?
        };

        let client = ProtoClient::new(channel);
        *state = ConnectionState::Connected(client);

        info!("Connected to Model Server");
        Ok(())
    }

    /// Disconnect from the model server
    pub async fn disconnect(&self) {
        let mut state = self.state.write().await;
        *state = ConnectionState::Disconnected;
        info!("Disconnected from Model Server");
    }

    /// Check if connected
    pub async fn is_connected(&self) -> bool {
        let state = self.state.read().await;
        matches!(*state, ConnectionState::Connected(_))
    }

    /// Get the client, connecting if necessary
    async fn get_client(&self) -> Result<ProtoClient<Channel>> {
        // Check current state
        {
            let state = self.state.read().await;
            if let ConnectionState::Connected(ref client) = *state {
                return Ok(client.clone());
            }
            if matches!(*state, ConnectionState::Draining) {
                return Err(AosError::Internal(
                    "Model Server is draining, cannot accept new requests".to_string(),
                ));
            }
        }

        // Need to connect
        self.connect().await?;

        let state = self.state.read().await;
        if let ConnectionState::Connected(ref client) = *state {
            Ok(client.clone())
        } else {
            Err(AosError::Internal(
                "Failed to establish connection to Model Server".to_string(),
            ))
        }
    }

    /// Execute a forward pass
    pub async fn forward(
        &self,
        session_id: String,
        input_ids: Vec<u32>,
        position: u32,
        max_seq_len: u32,
        adapter_ids: Vec<u32>,
        adapter_gates_q15: Vec<i32>,
        manifest_seed: Option<Vec<u8>>,
        include_hidden_states: bool,
    ) -> Result<ForwardResponse> {
        let mut client = self.get_client().await?;

        let request = ForwardRequest {
            session_id: session_id.clone(),
            input_ids,
            position,
            max_seq_len,
            adapter_ids,
            adapter_gates_q15,
            manifest_seed: manifest_seed.unwrap_or_default(),
            include_hidden_states,
        };

        let response = self
            .retry_request(|| async {
                let mut c = client.clone();
                c.forward(request.clone()).await
            })
            .await?;

        debug!(
            session_id = %session_id,
            position = response.position,
            kv_cache_hit = response.kv_cache_hit,
            latency_ms = response.forward_latency_ms,
            "Forward pass completed"
        );

        Ok(response)
    }

    /// Health check
    pub async fn health(&self) -> Result<HealthResponse> {
        let mut client = self.get_client().await?;
        let response = client
            .health(HealthRequest {})
            .await
            .map_err(|e| AosError::Internal(format!("Health check failed: {}", e)))?;
        Ok(response.into_inner())
    }

    /// Get server status
    pub async fn status(&self) -> Result<StatusResponse> {
        let mut client = self.get_client().await?;
        let response = client
            .status(StatusRequest {})
            .await
            .map_err(|e| AosError::Internal(format!("Status request failed: {}", e)))?;
        Ok(response.into_inner())
    }

    /// Warmup KV cache for a session
    pub async fn warmup(
        &self,
        session_id: String,
        input_ids: Vec<u32>,
        max_seq_len: u32,
    ) -> Result<WarmupResponse> {
        let mut client = self.get_client().await?;
        let response = client
            .warmup(WarmupRequest {
                session_id,
                input_ids,
                max_seq_len,
            })
            .await
            .map_err(|e| AosError::Internal(format!("Warmup request failed: {}", e)))?;
        Ok(response.into_inner())
    }

    /// Request server drain
    pub async fn drain(&self, grace_period_secs: u32) -> Result<()> {
        let mut client = self.get_client().await?;
        client
            .drain(DrainRequest { grace_period_secs })
            .await
            .map_err(|e| AosError::Internal(format!("Drain request failed: {}", e)))?;

        // Update local state
        let mut state = self.state.write().await;
        *state = ConnectionState::Draining;

        Ok(())
    }

    /// Load an adapter into the model server (promote to hot)
    pub async fn load_adapter(
        &self,
        adapter_id: u32,
        adapter_name: String,
        adapter_weights: Vec<u8>,
        promote_to_hot: bool,
    ) -> Result<LoadAdapterResponse> {
        let mut client = self.get_client().await?;
        let response = client
            .load_adapter(LoadAdapterRequest {
                adapter_id,
                adapter_name,
                adapter_weights,
                promote_to_hot,
            })
            .await
            .map_err(|e| AosError::Internal(format!("Load adapter failed: {}", e)))?;
        Ok(response.into_inner())
    }

    /// Unload an adapter from the model server
    pub async fn unload_adapter(&self, adapter_id: u32) -> Result<UnloadAdapterResponse> {
        let mut client = self.get_client().await?;
        let response = client
            .unload_adapter(UnloadAdapterRequest { adapter_id })
            .await
            .map_err(|e| AosError::Internal(format!("Unload adapter failed: {}", e)))?;
        Ok(response.into_inner())
    }

    /// List loaded adapters
    pub async fn list_adapters(&self) -> Result<ListAdaptersResponse> {
        let mut client = self.get_client().await?;
        let response = client
            .list_adapters(ListAdaptersRequest {})
            .await
            .map_err(|e| AosError::Internal(format!("List adapters failed: {}", e)))?;
        Ok(response.into_inner())
    }

    /// Retry a request with exponential backoff
    async fn retry_request<F, Fut, T>(&self, mut f: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = std::result::Result<tonic::Response<T>, tonic::Status>>,
    {
        let mut attempts = 0;
        let mut last_error = None;

        while attempts < self.config.max_retries {
            match f().await {
                Ok(response) => return Ok(response.into_inner()),
                Err(status) => {
                    let is_transient = matches!(
                        status.code(),
                        tonic::Code::Unavailable
                            | tonic::Code::DeadlineExceeded
                            | tonic::Code::ResourceExhausted
                    );

                    if !is_transient {
                        return Err(AosError::Internal(format!(
                            "Model Server request failed: {}",
                            status
                        )));
                    }

                    attempts += 1;
                    last_error = Some(status);

                    if attempts < self.config.max_retries {
                        let delay = self.config.retry_backoff_base * 2u32.pow(attempts - 1);
                        warn!(
                            attempt = attempts,
                            max_retries = self.config.max_retries,
                            delay_ms = delay.as_millis(),
                            "Retrying Model Server request"
                        );
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(AosError::Internal(format!(
            "Model Server request failed after {} retries: {:?}",
            self.config.max_retries, last_error
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ModelServerClientConfig::default();
        assert_eq!(config.server_addr, "http://127.0.0.1:18085");
        assert!(config.socket_path.is_none());
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_config_with_addr() {
        let config = ModelServerClientConfig::with_addr("http://localhost:9000");
        assert_eq!(config.server_addr, "http://localhost:9000");
        assert!(config.socket_path.is_none());
    }

    #[test]
    fn test_config_from_socket_path() {
        let config = ModelServerClientConfig::from_socket_path(PathBuf::from("var/run/model.sock"));
        assert_eq!(config.server_addr, "http://[::]:50051");
        assert_eq!(
            config.socket_path,
            Some(PathBuf::from("var/run/model.sock"))
        );
    }
}
