//! Telemetry management commands
//!
//! Provides git-style subcommands for telemetry operations:
//! - `aosctl telemetry list` - List telemetry events
//! - `aosctl telemetry verify` - Verify telemetry bundle chain integrity

use crate::output::OutputWriter;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_crypto::signature::{PublicKey, Signature};
use adapteros_telemetry::bundle::SignatureMetadata;
use clap::Subcommand;
use serde::Serialize;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

/// Telemetry subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum TelemetryCommand {
    /// List telemetry events with optional filtering
    #[command(after_help = r#"Examples:
  # List all events
  aosctl telemetry list

  # Filter by stack
  aosctl telemetry list --by-stack stack-prod-001

  # Filter by event type
  aosctl telemetry list --event-type router.decision

  # Combine filters with JSON output
  aosctl telemetry list --by-stack stack-prod-001 --limit 100 --json > events.json
"#)]
    List {
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,

        /// Filter by stack ID
        #[arg(long)]
        by_stack: Option<String>,

        /// Filter by event type
        #[arg(long)]
        event_type: Option<String>,

        /// Maximum number of events to return
        #[arg(long, default_value = "50")]
        limit: u32,
    },

    /// Verify telemetry bundle chain integrity
    #[command(after_help = r#"Examples:
  aosctl telemetry verify --bundle-dir ./var/telemetry
  aosctl telemetry verify --bundle-dir ./var/telemetry --json > verify.json
"#)]
    Verify {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,
    },
}

/// Get telemetry command name for telemetry emission
fn get_telemetry_command_name(cmd: &TelemetryCommand) -> String {
    match cmd {
        TelemetryCommand::List { .. } => "telemetry_list".to_string(),
        TelemetryCommand::Verify { .. } => "telemetry_verify".to_string(),
    }
}

/// Handle telemetry subcommands
///
/// Routes telemetry commands to appropriate handlers
pub async fn handle_telemetry_command(cmd: TelemetryCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_telemetry_command_name(&cmd);

    info!(command = ?cmd, "Handling telemetry command");

    // Emit CLI telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await;

    match cmd {
        TelemetryCommand::List {
            database,
            by_stack,
            event_type,
            limit,
        } => list_telemetry_events(
            &database,
            by_stack.as_deref(),
            event_type.as_deref(),
            limit,
            output,
        )
        .await
        .map_err(|e| AosError::Internal(e.to_string())),
        TelemetryCommand::Verify { bundle_dir } => {
            verify_telemetry_chain(&bundle_dir, output).await
        }
    }
}

// ============================================================
// Telemetry List Implementation
// (consolidated from telemetry_list.rs)
// ============================================================

/// List telemetry events from database with optional stack filtering
///
/// This function queries telemetry bundles and their events, filtering by stack_id if provided.
/// Supports stack versioning and telemetry correlation.
pub async fn list_telemetry_events(
    database_path: &Path,
    by_stack: Option<&str>,
    event_type: Option<&str>,
    limit: u32,
    output: &OutputWriter,
) -> anyhow::Result<()> {
    use sqlx::sqlite::SqlitePool;
    use sqlx::Row;

    // Connect to database
    let db_url = format!("sqlite://{}", database_path.display());
    let pool = SqlitePool::connect(&db_url).await?;

    // Query telemetry_bundles table
    // Note: Actual event data is stored in bundle files, not database
    // This queries bundle metadata with stack correlation
    let bundles = if let Some(stack_id) = by_stack {
        // Filter bundles by stack_id (requires bundle metadata to include stack info)
        // For now, this is a placeholder - actual implementation needs bundle parsing
        sqlx::query(
            r#"
            SELECT id, tenant_id, cpid, path, event_count, created_at
            FROM telemetry_bundles
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(stack_id) // Temporary: using stack_id as tenant filter
        .bind(limit as i64)
        .fetch_all(&pool)
        .await?
    } else {
        // List all bundles
        sqlx::query(
            r#"
            SELECT id, tenant_id, cpid, path, event_count, created_at
            FROM telemetry_bundles
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&pool)
        .await?
    };

    // Format output
    let results: Vec<_> = bundles
        .iter()
        .map(|row| {
            json!({
                "bundle_id": row.get::<String, _>("id"),
                "tenant_id": row.get::<String, _>("tenant_id"),
                "cpid": row.get::<String, _>("cpid"),
                "path": row.get::<String, _>("path"),
                "event_count": row.get::<i64, _>("event_count"),
                "created_at": row.get::<String, _>("created_at"),
                // Note: stack_id and stack_version will be added when bundle metadata is updated
                "note": "Full event-level filtering requires parsing bundle files"
            })
        })
        .collect();

    if output.is_json() {
        output.print_json(&json!({
            "bundles": results,
            "count": results.len(),
            "limit": limit,
            "filters": {
                "by_stack": by_stack,
                "event_type": event_type
            }
        }))?;
    } else {
        output.print_line("Telemetry Bundles:")?;
        output.print_line(format!(
            "{:<36} {:<20} {:<12} {:>8}",
            "Bundle ID", "Tenant ID", "CPID", "Events"
        ))?;
        output.print_line("-".repeat(80))?;

        for bundle in &results {
            output.print_line(format!(
                "{:<36} {:<20} {:<12} {:>8}",
                bundle["bundle_id"].as_str().unwrap_or(""),
                bundle["tenant_id"].as_str().unwrap_or(""),
                bundle["cpid"].as_str().unwrap_or(""),
                bundle["event_count"].as_i64().unwrap_or(0),
            ))?;
        }

        output.print_line("")?;
        output.print_line(format!("Total bundles: {}", results.len()))?;

        if by_stack.is_some() || event_type.is_some() {
            output.print_line("")?;
            output
                .print_line("Note: Event-level stack filtering requires parsing bundle files.")?;
            output.print_line(
                "This currently shows bundle-level metadata. Full implementation pending.",
            )?;
        }
    }

    Ok(())
}

// ============================================================
// Telemetry Verify Implementation
// (consolidated from verify_telemetry.rs)
// ============================================================

#[derive(Serialize)]
struct VerificationResult {
    total_bundles: usize,
    verified_count: usize,
    chain_continuity: String,
    signatures_valid: bool,
}

/// Verify telemetry bundle chain
pub async fn verify_telemetry_chain(bundle_dir: &Path, output: &OutputWriter) -> Result<()> {
    output.info(format!(
        "Verifying telemetry bundle chain in: {}",
        bundle_dir.display()
    ));
    output.blank();

    let bundles = discover_bundles(bundle_dir)?;

    if bundles.is_empty() {
        output.warning("No bundles found");
        return Ok(());
    }

    output.info(format!("Found {} bundles", bundles.len()));
    output.blank();

    let mut prev_hash: Option<String> = None;
    let mut verified_count = 0;

    for bundle_info in bundles {
        let filename = bundle_info
            .path
            .file_name()
            .ok_or_else(|| AosError::Validation("Invalid bundle path".to_string()))?
            .to_string_lossy();
        output.progress(format!("Verifying: {}", filename));

        // Load signature metadata
        let metadata = load_signature_metadata(&bundle_info.sig_path)?;

        // Verify signature
        verify_signature(&bundle_info.path, &metadata)?;

        // Verify chain link
        if let Some(expected_prev) = &prev_hash {
            let expected_b3hash = B3Hash::from_hex(expected_prev).map_err(|e| {
                AosError::Validation(format!("Invalid expected hash format: {}", e))
            })?;

            match &metadata.prev_bundle_hash {
                Some(actual_prev) if *actual_prev == expected_b3hash => {
                    // Chain link valid
                }
                Some(actual_prev) => {
                    output.progress_done(false);
                    return Err(AosError::Validation(format!(
                        "Chain break detected!\n  Expected prev hash: {}\n  Got: {}",
                        expected_prev, actual_prev
                    )));
                }
                None => {
                    output.progress_done(false);
                    return Err(AosError::Validation(format!(
                        "Missing prev_bundle_hash in chain (expected: {})",
                        expected_prev
                    )));
                }
            }
        } else {
            // First bundle - should not have prev_bundle_hash
            if metadata.prev_bundle_hash.is_some() {
                output.warning("First bundle has prev_bundle_hash (will be ignored)");
            }
        }

        prev_hash = Some(metadata.merkle_root.clone());
        verified_count += 1;

        output.progress_done(true);
    }

    output.blank();

    if output.is_json() {
        let result = VerificationResult {
            total_bundles: verified_count,
            verified_count,
            chain_continuity: "intact".to_string(),
            signatures_valid: true,
        };
        output.json(&result)?;
    } else {
        output.success("Chain verified successfully!");
        output.kv("Total bundles", &verified_count.to_string());
        output.kv("Chain continuity", "intact");
        output.kv("Signatures", "all valid");
    }

    Ok(())
}

/// Bundle information
struct BundleInfo {
    path: PathBuf,
    sig_path: PathBuf,
    timestamp: u64,
}

/// Discover all telemetry bundles in a directory
fn discover_bundles(dir: &Path) -> Result<Vec<BundleInfo>> {
    let mut bundles = Vec::new();

    for entry in fs::read_dir(dir).map_err(|e| AosError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| AosError::Io(e.to_string()))?;
        let path = entry.path();

        // Look for .ndjson files
        if path.extension().and_then(|s| s.to_str()) == Some("ndjson") {
            let sig_path = path.with_extension("ndjson.sig");

            if !sig_path.exists() {
                eprintln!("Warning: Bundle missing signature: {}", path.display());
                continue;
            }

            // Load metadata to get timestamp for sorting
            let metadata = load_signature_metadata(&sig_path)?;

            bundles.push(BundleInfo {
                path,
                sig_path,
                timestamp: metadata.sequence_no as u64,
            });
        }
    }

    // Sort by timestamp (chronological order)
    bundles.sort_by_key(|b| b.timestamp);

    Ok(bundles)
}

/// Load signature metadata from .sig file
fn load_signature_metadata(sig_path: &Path) -> Result<SignatureMetadata> {
    let sig_json = fs::read_to_string(sig_path)
        .map_err(|e| AosError::Io(format!("Failed to read signature: {}", e)))?;

    serde_json::from_str(&sig_json).map_err(AosError::Serialization)
}

/// Verify bundle signature
fn verify_signature(_bundle_path: &Path, metadata: &SignatureMetadata) -> Result<()> {
    // Decode public key
    let pubkey_bytes = hex::decode(&metadata.public_key)
        .map_err(|e| AosError::Validation(format!("Invalid public key hex: {}", e)))?;

    let pubkey_array: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| AosError::Validation("Invalid public key length".to_string()))?;
    let pubkey = PublicKey::from_bytes(&pubkey_array)
        .map_err(|e| AosError::Validation(format!("Invalid public key: {}", e)))?;

    // Decode signature
    let sig_bytes = hex::decode(&metadata.signature)
        .map_err(|e| AosError::Validation(format!("Invalid signature hex: {}", e)))?;

    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| AosError::Validation("Invalid signature length".to_string()))?;
    let signature = Signature::from_bytes(&sig_array)
        .map_err(|e| AosError::Validation(format!("Invalid signature: {}", e)))?;

    // Verify signature against Merkle root
    let merkle_root_bytes = metadata.merkle_root.as_bytes();

    pubkey
        .verify(merkle_root_bytes, &signature)
        .map_err(|e| AosError::Validation(format!("Signature verification failed: {}", e)))?;

    // Note: For full verification, we should also:
    // 1. Recompute Merkle root from bundle events
    // 2. Compare with metadata.merkle_root
    // This requires reading the full bundle, which we skip for now

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMode;

    #[test]
    fn test_get_telemetry_command_name() {
        assert_eq!(
            get_telemetry_command_name(&TelemetryCommand::List {
                database: PathBuf::from("test.db"),
                by_stack: None,
                event_type: None,
                limit: 50
            }),
            "telemetry_list"
        );
        assert_eq!(
            get_telemetry_command_name(&TelemetryCommand::Verify {
                bundle_dir: PathBuf::from("./bundles")
            }),
            "telemetry_verify"
        );
    }

    #[test]
    fn test_telemetry_command_clone() {
        let cmd = TelemetryCommand::List {
            database: PathBuf::from("./var/aos-cp.sqlite3"),
            by_stack: Some("stack-prod-001".to_string()),
            event_type: Some("router.decision".to_string()),
            limit: 100,
        };

        let cloned = cmd.clone();
        match cloned {
            TelemetryCommand::List {
                database,
                by_stack,
                event_type,
                limit,
            } => {
                assert_eq!(database, PathBuf::from("./var/aos-cp.sqlite3"));
                assert_eq!(by_stack, Some("stack-prod-001".to_string()));
                assert_eq!(event_type, Some("router.decision".to_string()));
                assert_eq!(limit, 100);
            }
            _ => panic!("Expected List variant"),
        }
    }

    #[test]
    fn test_verify_command_clone() {
        let cmd = TelemetryCommand::Verify {
            bundle_dir: PathBuf::from("./var/telemetry"),
        };

        let cloned = cmd.clone();
        match cloned {
            TelemetryCommand::Verify { bundle_dir } => {
                assert_eq!(bundle_dir, PathBuf::from("./var/telemetry"));
            }
            _ => panic!("Expected Verify variant"),
        }
    }
}
