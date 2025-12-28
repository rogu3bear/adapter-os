//! Pre-boot audit logging for events that occur before the database is available.
//!
//! During the boot sequence, certain security-critical events (like baseline creation,
//! keypair generation, and drift detection) occur before the database connection is
//! established. This module provides a file-based audit log that captures these events
//! and can later be replayed to the database once it's available.
//!
//! # Event Types
//!
//! - `BaselineCreated` - Initial baseline fingerprint created with operator authorization
//! - `BaselineVerified` - Baseline fingerprint loaded and verified
//! - `DriftDetected` - Environment drift detected against baseline
//! - `KeypairLoaded` - Signing keypair loaded or generated
//!
//! # File Format
//!
//! Events are stored as JSON Lines (JSONL) in `var/pre_boot_audit.jsonl`.
//! Each line is a complete JSON object that can be independently parsed.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use tracing::{debug, warn};

/// Pre-boot audit log file path
const PRE_BOOT_AUDIT_PATH: &str = "var/pre_boot_audit.jsonl";

/// Pre-boot audit event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type", rename_all = "snake_case")]
pub enum PreBootAuditEvent {
    /// Initial baseline fingerprint created with explicit operator authorization
    BaselineCreated {
        fingerprint_hash: String,
        baseline_path: String,
        creation_method: String, // "cli_flag" or "env_var"
    },

    /// Baseline fingerprint loaded and signature verified
    BaselineVerified {
        baseline_hash: String,
        drift_detected: bool,
        should_block: bool,
    },

    /// Environment drift detected against baseline
    DriftDetected {
        baseline_hash: String,
        severity: String, // "warning" or "critical"
        blocked: bool,
        fields: Vec<String>, // Field diffs in format "field:old->new"
    },

    /// Signing keypair loaded or generated
    KeypairLoaded {
        key_type: String, // "fingerprint_signing", "worker_signing", etc.
        key_id: String,   // Hash of public key for identification
    },

    /// Keypair was newly generated (first run)
    KeypairGenerated {
        key_type: String,
        key_id: String,
        key_path: String,
    },
}

/// Pre-boot audit log entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreBootAuditEntry {
    /// Unique entry ID
    pub id: String,
    /// Timestamp of the event
    pub timestamp: DateTime<Utc>,
    /// The event details
    #[serde(flatten)]
    pub event: PreBootAuditEvent,
    /// Process ID for correlation
    pub pid: u32,
    /// Hostname for multi-host deployments
    pub hostname: Option<String>,
}

impl PreBootAuditEntry {
    /// Create a new audit entry from an event
    pub fn new(event: PreBootAuditEvent) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            event,
            pid: std::process::id(),
            hostname: hostname::get().ok().and_then(|h| h.into_string().ok()),
        }
    }
}

/// Emit a pre-boot audit event to the file-based audit log.
///
/// This function appends the event to `var/pre_boot_audit.jsonl` as a single JSON line.
/// The directory is created if it doesn't exist.
///
/// # Arguments
///
/// * `event` - The audit event to log
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if the file could not be written.
pub fn emit_pre_boot_audit(event: PreBootAuditEvent) -> Result<()> {
    let entry = PreBootAuditEntry::new(event);
    let path = PathBuf::from(PRE_BOOT_AUDIT_PATH);

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Append to file
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;

    let json = serde_json::to_string(&entry)?;
    writeln!(file, "{}", json)?;

    debug!(
        id = %entry.id,
        event_type = %serde_json::to_value(&entry.event)
            .ok()
            .and_then(|v| v.get("event_type").and_then(|t| t.as_str()).map(String::from))
            .unwrap_or_default(),
        "Pre-boot audit event logged"
    );

    Ok(())
}

/// Read all pre-boot audit entries from the log file.
///
/// # Returns
///
/// Returns a vector of all audit entries, or an empty vector if the file doesn't exist.
pub fn read_pre_boot_audit_log() -> Result<Vec<PreBootAuditEntry>> {
    let path = PathBuf::from(PRE_BOOT_AUDIT_PATH);

    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = File::open(&path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<PreBootAuditEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                warn!(
                    line_num = line_num + 1,
                    error = %e,
                    "Failed to parse pre-boot audit entry, skipping"
                );
            }
        }
    }

    Ok(entries)
}

/// Replay pre-boot audit events to the database audit log.
///
/// This function reads all events from the pre-boot audit log and inserts them
/// into the database audit table. After successful replay, the pre-boot log
/// file is renamed with a `.replayed` suffix to prevent duplicate processing.
///
/// # Arguments
///
/// * `db` - Database connection for inserting audit events
///
/// # Returns
///
/// Returns the number of events replayed, or an error if replay failed.
pub async fn replay_pre_boot_audit_to_db(
    db: &adapteros_db::Db,
) -> Result<usize> {
    let entries = read_pre_boot_audit_log()?;

    if entries.is_empty() {
        debug!("No pre-boot audit entries to replay");
        return Ok(0);
    }

    let mut replayed = 0;
    for entry in &entries {
        // Convert event to audit log format
        let (action, resource_type, resource_id, metadata) = match &entry.event {
            PreBootAuditEvent::BaselineCreated {
                fingerprint_hash,
                baseline_path,
                creation_method,
            } => (
                "baseline.created",
                "baseline_fingerprint",
                fingerprint_hash.clone(),
                serde_json::json!({
                    "path": baseline_path,
                    "method": creation_method,
                }),
            ),
            PreBootAuditEvent::BaselineVerified {
                baseline_hash,
                drift_detected,
                should_block,
            } => (
                "baseline.verified",
                "baseline_fingerprint",
                baseline_hash.clone(),
                serde_json::json!({
                    "drift_detected": drift_detected,
                    "should_block": should_block,
                }),
            ),
            PreBootAuditEvent::DriftDetected {
                baseline_hash,
                severity,
                blocked,
                fields,
            } => (
                "baseline.drift_detected",
                "baseline_fingerprint",
                baseline_hash.clone(),
                serde_json::json!({
                    "severity": severity,
                    "blocked": blocked,
                    "fields": fields,
                }),
            ),
            PreBootAuditEvent::KeypairLoaded { key_type, key_id } => (
                "keypair.loaded",
                "signing_keypair",
                key_id.clone(),
                serde_json::json!({
                    "key_type": key_type,
                }),
            ),
            PreBootAuditEvent::KeypairGenerated {
                key_type,
                key_id,
                key_path,
            } => (
                "keypair.generated",
                "signing_keypair",
                key_id.clone(),
                serde_json::json!({
                    "key_type": key_type,
                    "key_path": key_path,
                }),
            ),
        };

        // Insert into audit log with original timestamp
        // Note: We use a special "system" tenant for pre-boot events
        if let Err(e) = db
            .log_audit_with_timestamp(
                "system",         // tenant_id - system events
                None,             // user_id - no user during boot
                None,             // role - no role during boot
                action,
                resource_type,
                &resource_id,
                "success",
                None, // error_message
                None, // ip_address
                Some(metadata.to_string()),
                entry.timestamp,
            )
            .await
        {
            warn!(
                entry_id = %entry.id,
                error = %e,
                "Failed to replay pre-boot audit entry to database"
            );
            continue;
        }

        replayed += 1;
    }

    // Rename file to mark as replayed
    if replayed > 0 {
        let path = PathBuf::from(PRE_BOOT_AUDIT_PATH);
        let replayed_path = path.with_extension("jsonl.replayed");

        // If there's already a replayed file, append to it
        if replayed_path.exists() {
            let mut replayed_file = OpenOptions::new()
                .append(true)
                .open(&replayed_path)?;

            let current_file = std::fs::read_to_string(&path)?;
            write!(replayed_file, "{}", current_file)?;
            std::fs::remove_file(&path)?;
        } else {
            std::fs::rename(&path, &replayed_path)?;
        }

        debug!(
            count = replayed,
            "Pre-boot audit events replayed to database"
        );
    }

    Ok(replayed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_emit_and_read_audit_events() {
        // This test would need to use a temp directory
        // For now, we just test serialization
        let event = PreBootAuditEvent::BaselineCreated {
            fingerprint_hash: "abc123".to_string(),
            baseline_path: "/var/baseline.json".to_string(),
            creation_method: "cli_flag".to_string(),
        };

        let entry = PreBootAuditEntry::new(event);
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: PreBootAuditEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.id, parsed.id);
        assert_eq!(entry.pid, parsed.pid);
    }

    #[test]
    fn test_event_serialization() {
        let events = vec![
            PreBootAuditEvent::BaselineCreated {
                fingerprint_hash: "hash1".to_string(),
                baseline_path: "/path".to_string(),
                creation_method: "env_var".to_string(),
            },
            PreBootAuditEvent::BaselineVerified {
                baseline_hash: "hash2".to_string(),
                drift_detected: false,
                should_block: false,
            },
            PreBootAuditEvent::DriftDetected {
                baseline_hash: "hash3".to_string(),
                severity: "critical".to_string(),
                blocked: true,
                fields: vec!["os_version:10->11".to_string()],
            },
            PreBootAuditEvent::KeypairLoaded {
                key_type: "fingerprint_signing".to_string(),
                key_id: "keyid1".to_string(),
            },
            PreBootAuditEvent::KeypairGenerated {
                key_type: "worker_signing".to_string(),
                key_id: "keyid2".to_string(),
                key_path: "/var/keys/key.bin".to_string(),
            },
        ];

        for event in events {
            let json = serde_json::to_string(&event).unwrap();
            let parsed: PreBootAuditEvent = serde_json::from_str(&json).unwrap();
            // Just verify it round-trips without error
            let _ = serde_json::to_string(&parsed).unwrap();
        }
    }
}
