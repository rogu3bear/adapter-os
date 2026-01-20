//! Bundle event parsing for telemetry filtering
//!
//! Provides parsing of NDJSON telemetry bundles with support for:
//! - Plain `.ndjson` files
//! - Compressed formats: `.zst`, `.gz`, `.lz4`
//! - Associated metadata files (`.meta.json`, `.sig`)
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_telemetry::bundle_parser::{parse_bundle_events, BundleEventFilter};
//!
//! let events = parse_bundle_events(bundle_path)?;
//! let filtered = BundleEventFilter::new()
//!     .by_stack("stack-prod-001")
//!     .by_event_type("router.decision")
//!     .apply(&events);
//! ```

use crate::compression::{CompressionAlgorithm, TelemetryCompressor};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Parsed telemetry event from a bundle file
///
/// This is a lightweight representation of events extracted from bundles,
/// suitable for filtering and display in the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    /// Unique event identifier
    pub id: String,
    /// Event type (e.g., "router.decision", "inference.complete")
    pub event_type: String,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Human-readable message
    pub message: String,
    /// Tenant ID from identity envelope
    pub tenant_id: Option<String>,
    /// Stack ID from metadata or bundle metadata
    pub stack_id: Option<String>,
    /// Log level (Debug, Info, Warn, Error, Critical)
    pub level: Option<String>,
    /// Component that generated the event
    pub component: Option<String>,
    /// Full metadata for advanced queries
    pub metadata: Option<serde_json::Value>,
    /// User ID if present
    pub user_id: Option<String>,
    /// Trace ID for distributed tracing
    pub trace_id: Option<String>,
}

/// Filter configuration for bundle events
#[derive(Debug, Clone, Default)]
pub struct BundleEventFilter {
    /// Filter by stack ID
    pub stack_id: Option<String>,
    /// Filter by event type (supports prefix matching)
    pub event_type: Option<String>,
    /// Filter by tenant ID
    pub tenant_id: Option<String>,
    /// Filter by log level
    pub level: Option<String>,
    /// Filter by component
    pub component: Option<String>,
    /// Maximum events to return
    pub limit: Option<usize>,
}

impl BundleEventFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by stack ID
    pub fn by_stack(mut self, stack_id: impl Into<String>) -> Self {
        self.stack_id = Some(stack_id.into());
        self
    }

    /// Filter by event type (supports prefix matching, e.g., "router" matches "router.decision")
    pub fn by_event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    /// Filter by tenant ID
    pub fn by_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Filter by log level
    pub fn by_level(mut self, level: impl Into<String>) -> Self {
        self.level = Some(level.into());
        self
    }

    /// Filter by component
    pub fn by_component(mut self, component: impl Into<String>) -> Self {
        self.component = Some(component.into());
        self
    }

    /// Set maximum events to return
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Check if an event matches this filter
    pub fn matches(&self, event: &TelemetryEvent) -> bool {
        // Stack ID filter - check both stack_id field and tenant_id as fallback
        if let Some(ref filter_stack) = self.stack_id {
            let matches_stack = event
                .stack_id
                .as_ref()
                .map(|s| s == filter_stack)
                .unwrap_or(false);
            let matches_tenant = event
                .tenant_id
                .as_ref()
                .map(|t| t == filter_stack)
                .unwrap_or(false);

            if !matches_stack && !matches_tenant {
                return false;
            }
        }

        // Event type filter - supports prefix matching
        if let Some(ref filter_type) = self.event_type {
            if !event.event_type.contains(filter_type) {
                return false;
            }
        }

        // Tenant ID filter
        if let Some(ref filter_tenant) = self.tenant_id {
            if event
                .tenant_id
                .as_ref()
                .map(|t| t != filter_tenant)
                .unwrap_or(true)
            {
                return false;
            }
        }

        // Log level filter (case-insensitive)
        if let Some(ref filter_level) = self.level {
            if let Some(ref event_level) = event.level {
                if !event_level.eq_ignore_ascii_case(filter_level) {
                    return false;
                }
            } else {
                return false;
            }
        }

        // Component filter
        if let Some(ref filter_component) = self.component {
            if event
                .component
                .as_ref()
                .map(|c| !c.contains(filter_component))
                .unwrap_or(true)
            {
                return false;
            }
        }

        true
    }

    /// Apply filter to a list of events
    pub fn apply(&self, events: &[TelemetryEvent]) -> Vec<TelemetryEvent> {
        let filtered = events.iter().filter(|e| self.matches(e)).cloned();

        match self.limit {
            Some(limit) => filtered.take(limit).collect(),
            None => filtered.collect(),
        }
    }
}

/// Parse events from a bundle file
///
/// Supports multiple formats:
/// - Plain NDJSON (`.ndjson`)
/// - Zstd compressed (`.ndjson.zst`)
/// - Gzip compressed (`.ndjson.gz`)
/// - LZ4 compressed (`.ndjson.lz4`)
///
/// Also attempts to load stack_id from associated metadata files.
pub fn parse_bundle_events(bundle_path: &Path) -> Result<Vec<TelemetryEvent>> {
    // Determine the actual file to read and compression algorithm
    let (file_path, compression) = resolve_bundle_file(bundle_path)?;

    // Read and decompress the file
    let content = read_bundle_content(&file_path, compression)?;

    // Load bundle metadata for stack_id if available
    let bundle_stack_id = load_bundle_stack_id(bundle_path);

    // Parse NDJSON content
    parse_ndjson_content(&content, bundle_stack_id.as_deref())
}

/// Parse events from raw bundle content (for testing or streaming)
pub fn parse_bundle_content(
    content: &[u8],
    bundle_stack_id: Option<&str>,
) -> Result<Vec<TelemetryEvent>> {
    let content_str = String::from_utf8_lossy(content);
    parse_ndjson_content(&content_str, bundle_stack_id)
}

/// Resolve the actual bundle file path and compression algorithm
fn resolve_bundle_file(bundle_path: &Path) -> Result<(std::path::PathBuf, CompressionAlgorithm)> {
    // If the path exists as-is, check its extension
    if bundle_path.exists() {
        let extension = bundle_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let compression = match extension {
            "zst" => CompressionAlgorithm::Zstd,
            "gz" => CompressionAlgorithm::Gzip,
            "lz4" => CompressionAlgorithm::Lz4,
            _ => CompressionAlgorithm::None,
        };

        return Ok((bundle_path.to_path_buf(), compression));
    }

    // Try compressed variants
    let compressed_variants = [
        (
            bundle_path.with_extension("ndjson.zst"),
            CompressionAlgorithm::Zstd,
        ),
        (
            bundle_path.with_extension("ndjson.gz"),
            CompressionAlgorithm::Gzip,
        ),
        (
            bundle_path.with_extension("ndjson.lz4"),
            CompressionAlgorithm::Lz4,
        ),
    ];

    for (path, compression) in compressed_variants {
        if path.exists() {
            return Ok((path, compression));
        }
    }

    // Also try with just the compression extension added
    let base_path_str = bundle_path.to_string_lossy();
    let additional_variants = [
        (format!("{}.zst", base_path_str), CompressionAlgorithm::Zstd),
        (format!("{}.gz", base_path_str), CompressionAlgorithm::Gzip),
        (format!("{}.lz4", base_path_str), CompressionAlgorithm::Lz4),
    ];

    for (path_str, compression) in additional_variants {
        let path = std::path::PathBuf::from(&path_str);
        if path.exists() {
            return Ok((path, compression));
        }
    }

    Err(AosError::Io(format!(
        "Bundle file not found: {} (also tried compressed variants)",
        bundle_path.display()
    )))
}

/// Read and decompress bundle content
fn read_bundle_content(file_path: &Path, compression: CompressionAlgorithm) -> Result<String> {
    let raw_bytes = fs::read(file_path)
        .map_err(|e| AosError::Io(format!("Failed to read bundle file: {}", e)))?;

    let decompressed = match compression {
        CompressionAlgorithm::None => raw_bytes,
        CompressionAlgorithm::Zstd => {
            let compressor = TelemetryCompressor::with_config(
                CompressionAlgorithm::Zstd,
                crate::compression::CompressionLevel::DEFAULT,
            );
            compressor.decompress(&raw_bytes)?
        }
        CompressionAlgorithm::Gzip => {
            let compressor = TelemetryCompressor::with_config(
                CompressionAlgorithm::Gzip,
                crate::compression::CompressionLevel::DEFAULT,
            );
            compressor.decompress(&raw_bytes)?
        }
        CompressionAlgorithm::Lz4 => {
            let compressor = TelemetryCompressor::with_config(
                CompressionAlgorithm::Lz4,
                crate::compression::CompressionLevel::DEFAULT,
            );
            compressor.decompress(&raw_bytes)?
        }
    };

    String::from_utf8(decompressed)
        .map_err(|e| AosError::Io(format!("Bundle content is not valid UTF-8: {}", e)))
}

/// Load stack_id from bundle metadata file
fn load_bundle_stack_id(bundle_path: &Path) -> Option<String> {
    // Try .meta.json file
    let meta_path = bundle_path.with_extension("meta.json");
    if let Ok(meta_content) = fs::read_to_string(&meta_path) {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_content) {
            if let Some(stack_id) = meta.get("stack_id").and_then(|v| v.as_str()) {
                return Some(stack_id.to_string());
            }
        }
    }

    // Try .ndjson.sig file (SignatureMetadata format doesn't have stack_id, but check anyway)
    // SignatureMetadata is in bundle.rs - it doesn't have stack_id, skip this

    None
}

/// Parse NDJSON content into TelemetryEvent structs
fn parse_ndjson_content(
    content: &str,
    bundle_stack_id: Option<&str>,
) -> Result<Vec<TelemetryEvent>> {
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match serde_json::from_str::<serde_json::Value>(line) {
            Ok(event_json) => {
                let event = extract_telemetry_event(&event_json, bundle_stack_id);
                events.push(event);
            }
            Err(e) => {
                // Log parsing error but continue with other events
                tracing::debug!(error = %e, "Failed to parse event line, skipping");
            }
        }
    }

    Ok(events)
}

/// Extract TelemetryEvent fields from raw JSON
fn extract_telemetry_event(
    json: &serde_json::Value,
    bundle_stack_id: Option<&str>,
) -> TelemetryEvent {
    // Extract ID - try common field names
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("event_id").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    // Extract event_type - handle both string and enum formats
    let event_type = json
        .get("event_type")
        .and_then(|v| {
            // Could be a string or an object like {"Custom": "..."}
            v.as_str()
                .map(String::from)
                .or_else(|| v.get("Custom").and_then(|c| c.as_str()).map(String::from))
        })
        .or_else(|| {
            json.get("ev_type")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default();

    // Extract timestamp - try multiple field names
    let timestamp = json
        .get("timestamp")
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| json.get("ts").and_then(|v| v.as_str()).map(String::from))
        .or_else(|| {
            // Handle numeric timestamps
            json.get("timestamp")
                .and_then(|v| v.as_i64())
                .map(|ts| ts.to_string())
        })
        .unwrap_or_default();

    // Extract message
    let message = json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Extract tenant_id from identity envelope
    let tenant_id = json
        .get("identity")
        .and_then(|identity| identity.get("tenant_id"))
        .and_then(|v| v.as_str())
        .or_else(|| json.get("tenant_id").and_then(|v| v.as_str()))
        .map(String::from);

    // Extract stack_id - try multiple locations
    let stack_id = json
        .get("metadata")
        .and_then(|meta| meta.get("stack_id"))
        .and_then(|v| v.as_str())
        .or_else(|| json.get("stack_id").and_then(|v| v.as_str()))
        .map(String::from)
        .or_else(|| bundle_stack_id.map(String::from));

    // Extract log level
    let level = json
        .get("level")
        .and_then(|v| {
            // Handle both string and enum formats
            v.as_str().map(String::from).or_else(|| {
                // Try to stringify the value if it's an object
                serde_json::to_string(v).ok()
            })
        })
        .map(|s| s.trim_matches('"').to_string());

    // Extract component
    let component = json
        .get("component")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract user_id
    let user_id = json
        .get("user_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Extract trace_id
    let trace_id = json
        .get("trace_id")
        .and_then(|v| v.as_str())
        .map(String::from);

    // Keep full metadata for advanced queries
    let metadata = json.get("metadata").cloned();

    TelemetryEvent {
        id,
        event_type,
        timestamp,
        message,
        tenant_id,
        stack_id,
        level,
        component,
        metadata,
        user_id,
        trace_id,
    }
}

/// Statistics about parsed bundle
#[derive(Debug, Clone, Serialize)]
pub struct BundleParseStats {
    /// Total events parsed
    pub total_events: usize,
    /// Events that matched filter
    pub matched_events: usize,
    /// Bundles scanned
    pub bundles_scanned: usize,
    /// Parse errors encountered
    pub parse_errors: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_by_stack() {
        let events = vec![
            TelemetryEvent {
                id: "1".to_string(),
                event_type: "test".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: Some("tenant-1".to_string()),
                stack_id: Some("stack-prod-001".to_string()),
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
            TelemetryEvent {
                id: "2".to_string(),
                event_type: "test".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: Some("tenant-2".to_string()),
                stack_id: Some("stack-dev-001".to_string()),
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
        ];

        let filter = BundleEventFilter::new().by_stack("stack-prod-001");
        let filtered = filter.apply(&events);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].stack_id, Some("stack-prod-001".to_string()));
    }

    #[test]
    fn test_filter_by_event_type() {
        let events = vec![
            TelemetryEvent {
                id: "1".to_string(),
                event_type: "router.decision".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: None,
                stack_id: None,
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
            TelemetryEvent {
                id: "2".to_string(),
                event_type: "inference.complete".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: None,
                stack_id: None,
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
        ];

        let filter = BundleEventFilter::new().by_event_type("router");
        let filtered = filter.apply(&events);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_type, "router.decision");
    }

    #[test]
    fn test_combined_filters() {
        let events = vec![
            TelemetryEvent {
                id: "1".to_string(),
                event_type: "router.decision".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: None,
                stack_id: Some("stack-prod".to_string()),
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
            TelemetryEvent {
                id: "2".to_string(),
                event_type: "router.decision".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: None,
                stack_id: Some("stack-dev".to_string()),
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
            TelemetryEvent {
                id: "3".to_string(),
                event_type: "inference.complete".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: None,
                stack_id: Some("stack-prod".to_string()),
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            },
        ];

        let filter = BundleEventFilter::new()
            .by_stack("stack-prod")
            .by_event_type("router");
        let filtered = filter.apply(&events);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "1");
    }

    #[test]
    fn test_filter_with_limit() {
        let events: Vec<TelemetryEvent> = (0..10)
            .map(|i| TelemetryEvent {
                id: i.to_string(),
                event_type: "test".to_string(),
                timestamp: "2024-01-01".to_string(),
                message: "test".to_string(),
                tenant_id: None,
                stack_id: None,
                level: None,
                component: None,
                metadata: None,
                user_id: None,
                trace_id: None,
            })
            .collect();

        let filter = BundleEventFilter::new().with_limit(5);
        let filtered = filter.apply(&events);

        assert_eq!(filtered.len(), 5);
    }

    #[test]
    fn test_parse_ndjson_content() {
        let content = r#"{"id":"evt-1","event_type":"router.decision","timestamp":"2024-01-01T00:00:00Z","message":"Test event","identity":{"tenant_id":"tenant-1"}}
{"id":"evt-2","event_type":"inference.complete","timestamp":"2024-01-01T00:00:01Z","message":"Second event","metadata":{"stack_id":"stack-001"}}"#;

        let events = parse_ndjson_content(content, None).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "evt-1");
        assert_eq!(events[0].event_type, "router.decision");
        assert_eq!(events[0].tenant_id, Some("tenant-1".to_string()));
        assert_eq!(events[1].stack_id, Some("stack-001".to_string()));
    }

    #[test]
    fn test_extract_event_with_bundle_stack_id_fallback() {
        let json: serde_json::Value = serde_json::json!({
            "id": "evt-1",
            "event_type": "test",
            "message": "test"
        });

        let event = extract_telemetry_event(&json, Some("bundle-stack-id"));
        assert_eq!(event.stack_id, Some("bundle-stack-id".to_string()));
    }

    #[test]
    fn test_filter_fallback_to_tenant_id() {
        // When stack_id is not present, should fall back to tenant_id
        let events = vec![TelemetryEvent {
            id: "1".to_string(),
            event_type: "test".to_string(),
            timestamp: "2024-01-01".to_string(),
            message: "test".to_string(),
            tenant_id: Some("my-stack".to_string()),
            stack_id: None,
            level: None,
            component: None,
            metadata: None,
            user_id: None,
            trace_id: None,
        }];

        let filter = BundleEventFilter::new().by_stack("my-stack");
        let filtered = filter.apply(&events);

        assert_eq!(filtered.len(), 1);
    }
}
