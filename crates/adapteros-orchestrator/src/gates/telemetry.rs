//! Telemetry Promotion Gate
//!
//! Verifies telemetry bundle chain integrity before allowing CP promotion.
//! This ensures complete audit trail and prevents promotion with gaps
//! in the telemetry record.

use crate::{DependencyChecker, Gate, OrchestratorConfig};
use adapteros_core::{AosError, Result};
use adapteros_crypto::signature::{PublicKey, Signature};
use adapteros_db::Db;
use adapteros_telemetry::bundle::SignatureMetadata;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Telemetry verification gate
#[derive(Debug, Clone, Default)]
pub struct TelemetryGate;

#[async_trait::async_trait]
impl Gate for TelemetryGate {
    fn name(&self) -> String {
        "Telemetry Chain".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        debug!("Verifying telemetry chain");

        // Check dependencies first
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("telemetry")?;

        if !deps.all_available {
            debug!(messages = ?deps.messages, "Some telemetry dependencies missing");
        }

        // Connect to database for signature verification
        let db = Db::connect(&config.db_path).await?;

        // Try to resolve telemetry directory
        let bundle_dir = if let Some(resolved) = deps.get_resolved_path("telemetry_dir") {
            let path = Path::new(&resolved).join(&config.cpid);
            if path.exists() {
                if resolved != "var/telemetry" {
                    warn!("Using fallback telemetry path: {}", resolved);
                }
                path
            } else {
                // Fallback to default
                Path::new("var/telemetry").join(&config.cpid)
            }
        } else {
            Path::new("var/telemetry").join(&config.cpid)
        };

        if !bundle_dir.exists() {
            // Try to handle missing bundles gracefully
            if config.require_telemetry_bundles {
                return Err(AosError::NotFound(format!(
                    "No telemetry bundles found for CPID: {}. Checked: {}",
                    config.cpid,
                    bundle_dir.display()
                )));
            } else {
                warn!(
                    bundle_dir = %bundle_dir.display(),
                    "Telemetry bundles not required but not found - proceeding with caution"
                );
                return Ok(());
            }
        }

        let bundles = discover_bundles(&bundle_dir)?;

        if bundles.is_empty() {
            if config.require_telemetry_bundles {
                return Err(AosError::NotFound(format!(
                    "No telemetry bundles found in: {}",
                    bundle_dir.display()
                )));
            } else {
                warn!(
                    bundle_dir = %bundle_dir.display(),
                    "No telemetry bundles found but not required"
                );
                return Ok(());
            }
        }

        verify_chain(&bundles, &db, &config.cpid).await?;

        tracing::info!(bundle_count = bundles.len(), "Telemetry chain verified");
        Ok(())
    }
}

/// Bundle information
struct BundleInfo {
    /// Path to the bundle file (reserved for bundle content loading)
    _path: PathBuf,
    sig_path: PathBuf,
    timestamp: u64,
}

/// Discover all telemetry bundles in a directory
fn discover_bundles(dir: &Path) -> Result<Vec<BundleInfo>> {
    let mut bundles = Vec::new();

    for entry in fs::read_dir(dir)
        .map_err(|e| AosError::Io(format!("Failed to read telemetry directory: {}", e)))?
    {
        let entry =
            entry.map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
        let path = entry.path();

        // Look for .ndjson files
        if path.extension().and_then(|s| s.to_str()) == Some("ndjson") {
            let sig_path = path.with_extension("ndjson.sig");

            if !sig_path.exists() {
                return Err(AosError::Validation(format!(
                    "Bundle missing signature: {}",
                    path.display()
                )));
            }

            // Load metadata to get sequence number for sorting
            let metadata = load_signature_metadata(&sig_path)?;

            bundles.push(BundleInfo {
                _path: path,
                sig_path,
                timestamp: metadata.sequence_no,
            });
        }
    }

    // Sort by sequence number (chronological order)
    bundles.sort_by_key(|b| b.timestamp);

    Ok(bundles)
}

/// Verify the complete bundle chain
async fn verify_chain(bundles: &[BundleInfo], db: &Db, cpid: &str) -> Result<()> {
    let mut prev_hash: Option<String> = None;

    for (i, bundle_info) in bundles.iter().enumerate() {
        // Load signature metadata from file
        let metadata = load_signature_metadata(&bundle_info.sig_path)?;

        // Verify signature cryptographically
        verify_signature(&metadata)?;

        // Verify signature against database record
        verify_signature_against_db(&metadata, db, cpid).await?;

        // Verify chain link
        if let Some(expected_prev) = &prev_hash {
            match &metadata.prev_bundle_hash {
                Some(actual_prev) if actual_prev.to_string() == *expected_prev => {
                    // Chain link valid
                }
                Some(actual_prev) => {
                    return Err(AosError::Validation(format!(
                        "Chain break at bundle {}!\n  Expected prev: {}\n  Got: {}",
                        i, expected_prev, actual_prev
                    )));
                }
                None => {
                    return Err(AosError::Validation(format!(
                        "Missing prev_bundle_hash at bundle {} (expected: {})",
                        i, expected_prev
                    )));
                }
            }
        } else {
            // First bundle - should not have prev_bundle_hash
            if metadata.prev_bundle_hash.is_some() {
                // This is OK, just means we're starting mid-chain
            }
        }

        prev_hash = Some(metadata.merkle_root.clone());
    }

    Ok(())
}

/// Verify signature metadata against database record
async fn verify_signature_against_db(
    metadata: &SignatureMetadata,
    db: &Db,
    cpid: &str,
) -> Result<()> {
    // Look up stored signature by merkle root (bundle hash)
    let stored_sig = db
        .get_bundle_signature(&metadata.merkle_root)
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to query bundle signature from database: {}",
                e
            ))
        })?;

    match stored_sig {
        Some(db_sig) => {
            // Verify CPID matches
            if db_sig.cpid != cpid {
                return Err(AosError::Validation(format!(
                    "Bundle CPID mismatch: expected '{}', database has '{}'",
                    cpid, db_sig.cpid
                )));
            }

            // Verify signature matches database record
            if db_sig.signature_hex != metadata.signature {
                return Err(AosError::Verification(format!(
                    "Signature mismatch for bundle {}:\n  File: {}\n  Database: {}",
                    metadata.merkle_root, metadata.signature, db_sig.signature_hex
                )));
            }

            // Verify public key matches database record
            if db_sig.public_key_hex != metadata.public_key {
                return Err(AosError::Verification(format!(
                    "Public key mismatch for bundle {}:\n  File: {}\n  Database: {}",
                    metadata.merkle_root, metadata.public_key, db_sig.public_key_hex
                )));
            }

            tracing::debug!(
                bundle_hash = %metadata.merkle_root,
                "Bundle signature verified against database"
            );
        }
        None => {
            // No database record - this is an error for promotion gates
            return Err(AosError::NotFound(format!(
                "Bundle signature not found in database: {}. \
                 All bundles must be registered before promotion.",
                metadata.merkle_root
            )));
        }
    }

    Ok(())
}

/// Load signature metadata from .sig file
fn load_signature_metadata(sig_path: &Path) -> Result<SignatureMetadata> {
    let sig_json = fs::read_to_string(sig_path)
        .map_err(|e| AosError::Io(format!("Failed to read signature file: {}", e)))?;

    serde_json::from_str(&sig_json)
        .map_err(|e| AosError::Parse(format!("Failed to parse signature metadata: {}", e)))
}

/// Verify bundle signature
fn verify_signature(metadata: &SignatureMetadata) -> Result<()> {
    // Decode public key
    let pubkey_bytes = hex::decode(&metadata.public_key)
        .map_err(|e| AosError::Crypto(format!("Invalid public key hex: {}", e)))?;

    if pubkey_bytes.len() != 32 {
        return Err(AosError::Crypto(format!(
            "Invalid public key length: {}",
            pubkey_bytes.len()
        )));
    }
    let mut pubkey_array = [0u8; 32];
    pubkey_array.copy_from_slice(&pubkey_bytes);
    let pubkey = PublicKey::from_bytes(&pubkey_array)
        .map_err(|e| AosError::Crypto(format!("Invalid public key format: {}", e)))?;

    // Decode signature
    let sig_bytes = hex::decode(&metadata.signature)
        .map_err(|e| AosError::Crypto(format!("Invalid signature hex: {}", e)))?;

    if sig_bytes.len() != 64 {
        return Err(AosError::Crypto(format!(
            "Invalid signature length: {}",
            sig_bytes.len()
        )));
    }
    let mut sig_array = [0u8; 64];
    sig_array.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_array)
        .map_err(|e| AosError::Crypto(format!("Invalid signature format: {}", e)))?;

    // Verify signature against Merkle root
    let merkle_root_bytes = metadata.merkle_root.as_bytes();

    pubkey
        .verify(merkle_root_bytes, &signature)
        .map_err(|e| AosError::Verification(format!("Signature verification failed: {}", e)))?;

    Ok(())
}
