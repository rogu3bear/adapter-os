//! Secure Enclave Daemon (secd) client
//!
//! Provides UDS-based communication with the aos-secd daemon for
//! signing and encryption operations.

use adapteros_core::{AosError, Result};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{sleep, timeout};

/// UDS client for communicating with aos-secd daemon
pub struct SecdClient {
    socket_path: String,
    timeout_duration: Duration,
}

/// Request to secd daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SecdRequest {
    /// Seal data for encryption
    Seal { data: String },
    /// Unseal encrypted data
    Unseal { data: String },
    /// Sign data
    Sign { data: String },
    /// Seal data with tenant-specific key
    SealTenant { tenant_id: String, data: String },
    /// Unseal data with tenant-specific key
    UnsealTenant { tenant_id: String, data: String },
    /// Compute keyed digest with tenant-specific key
    DigestTenant { tenant_id: String, data: String },
    /// Ensure tenant key exists
    EnsureTenantKey { tenant_id: String },
    /// Export tenant key (requires permission token)
    ExportTenantKey {
        tenant_id: String,
        permission_token: String,
    },
    /// Health check
    Ping,
}

/// Response from secd daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SecdResponse {
    /// Operation succeeded
    Ok { result: Option<String> },
    /// Operation failed
    Error { message: String },
}

impl SecdClient {
    /// Create a new secd client
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout_duration: Duration::from_secs(5),
        }
    }

    /// Set timeout duration for operations
    pub fn with_timeout(mut self, duration: Duration) -> Self {
        self.timeout_duration = duration;
        self
    }

    /// Seal data using secd daemon
    pub async fn seal(&self, data: &[u8]) -> Result<Vec<u8>> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(data);
        let request = SecdRequest::Seal { data: base64_data };

        let response = self.send_request(request).await?;

        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|e| AosError::Crypto(format!("Invalid base64 in seal response: {}", e))),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Seal operation failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Unseal encrypted data using secd daemon
    pub async fn unseal(&self, sealed_data: &[u8]) -> Result<Vec<u8>> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(sealed_data);
        let request = SecdRequest::Unseal { data: base64_data };

        let response = self.send_request(request).await?;

        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|e| AosError::Crypto(format!("Invalid base64 in unseal response: {}", e))),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Unseal operation failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Sign data using secd daemon
    pub async fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(data);
        let request = SecdRequest::Sign { data: base64_data };

        let response = self.send_request(request).await?;

        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|e| AosError::Crypto(format!("Invalid base64 in sign response: {}", e))),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Sign operation failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Seal data using a tenant-specific key
    pub async fn seal_tenant(&self, tenant_id: &str, data: &[u8]) -> Result<Vec<u8>> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(data);
        let request = SecdRequest::SealTenant {
            tenant_id: tenant_id.to_string(),
            data: base64_data,
        };
        let response = self.send_request(request).await?;

        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|e| {
                    AosError::Crypto(format!("Invalid base64 in seal_tenant response: {}", e))
                }),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Seal tenant operation failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Unseal data using a tenant-specific key
    pub async fn unseal_tenant(&self, tenant_id: &str, sealed: &[u8]) -> Result<Vec<u8>> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(sealed);
        let request = SecdRequest::UnsealTenant {
            tenant_id: tenant_id.to_string(),
            data: base64_data,
        };
        let response = self.send_request(request).await?;

        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|e| {
                    AosError::Crypto(format!("Invalid base64 in unseal_tenant response: {}", e))
                }),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Unseal tenant operation failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Compute keyed digest for data using tenant-specific key
    pub async fn digest_tenant(&self, tenant_id: &str, data: &[u8]) -> Result<[u8; 32]> {
        let base64_data = base64::engine::general_purpose::STANDARD.encode(data);
        let request = SecdRequest::DigestTenant {
            tenant_id: tenant_id.to_string(),
            data: base64_data,
        };
        let response = self.send_request(request).await?;

        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(result)
                    .map_err(|e| {
                        AosError::Crypto(format!("Invalid base64 in digest_tenant response: {}", e))
                    })?;
                if bytes.len() != 32 {
                    return Err(AosError::Crypto(format!(
                        "Expected 32-byte digest, got {} bytes",
                        bytes.len()
                    )));
                }
                let mut out = [0u8; 32];
                out.copy_from_slice(&bytes);
                Ok(out)
            }
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Digest tenant operation failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Ensure tenant key exists
    pub async fn ensure_tenant_key(&self, tenant_id: &str) -> Result<()> {
        let request = SecdRequest::EnsureTenantKey {
            tenant_id: tenant_id.to_string(),
        };
        let response = self.send_request(request).await?;
        match response {
            SecdResponse::Ok { .. } => Ok(()),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Ensure tenant key failed: {}",
                message
            ))),
        }
    }

    /// Export tenant key bytes (requires permission token)
    pub async fn export_tenant_key(
        &self,
        tenant_id: &str,
        permission_token: &str,
    ) -> Result<Vec<u8>> {
        let request = SecdRequest::ExportTenantKey {
            tenant_id: tenant_id.to_string(),
            permission_token: permission_token.to_string(),
        };
        let response = self.send_request(request).await?;
        match response {
            SecdResponse::Ok {
                result: Some(result),
            } => base64::engine::general_purpose::STANDARD
                .decode(result)
                .map_err(|e| {
                    AosError::Crypto(format!(
                        "Invalid base64 in export_tenant_key response: {}",
                        e
                    ))
                }),
            SecdResponse::Error { message } => Err(AosError::Crypto(format!(
                "Export tenant key failed: {}",
                message
            ))),
            _ => Err(AosError::Crypto("Invalid response format".to_string())),
        }
    }

    /// Send request to secd daemon with retry logic
    async fn send_request(&self, request: SecdRequest) -> Result<SecdResponse> {
        let mut last_error = None;

        // Exponential backoff retry: 100ms, 200ms, 400ms
        for attempt in 0..3 {
            match self.try_send_request(&request).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if attempt < 2 {
                        let delay = Duration::from_millis(100 * (1 << attempt));
                        tracing::debug!(
                            "Secd request failed (attempt {}), retrying in {:?}: {}",
                            attempt + 1,
                            delay,
                            e
                        );
                        sleep(delay).await;
                    }
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            AosError::Artifact(
                "All secd request attempts failed with no error recorded".to_string(),
            )
        }))
    }

    /// Attempt to send request to secd daemon
    async fn try_send_request(&self, request: &SecdRequest) -> Result<SecdResponse> {
        // Check if socket exists
        if !Path::new(&self.socket_path).exists() {
            return Err(AosError::Crypto(format!(
                "Secd socket not found: {}. Is aos-secd daemon running?",
                self.socket_path
            )));
        }

        // Connect to UDS socket
        let mut stream = timeout(
            self.timeout_duration,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .map_err(|_| AosError::Crypto("Connection timeout".to_string()))?
        .map_err(|e| AosError::Crypto(format!("Failed to connect to secd: {}", e)))?;

        // Serialize request
        let request_json = serde_json::to_string(request).map_err(AosError::Serialization)?;

        // Send request
        stream
            .write_all(request_json.as_bytes())
            .await
            .map_err(|e| AosError::Io(format!("Failed to write request: {}", e)))?;
        stream
            .write_all(b"\n")
            .await
            .map_err(|e| AosError::Io(format!("Failed to write newline: {}", e)))?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();

        timeout(self.timeout_duration, reader.read_line(&mut response_line))
            .await
            .map_err(|_| AosError::Crypto("Response timeout".to_string()))?
            .map_err(|e| AosError::Io(format!("Failed to read response: {}", e)))?;

        // Parse response
        let response: SecdResponse =
            serde_json::from_str(&response_line).map_err(AosError::Serialization)?;

        Ok(response)
    }
}

/// Default secd client instance
pub fn default_secd_client() -> SecdClient {
    SecdClient::new("/var/run/aos-secd.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_secd_client_creation() {
        let client = SecdClient::new("/tmp/test-socket");
        assert_eq!(client.socket_path, "/tmp/test-socket");
    }

    #[tokio::test]
    async fn test_secd_client_timeout() {
        let client = SecdClient::new("/tmp/test-socket").with_timeout(Duration::from_millis(100));

        // This will fail because socket doesn't exist, but tests timeout setting
        let result = client.ping().await;
        assert!(result.is_err());
    }

    impl SecdClient {
        /// Ping the secd daemon
        pub async fn ping(&self) -> Result<()> {
            let request = SecdRequest::Ping;
            let response = self.send_request(request).await?;

            match response {
                SecdResponse::Ok { .. } => Ok(()),
                SecdResponse::Error { message } => {
                    Err(AosError::Crypto(format!("Ping failed: {}", message)))
                }
            }
        }
    }
}
