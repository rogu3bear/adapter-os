//! UDS API Server for Secure Enclave Operations

use crate::audit::AuditLogger;
use crate::enclave::EnclaveManager;
use crate::protocol::{Request, Response};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use adapteros_deterministic_exec::spawn_deterministic;

/// Serve enclave operations over Unix Domain Socket
pub async fn serve_uds(
    socket_path: impl AsRef<Path>,
    audit_logger: AuditLogger,
) -> std::io::Result<()> {
    let socket_path = socket_path.as_ref();

    // Remove existing socket if present
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }

    // Create parent directory if needed
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    tracing::info!("Starting enclave daemon on: {}", socket_path.display());

    let listener = UnixListener::bind(socket_path)?;

    // Set restrictive permissions (owner read/write only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(socket_path, perms)?;
    }

    let enclave =
        Arc::new(Mutex::new(EnclaveManager::new().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
        })?));

    tracing::info!("Enclave daemon ready");

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let enclave = enclave.clone();
                let audit_logger = audit_logger.clone();
                let _ = spawn_deterministic("UDS connection handler".to_string(), async move {
                    if let Err(e) = handle_connection(stream, enclave, audit_logger).await {
                        tracing::error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                tracing::error!("Accept error: {}", e);
            }
        }
    }
}

/// Handle a single UDS connection
async fn handle_connection(
    stream: UnixStream,
    enclave: Arc<Mutex<EnclaveManager>>,
    audit_logger: AuditLogger,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;

        if n == 0 {
            // EOF
            break;
        }

        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let response = Response::error(format!("Invalid request: {}", e));
                let response_json = serde_json::to_string(&response)?;
                writer.write_all(response_json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                continue;
            }
        };

        tracing::debug!("Processing request: {:?}", request);

        let response = process_request(request, &enclave, &audit_logger).await;
        let response_json = serde_json::to_string(&response)?;

        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }

    Ok(())
}

/// Process a single request
async fn process_request(
    request: Request,
    enclave: &Arc<Mutex<EnclaveManager>>,
    audit_logger: &AuditLogger,
) -> Response {
    use base64::Engine;
    let engine = base64::engine::general_purpose::STANDARD;

    match request {
        Request::Sign { data, key_label } => {
            let data_bytes = match engine.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    audit_logger
                        .log_error("sign", None, &format!("Invalid base64: {}", e))
                        .await;
                    return Response::error(format!("Invalid base64: {}", e));
                }
            };

            let hash_hex = hex::encode(&data_bytes[..32.min(data_bytes.len())]);
            let label = key_label.as_deref().unwrap_or("aos_bundle_signing");
            let mut enclave = enclave.lock().await;

            // For signing, we need to use the correct key
            // Currently sign_bundle uses a hardcoded label internally
            match enclave.sign_bundle(&data_bytes) {
                Ok(signature) => {
                    audit_logger.log_success("sign", Some(&hash_hex)).await;
                    Response::ok(signature)
                }
                Err(e) => {
                    let error_msg = format!("Signing failed: {}", e);
                    audit_logger
                        .log_error("sign", Some(&hash_hex), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::Seal { data } => {
            let data_bytes = match engine.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    audit_logger
                        .log_error("seal", None, &format!("Invalid base64: {}", e))
                        .await;
                    return Response::error(format!("Invalid base64: {}", e));
                }
            };

            let hash_hex = hex::encode(&data_bytes[..32.min(data_bytes.len())]);
            let mut enclave = enclave.lock().await;

            match enclave.seal_lora_delta(&data_bytes) {
                Ok(sealed) => {
                    audit_logger.log_success("seal", Some(&hash_hex)).await;
                    Response::ok(sealed)
                }
                Err(e) => {
                    let error_msg = format!("Sealing failed: {}", e);
                    audit_logger
                        .log_error("seal", Some(&hash_hex), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::Unseal { data } => {
            let data_bytes = match engine.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    audit_logger
                        .log_error("unseal", None, &format!("Invalid base64: {}", e))
                        .await;
                    return Response::error(format!("Invalid base64: {}", e));
                }
            };

            let hash_hex = hex::encode(&data_bytes[..32.min(data_bytes.len())]);
            let mut enclave = enclave.lock().await;

            match enclave.unseal_lora_delta(&data_bytes) {
                Ok(unsealed) => {
                    audit_logger.log_success("unseal", Some(&hash_hex)).await;
                    Response::ok(unsealed)
                }
                Err(e) => {
                    let error_msg = format!("Unsealing failed: {}", e);
                    audit_logger
                        .log_error("unseal", Some(&hash_hex), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::GetPublicKey { key_label } => {
            let mut enclave = enclave.lock().await;
            match enclave.get_public_key(&key_label) {
                Ok(pubkey) => {
                    audit_logger
                        .log_success("get_public_key", Some(&key_label))
                        .await;
                    Response::ok(pubkey)
                }
                Err(e) => {
                    let error_msg = format!("Failed to get public key: {}", e);
                    audit_logger
                        .log_error("get_public_key", Some(&key_label), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::Ping => Response::ok_empty(),
    }
}
