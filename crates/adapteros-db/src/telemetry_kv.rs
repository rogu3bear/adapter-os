//! KV helpers for telemetry parity and migration.

use crate::telemetry_bundles::{TelemetryBatchParams, TelemetryBundle, TelemetryRecord};
use crate::Db;
use adapteros_core::telemetry::dual_write_divergence_event;
use adapteros_core::{AosError, Result};
use adapteros_storage::{TelemetryBundleKv, TelemetryEventKv, TelemetryRepository};
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

static TELEMETRY_DRIFT_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Db {
    pub(crate) fn telemetry_repo(&self) -> Option<TelemetryRepository> {
        self.kv_backend()
            .map(|kv| TelemetryRepository::new(kv.backend().clone(), kv.index_manager().clone()))
    }

    pub(crate) fn telemetry_repo_if_write(&self) -> Option<TelemetryRepository> {
        if self.storage_mode().write_to_kv() {
            self.telemetry_repo()
        } else {
            None
        }
    }

    pub(crate) fn telemetry_repo_if_read(&self) -> Option<TelemetryRepository> {
        if self.storage_mode().read_from_kv() {
            self.telemetry_repo()
        } else {
            None
        }
    }

    pub(crate) fn kv_event_from_params(
        &self,
        id: &str,
        params: &TelemetryBatchParams,
    ) -> TelemetryEventKv {
        let normalized_ts = normalize_timestamp(&params.timestamp);
        let seq = format!("{}-{}", normalized_ts, id);

        TelemetryEventKv {
            id: id.to_string(),
            tenant_id: params.tenant_id.clone(),
            event_type: params.event_type.clone(),
            event_data: params.event_data.clone(),
            timestamp: params.timestamp.clone(),
            source: params.source.clone(),
            user_id: params.user_id.clone(),
            session_id: params.session_id.clone(),
            metadata: params.metadata.clone(),
            tags: params.tags.clone(),
            priority: params.priority.clone(),
            seq,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    pub(crate) fn kv_event_to_record(event: TelemetryEventKv) -> Result<TelemetryRecord> {
        Ok(TelemetryRecord {
            id: event.id,
            tenant_id: event.tenant_id,
            event_type: event.event_type,
            event_data: serde_json::to_string(&event.event_data)
                .map_err(AosError::Serialization)?,
            timestamp: event.timestamp,
            source: event.source,
            user_id: event.user_id,
            session_id: event.session_id,
            metadata: event.metadata.map(|m| m.to_string()),
            tags: event.tags.map(|t| t.to_string()),
            priority: event.priority,
            created_at: event.created_at,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn kv_bundle_from_record(bundle: &TelemetryBundle) -> TelemetryBundleKv {
        TelemetryBundleKv {
            id: bundle.id.clone(),
            tenant_id: bundle.tenant_id.clone(),
            cpid: bundle.cpid.clone(),
            path: bundle.path.clone(),
            merkle_root_b3: bundle.merkle_root_b3.clone(),
            start_seq: bundle.start_seq,
            end_seq: bundle.end_seq,
            event_count: bundle.event_count,
            created_at: bundle.created_at.clone(),
            signature_b64: None,
            chunk_count: None,
            chunk_size_bytes: None,
        }
    }

    pub(crate) fn kv_bundle_to_record(bundle: TelemetryBundleKv) -> TelemetryBundle {
        TelemetryBundle {
            id: bundle.id,
            tenant_id: bundle.tenant_id,
            cpid: bundle.cpid,
            path: bundle.path,
            merkle_root_b3: bundle.merkle_root_b3,
            start_seq: bundle.start_seq,
            end_seq: bundle.end_seq,
            event_count: bundle.event_count,
            created_at: bundle.created_at,
        }
    }
}

#[allow(dead_code)]
pub fn telemetry_drift_count() -> u64 {
    TELEMETRY_DRIFT_COUNTER.load(Ordering::Relaxed)
}

pub(crate) fn record_telemetry_drift(reason: &str) {
    warn!(reason = reason, "Telemetry KV/SQL drift detected");
    TELEMETRY_DRIFT_COUNTER.fetch_add(1, Ordering::Relaxed);

    // Emit structured observability event for alerting/correlation
    let _event = dual_write_divergence_event(
        "telemetry", // table
        reason,      // key (using reason as identifier)
        "sql",       // primary backend
        "kv",        // secondary backend
        1,           // attempt count
        None,        // tenant_id not available at this callsite
    );

    debug!(
        drift_total = telemetry_drift_count(),
        "Telemetry drift counter updated"
    );
}

/// Normalize a timestamp string to microseconds for deterministic sequencing.
///
/// Accepts ISO 8601 timestamps (e.g., "2024-01-15T10:30:00.123456Z") and converts
/// them to microseconds since UNIX epoch. Falls back to digit filtering for
/// already-numeric timestamps or malformed inputs.
fn normalize_timestamp(ts: &str) -> String {
    // Fast path: if already all digits, return as-is
    if !ts.is_empty() && ts.chars().all(|c| c.is_ascii_digit()) {
        return ts.to_string();
    }

    // Try parsing as ISO 8601 (RFC 3339)
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        return dt.timestamp_micros().to_string();
    }

    // Try parsing without timezone (assume UTC)
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.f") {
        return dt.and_utc().timestamp_micros().to_string();
    }

    // Fallback: filter to digits only (backward compat for legacy formats)
    let filtered: String = ts.chars().filter(|c| c.is_ascii_digit()).collect();
    if filtered.is_empty() {
        warn!(
            timestamp = %ts,
            "Timestamp normalization failed: no digits found, using epoch"
        );
        "0".to_string()
    } else {
        debug!(
            original = %ts,
            normalized = %filtered,
            "Timestamp normalized via digit filtering (legacy path)"
        );
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_timestamp_already_numeric() {
        assert_eq!(normalize_timestamp("1234567890"), "1234567890");
        assert_eq!(normalize_timestamp("0"), "0");
    }

    #[test]
    fn test_normalize_timestamp_iso8601() {
        // RFC 3339 with timezone
        let ts = "2024-01-15T10:30:00.123456Z";
        let normalized = normalize_timestamp(ts);
        // Should be microseconds since epoch
        assert!(normalized.len() >= 16, "Should be microseconds: {}", normalized);
        assert!(normalized.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_normalize_timestamp_iso8601_no_tz() {
        // ISO 8601 without timezone
        let ts = "2024-01-15T10:30:00.123456";
        let normalized = normalize_timestamp(ts);
        assert!(normalized.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_normalize_timestamp_legacy_fallback() {
        // Legacy format with mixed chars - falls back to digit filtering
        let ts = "2024-01-15_10:30:00";
        let normalized = normalize_timestamp(ts);
        assert_eq!(normalized, "20240115103000");
    }

    #[test]
    fn test_normalize_timestamp_empty() {
        assert_eq!(normalize_timestamp(""), "0");
    }

    #[test]
    fn test_normalize_timestamp_no_digits() {
        assert_eq!(normalize_timestamp("no-digits-here"), "0");
    }
}
