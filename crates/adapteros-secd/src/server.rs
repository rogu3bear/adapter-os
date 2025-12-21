//! UDS API Server for Secure Enclave Operations

use crate::audit::AuditLogger;
use crate::enclave::EnclaveManager;
use crate::protocol::{Request, Response};
use adapteros_deterministic_exec::spawn_deterministic;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

fn export_token_valid(token: &str) -> bool {
    matches!(
        std::env::var("AOS_SECD_EXPORT_TOKEN"),
        Ok(expected) if !expected.is_empty() && expected == token
    )
}

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

    let enclave = Arc::new(Mutex::new(
        EnclaveManager::new().map_err(|e| std::io::Error::other(e.to_string()))?,
    ));

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

            // Sign using the specified key label for key selection
            match enclave.sign_with_label(label, &data_bytes) {
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

        Request::EnsureTenantKey { tenant_id } => {
            let label = format!("tenant:{}", tenant_id);
            let mut enclave = enclave.lock().await;
            match enclave.ensure_encryption_key(&label) {
                Ok(_) => {
                    audit_logger
                        .log_success("ensure_tenant_key", Some(&tenant_id))
                        .await;
                    Response::ok_empty()
                }
                Err(e) => {
                    let error_msg = format!("Ensure tenant key failed: {}", e);
                    audit_logger
                        .log_error("ensure_tenant_key", Some(&tenant_id), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::SealTenant { tenant_id, data } => {
            let data_bytes = match engine.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    audit_logger
                        .log_error(
                            "seal_tenant",
                            Some(&tenant_id),
                            &format!("Invalid base64: {}", e),
                        )
                        .await;
                    return Response::error(format!("Invalid base64: {}", e));
                }
            };

            let label = format!("tenant:{}", tenant_id);
            let hash_hex = hex::encode(&data_bytes[..32.min(data_bytes.len())]);
            let mut enclave = enclave.lock().await;

            match enclave.seal_with_label(&label, &data_bytes) {
                Ok(sealed) => {
                    audit_logger
                        .log_success("seal_tenant", Some(&hash_hex))
                        .await;
                    Response::ok(sealed)
                }
                Err(e) => {
                    let error_msg = format!("Tenant seal failed: {}", e);
                    audit_logger
                        .log_error("seal_tenant", Some(&hash_hex), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::UnsealTenant { tenant_id, data } => {
            let data_bytes = match engine.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    audit_logger
                        .log_error(
                            "unseal_tenant",
                            Some(&tenant_id),
                            &format!("Invalid base64: {}", e),
                        )
                        .await;
                    return Response::error(format!("Invalid base64: {}", e));
                }
            };

            let label = format!("tenant:{}", tenant_id);
            let hash_hex = hex::encode(&data_bytes[..32.min(data_bytes.len())]);
            let mut enclave = enclave.lock().await;

            match enclave.unseal_with_label(&label, &data_bytes) {
                Ok(unsealed) => {
                    audit_logger
                        .log_success("unseal_tenant", Some(&hash_hex))
                        .await;
                    Response::ok(unsealed)
                }
                Err(e) => {
                    let error_msg = format!("Tenant unseal failed: {}", e);
                    audit_logger
                        .log_error("unseal_tenant", Some(&hash_hex), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::DigestTenant { tenant_id, data } => {
            let data_bytes = match engine.decode(&data) {
                Ok(bytes) => bytes,
                Err(e) => {
                    audit_logger
                        .log_error(
                            "digest_tenant",
                            Some(&tenant_id),
                            &format!("Invalid base64: {}", e),
                        )
                        .await;
                    return Response::error(format!("Invalid base64: {}", e));
                }
            };

            let label = format!("tenant:{}", tenant_id);
            let mut enclave = enclave.lock().await;

            match enclave.digest_with_label(&label, &data_bytes) {
                Ok(digest) => {
                    audit_logger
                        .log_success("digest_tenant", Some(&tenant_id))
                        .await;
                    Response::ok(digest.to_vec())
                }
                Err(e) => {
                    let error_msg = format!("Tenant digest failed: {}", e);
                    audit_logger
                        .log_error("digest_tenant", Some(&tenant_id), &error_msg)
                        .await;
                    Response::error(error_msg)
                }
            }
        }

        Request::ExportTenantKey {
            tenant_id,
            permission_token,
        } => {
            if !export_token_valid(&permission_token) {
                audit_logger
                    .log_error("export_tenant_key", Some(&tenant_id), "permission denied")
                    .await;
                return Response::error("permission denied");
            }

            let label = format!("tenant:{}", tenant_id);
            let mut enclave = enclave.lock().await;
            match enclave.export_encryption_key(&label) {
                Ok(key_bytes) => {
                    audit_logger
                        .log_success("export_tenant_key", Some(&tenant_id))
                        .await;
                    Response::ok(key_bytes.to_vec())
                }
                Err(e) => {
                    let error_msg = format!("Export tenant key failed: {}", e);
                    audit_logger
                        .log_error("export_tenant_key", Some(&tenant_id), &error_msg)
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

#[cfg(test)]
mod tests {
    use super::export_token_valid;

    #[test]
    fn export_token_guard_respects_permission() {
        std::env::set_var("AOS_SECD_EXPORT_TOKEN", "permit");
        assert!(export_token_valid("permit"));
        assert!(!export_token_valid("deny"));
        std::env::remove_var("AOS_SECD_EXPORT_TOKEN");
    }
}
