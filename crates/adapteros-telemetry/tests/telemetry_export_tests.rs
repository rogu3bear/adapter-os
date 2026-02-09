//! Integration tests for telemetry export functionality
//!
//! Tests cover:
//! - Metric collection (counters, gauges, histograms)
//! - Prometheus export format validation
//! - Tenant-scoped metrics
//! - Bundle creation and signing
//! - UDS exporter functionality

use adapteros_telemetry::bundle::BundleWriter;
use adapteros_telemetry::bundle_store::{BundleStore, RetentionPolicy};
use adapteros_telemetry::metrics::critical_components::CriticalComponentMetrics;
use adapteros_telemetry::uds_exporter::{MetricMetadata, MetricValue, UdsMetricsExporter};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    adapteros_core::tempdir_in_var("aos-test-").expect("create temp dir")
}

#[cfg(test)]
mod metric_collection_tests {
    use super::*;

    #[test]
    fn test_counter_metrics_collection() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record various counter metrics
        metrics.record_metal_kernel_failure("fused_mlp", "timeout");
        metrics.record_metal_kernel_failure("matmul", "out_of_memory");
        metrics.inc_metal_kernel_failures("attention", "invalid_input", 3);

        let export = metrics.export().expect("Failed to export");

        // Verify counter metrics are present
        assert!(export.contains("metal_kernel_failures_total"));
        assert!(export.contains("fused_mlp"));
        assert!(export.contains("matmul"));
        assert!(export.contains("attention"));
    }

    #[test]
    fn test_gauge_metrics_collection() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record gauge metrics
        metrics.set_gpu_memory_pressure("gpu-0", 0.75);
        metrics.set_memory_pressure_ratio("gpu", 0.80);
        metrics.set_vram_usage_bytes("adapter-x", 1_073_741_824); // 1GB

        let export = metrics.export().expect("Failed to export");

        // Verify gauge metrics are present
        assert!(export.contains("gpu_memory_pressure"));
        assert!(export.contains("memory_pressure_ratio"));
        assert!(export.contains("vram_usage_bytes"));
        assert!(export.contains("0.75"));
        assert!(export.contains("0.8"));
    }

    #[test]
    fn test_histogram_metrics_collection() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record histogram metrics
        metrics.record_metal_kernel_execution_seconds("fused_mlp", "4096", 0.0015);
        metrics.record_metal_kernel_execution_seconds("attention", "2048", 0.0025);
        metrics.record_hotswap_latency_seconds("full_swap", true, 0.050);

        let export = metrics.export().expect("Failed to export");

        // Verify histogram metrics with buckets
        assert!(export.contains("metal_kernel_execution_seconds"));
        assert!(export.contains("hotswap_latency_seconds"));
        assert!(export.contains("_bucket"));
        assert!(export.contains("_sum"));
        assert!(export.contains("_count"));
    }

    #[test]
    fn test_hash_operation_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record hash operations with different size buckets
        metrics.record_hash_operation("blake3", "1KB");
        metrics.record_hash_operation("sha256", "10MB");
        metrics.inc_hash_operations("blake3", "100KB", 50);

        let export = metrics.export().expect("Failed to export");

        // Verify hash operations are tracked
        assert!(export.contains("hash_operations_total"));
        assert!(export.contains("blake3"));
        assert!(export.contains("sha256"));
    }

    #[test]
    fn test_hkdf_derivation_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record HKDF derivations
        metrics.record_hkdf_derivation("router");
        metrics.record_hkdf_derivation("dropout");
        metrics.inc_hkdf_derivations("sampling", 25);

        let export = metrics.export().expect("Failed to export");

        // Verify HKDF derivations are tracked
        assert!(export.contains("hkdf_derivations_total"));
        assert!(export.contains("router"));
        assert!(export.contains("dropout"));
        assert!(export.contains("sampling"));
    }

    #[test]
    fn test_adapter_lifecycle_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record lifecycle transitions
        metrics.record_adapter_lifecycle_transition("cold", "warm");
        metrics.record_adapter_lifecycle_transition("warm", "hot");
        metrics.record_adapter_lifecycle_transition("hot", "evicting");

        let export = metrics.export().expect("Failed to export");

        // Verify lifecycle transitions
        assert!(export.contains("adapter_lifecycle_transitions_total"));
        assert!(export.contains("cold"));
        assert!(export.contains("warm"));
        assert!(export.contains("hot"));
    }

    #[test]
    fn test_kv_cache_residency_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Set KV cache residency metrics
        metrics.set_kv_hot_entries(42);
        metrics.set_kv_cold_entries(128);
        metrics.set_kv_hot_bytes(104_857_600); // 100MB
        metrics.set_kv_cold_bytes(524_288_000); // 500MB

        // Record evictions
        metrics.record_kv_eviction("hot");
        metrics.record_kv_evictions("cold", 5);

        // Record quota events
        metrics.record_kv_quota_exceeded();
        metrics.record_kv_purgeable_failure();

        let export = metrics.export().expect("Failed to export");

        // Verify KV cache metrics
        assert!(export.contains("kv_hot_entries"));
        assert!(export.contains("kv_cold_entries"));
        assert!(export.contains("kv_hot_bytes"));
        assert!(export.contains("kv_cold_bytes"));
        assert!(export.contains("kv_evictions_by_residency_total"));
        assert!(export.contains("kv_quota_exceeded_total"));
        assert!(export.contains("kv_purgeable_failures_total"));
    }
}

#[cfg(test)]
mod prometheus_export_tests {
    use super::*;

    #[test]
    fn test_prometheus_text_format_structure() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record a simple counter
        metrics.record_metal_kernel_failure("test_kernel", "test_error");

        let export = metrics.export().expect("Failed to export");

        // Verify Prometheus format structure
        // Should contain HELP comment
        assert!(
            export.contains("# HELP metal_kernel_failures_total"),
            "Missing HELP comment"
        );

        // Should contain TYPE comment
        assert!(
            export.contains("# TYPE metal_kernel_failures_total counter"),
            "Missing TYPE comment"
        );

        // Should contain metric with labels
        assert!(
            export.contains("metal_kernel_failures_total{"),
            "Missing metric with labels"
        );
    }

    #[test]
    fn test_prometheus_histogram_format() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record histogram metric
        metrics.record_metal_kernel_execution_seconds("fused_mlp", "4096", 0.0015);

        let export = metrics.export().expect("Failed to export");

        // Verify histogram bucket format
        assert!(
            export.contains("metal_kernel_execution_seconds_bucket"),
            "Missing histogram buckets"
        );
        assert!(
            export.contains("le=\""),
            "Missing bucket label 'le' (less than or equal)"
        );
        assert!(
            export.contains("metal_kernel_execution_seconds_sum"),
            "Missing histogram sum"
        );
        assert!(
            export.contains("metal_kernel_execution_seconds_count"),
            "Missing histogram count"
        );
    }

    #[test]
    fn test_prometheus_label_formatting() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record metric with multiple labels
        metrics.record_db_query_duration("select", "adapters", "tenant-123", 0.025);

        let export = metrics.export().expect("Failed to export");

        // Verify label formatting: {label1="value1",label2="value2"}
        assert!(export.contains("tenant_id=\"tenant-123\""));
        assert!(export.contains("query_type=\"select\""));
        assert!(export.contains("table_name=\"adapters\""));
    }

    #[test]
    fn test_prometheus_metric_naming_conventions() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record various metrics
        metrics.record_model_cache_hit();
        metrics.record_determinism_violation_canonical("router", "unseeded_random");
        metrics.record_checkpoint_operation("save", true);

        // Record a time-based metric to verify _seconds suffix
        metrics.record_hotswap_latency_seconds("full_swap", true, 0.025);

        let export = metrics.export().expect("Failed to export");

        // Verify Prometheus naming conventions
        // Counters should end in _total
        assert!(export.contains("model_cache_hits_total"));
        assert!(export.contains("determinism_violation_total"));
        assert!(export.contains("checkpoint_operations_total"));

        // Time-based metrics should use _seconds suffix
        assert!(export.contains("hotswap_latency_seconds"));
    }

    #[test]
    fn test_prometheus_gauge_values() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Set specific gauge values
        metrics.set_executor_tick_counter(12345);
        metrics.set_hotswap_queue_depth(7);
        metrics.set_pinned_entries_count(3);

        let export = metrics.export().expect("Failed to export");

        // Verify gauge values are exported correctly
        assert!(export.contains("executor_tick_counter 12345"));
        assert!(export.contains("hotswap_queue_depth 7"));
        assert!(export.contains("model_cache_pinned_entries 3"));
    }

    #[test]
    fn test_prometheus_export_no_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Export without recording any metrics
        let export = metrics.export().expect("Failed to export");

        // Should still export valid Prometheus format with registered metrics at zero
        assert!(!export.is_empty(), "Export should not be empty");

        // Should contain at least the registered metric definitions
        // All metrics are pre-registered, so we should see HELP/TYPE comments
        assert!(export.contains("# HELP"));
        assert!(export.contains("# TYPE"));
    }
}

#[cfg(test)]
mod tenant_scoped_metrics_tests {
    use super::*;

    #[test]
    fn test_tenant_scoped_database_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record metrics for different tenants
        metrics.record_db_query_duration("select", "adapters", "tenant-1", 0.015);
        metrics.record_db_query_duration("select", "adapters", "tenant-2", 0.025);
        metrics.record_db_query_duration("insert", "policies", "tenant-1", 0.005);

        let export = metrics.export().expect("Failed to export");

        // Verify tenant labels are present
        assert!(export.contains("tenant_id=\"tenant-1\""));
        assert!(export.contains("tenant_id=\"tenant-2\""));

        // Verify metrics are scoped by tenant
        assert!(export.contains("db_query_duration_seconds"));
    }

    #[test]
    fn test_tenant_isolation_violation_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record tenant isolation violations
        metrics
            .tenant_isolation_violation_total
            .with_label_values(&["cross_tenant_access", "adapter"])
            .inc();
        metrics
            .tenant_isolation_violation_total
            .with_label_values(&["unauthorized_query", "database"])
            .inc();

        let export = metrics.export().expect("Failed to export");

        // Verify isolation violation tracking
        assert!(export.contains("tenant_isolation_violation_total"));
        assert!(export.contains("cross_tenant_access"));
        assert!(export.contains("unauthorized_query"));
    }

    #[test]
    fn test_tenant_query_error_tracking() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record tenant-specific query errors
        metrics.record_tenant_query_error("tenant-1", "select", "timeout");
        metrics.record_tenant_query_error("tenant-2", "insert", "constraint_violation");

        let export = metrics.export().expect("Failed to export");

        // Verify tenant error tracking
        assert!(export.contains("db_tenant_query_errors_total"));
        assert!(export.contains("tenant_id=\"tenant-1\""));
        assert!(export.contains("tenant_id=\"tenant-2\""));
        assert!(export.contains("timeout"));
        assert!(export.contains("constraint_violation"));
    }

    #[test]
    fn test_tenant_index_scan_metrics() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record index scans for different tenants
        metrics.record_db_index_scan("adapters", "idx_tenant_id", "range", "tenant-1");
        metrics.record_db_index_scan("policies", "idx_composite", "full", "tenant-2");

        let export = metrics.export().expect("Failed to export");

        // Verify tenant-scoped index scan tracking
        assert!(export.contains("db_index_scan_total"));
        assert!(export.contains("tenant_id=\"tenant-1\""));
        assert!(export.contains("tenant_id=\"tenant-2\""));
        assert!(export.contains("idx_tenant_id"));
        assert!(export.contains("idx_composite"));
    }

    #[test]
    fn test_system_tenant_label() {
        let metrics = Arc::new(CriticalComponentMetrics::new().expect("Failed to create metrics"));

        // Record system-wide query (non-tenant specific)
        let system_tenant = CriticalComponentMetrics::tenant_label_system();
        metrics.record_db_query_duration("select", "migrations", system_tenant, 0.010);

        let export = metrics.export().expect("Failed to export");

        // Verify system tenant label is used
        assert!(export.contains("tenant_id=\"system\""));
    }
}

#[cfg(test)]
mod bundle_creation_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_bundle_writer_basic_creation() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("bundles");

        let mut writer =
            BundleWriter::new(&output_dir, 10, 1024 * 1024).expect("Failed to create BundleWriter");

        // Write test events
        for i in 0..5 {
            let event = json!({
                "event_type": "test",
                "index": i,
                "message": format!("Test event {}", i)
            });
            writer.write_event(&event).expect("Failed to write event");
        }

        // Flush to create bundle
        writer.flush().expect("Failed to flush bundle");

        // Verify bundle files were created
        let bundle_files: Vec<_> = std::fs::read_dir(&output_dir)
            .expect("Failed to read output dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "ndjson" || ext == "sig")
                    .unwrap_or(false)
            })
            .collect();

        assert!(!bundle_files.is_empty(), "No bundle files created");
    }

    #[test]
    fn test_bundle_rotation_on_event_count() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("bundles");

        let max_events = 5;
        let mut writer = BundleWriter::new(&output_dir, max_events, 1024 * 1024)
            .expect("Failed to create BundleWriter");

        // Write events to trigger rotation
        for i in 0..12 {
            let event = json!({
                "event_type": "test",
                "index": i,
                "message": format!("Test event {}", i)
            });
            writer.write_event(&event).expect("Failed to write event");
        }

        writer.flush().expect("Failed to flush");

        // Should have created multiple bundles due to rotation
        let bundle_files: Vec<_> = std::fs::read_dir(&output_dir)
            .expect("Failed to read output dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "ndjson")
                    .unwrap_or(false)
            })
            .collect();

        // With 12 events and max_events=5, we should have at least 2 bundles
        assert!(
            bundle_files.len() >= 2,
            "Expected at least 2 bundles, got {}",
            bundle_files.len()
        );
    }

    #[test]
    fn test_bundle_signature_creation() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("bundles");

        let mut writer =
            BundleWriter::new(&output_dir, 5, 1024 * 1024).expect("Failed to create BundleWriter");

        // Write and flush
        for i in 0..3 {
            let event = json!({ "event_type": "test", "index": i });
            writer.write_event(&event).expect("Failed to write event");
        }
        writer.flush().expect("Failed to flush");

        // Find signature files
        let sig_files: Vec<_> = std::fs::read_dir(&output_dir)
            .expect("Failed to read output dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "sig")
                    .unwrap_or(false)
            })
            .collect();

        assert!(!sig_files.is_empty(), "No signature files created");

        // Verify signature file structure
        let sig_path = sig_files[0].path();
        let sig_content = std::fs::read_to_string(&sig_path).expect("Failed to read signature");
        let sig_json: serde_json::Value =
            serde_json::from_str(&sig_content).expect("Failed to parse signature");

        // Verify signature metadata fields
        assert!(sig_json.get("merkle_root").is_some());
        assert!(sig_json.get("signature").is_some());
        assert!(sig_json.get("public_key").is_some());
        assert!(sig_json.get("event_count").is_some());
        assert!(sig_json.get("sequence_no").is_some());
    }

    #[test]
    fn test_bundle_compression() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("bundles");

        // Create bundle writer with compression enabled
        let mut writer = BundleWriter::new(&output_dir, 100, 1024 * 1024)
            .expect("Failed to create BundleWriter");

        // Write enough events to trigger compression
        for i in 0..50 {
            let event = json!({
                "event_type": "test",
                "index": i,
                "message": format!("Test event with some repeated text: {}", "x".repeat(100))
            });
            writer.write_event(&event).expect("Failed to write event");
        }

        writer.flush().expect("Failed to flush");

        // Check compression statistics
        let stats = writer.compression_stats();
        // Note: Compression may not trigger if bundle is below min size threshold

        // Verify compression metadata is available in signatures if compression occurred
        let sig_files: Vec<_> = std::fs::read_dir(&output_dir)
            .expect("Failed to read output dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "sig")
                    .unwrap_or(false)
            })
            .collect();

        if !sig_files.is_empty() {
            let sig_content =
                std::fs::read_to_string(sig_files[0].path()).expect("Failed to read signature");
            let sig_json: serde_json::Value =
                serde_json::from_str(&sig_content).expect("Failed to parse signature");

            // Compression metadata is optional
            if sig_json.get("compression_metadata").is_some() {
                println!("Compression stats: {:?}", stats);
            }
        }
    }

    #[test]
    fn test_bundle_public_key_retrieval() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("bundles");

        let writer =
            BundleWriter::new(&output_dir, 10, 1024 * 1024).expect("Failed to create BundleWriter");

        // Get public key
        let public_key = writer.public_key();

        // Verify public key format (should be hex-encoded)
        assert!(!public_key.is_empty());
        assert!(public_key.chars().all(|c| c.is_ascii_hexdigit()));
        // Ed25519 public keys are 32 bytes = 64 hex chars
        assert_eq!(public_key.len(), 64);
    }
}

#[cfg(test)]
mod bundle_store_tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_telemetry::types::BundleMetadata;

    #[test]
    fn test_bundle_store_creation() {
        let temp_dir = new_test_tempdir();
        let store_dir = temp_dir.path().join("bundle_store");

        let policy = RetentionPolicy::default();
        let _store = BundleStore::new(&store_dir, policy).expect("Failed to create BundleStore");

        // Verify store directory exists
        assert!(store_dir.exists());
    }

    #[test]
    fn test_bundle_store_and_retrieve() {
        let temp_dir = new_test_tempdir();
        let store_dir = temp_dir.path().join("bundle_store");

        let policy = RetentionPolicy::default();
        let mut store = BundleStore::new(&store_dir, policy).expect("Failed to create BundleStore");

        // Create test bundle data
        let bundle_data = b"{\"event\":\"test\"}\n{\"event\":\"test2\"}\n";

        // Create metadata
        let metadata = BundleMetadata {
            bundle_hash: B3Hash::hash(bundle_data),
            merkle_root: B3Hash::hash(b"merkle_root"),
            event_count: 2,
            signature: "test_signature".to_string(),
            public_key: "test_public_key".to_string(),
            key_id: "test_key_id".to_string(),
            schema_version: 1,
            signed_at_us: 0,
            cpid: Some("test-cpid".to_string()),
            tenant_id: Some("tenant-123".to_string()),
            sequence_no: Some(1),
            created_at: std::time::SystemTime::now(),
            prev_bundle_hash: None,
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
            stack_id: None,
            stack_version: None,
        };

        // Store bundle
        let bundle_hash = store
            .store_bundle(bundle_data, metadata.clone())
            .expect("Failed to store bundle");

        // Retrieve bundle
        let retrieved_data = store
            .get_bundle(&bundle_hash)
            .expect("Failed to retrieve bundle");

        // Verify data matches
        assert_eq!(retrieved_data, bundle_data);

        // Verify metadata
        let retrieved_metadata = store
            .get_metadata(&bundle_hash)
            .expect("Metadata not found");
        assert_eq!(retrieved_metadata.event_count, 2);
        assert_eq!(retrieved_metadata.cpid, Some("test-cpid".to_string()));
        assert_eq!(retrieved_metadata.tenant_id, Some("tenant-123".to_string()));
    }

    #[test]
    fn test_tenant_scoped_bundle_storage() {
        let temp_dir = new_test_tempdir();
        let store_dir = temp_dir.path().join("bundle_store");

        let policy = RetentionPolicy::default();
        let mut store = BundleStore::new(&store_dir, policy).expect("Failed to create BundleStore");

        // Store bundles for different tenants
        for tenant in &["tenant-1", "tenant-2", "tenant-3"] {
            let bundle_data = format!("{{\"tenant\":\"{}\"}}\n", tenant).into_bytes();
            let metadata = BundleMetadata {
                bundle_hash: B3Hash::hash(&bundle_data),
                merkle_root: B3Hash::hash(b"test"),
                event_count: 1,
                signature: "sig".to_string(),
                public_key: "key".to_string(),
                key_id: "key_id".to_string(),
                schema_version: 1,
                signed_at_us: 0,
                cpid: Some("test-cpid".to_string()),
                tenant_id: Some(tenant.to_string()),
                sequence_no: Some(1),
                created_at: std::time::SystemTime::now(),
                prev_bundle_hash: None,
                is_incident_bundle: false,
                is_promotion_bundle: false,
                tags: vec![],
                stack_id: None,
                stack_version: None,
            };

            store
                .store_bundle(&bundle_data, metadata)
                .expect("Failed to store bundle");
        }

        // Verify tenant directories exist
        for tenant in &["tenant-1", "tenant-2", "tenant-3"] {
            let tenant_dir = store_dir.join(tenant).join("bundles");
            assert!(
                tenant_dir.exists(),
                "Tenant directory not created for {}",
                tenant
            );
        }
    }

    #[test]
    fn test_list_bundles_for_tenant() {
        let temp_dir = new_test_tempdir();
        let store_dir = temp_dir.path().join("bundle_store");

        let policy = RetentionPolicy::default();
        let mut store = BundleStore::new(&store_dir, policy).expect("Failed to create BundleStore");

        let tenant_id = "tenant-test";

        // Store multiple bundles for the same tenant
        for i in 0..3 {
            let bundle_data = format!("{{\"index\":{}}}\n", i).into_bytes();
            let metadata = BundleMetadata {
                bundle_hash: B3Hash::hash(&bundle_data),
                merkle_root: B3Hash::hash(b"test"),
                event_count: 1,
                signature: format!("sig_{}", i),
                public_key: "key".to_string(),
                key_id: "key_id".to_string(),
                schema_version: 1,
                signed_at_us: 0,
                cpid: Some("test-cpid".to_string()),
                tenant_id: Some(tenant_id.to_string()),
                sequence_no: Some(i),
                created_at: std::time::SystemTime::now(),
                prev_bundle_hash: None,
                is_incident_bundle: false,
                is_promotion_bundle: false,
                tags: vec![],
                stack_id: None,
                stack_version: None,
            };

            store
                .store_bundle(&bundle_data, metadata)
                .expect("Failed to store bundle");
        }

        // List bundles for tenant
        let bundles = store.list_bundles_for_tenant(tenant_id);

        // Should have 3 bundles
        assert_eq!(bundles.len(), 3);

        // All should belong to the same tenant
        for bundle in bundles {
            assert_eq!(bundle.tenant_id.as_deref(), Some(tenant_id));
        }
    }

    #[test]
    fn test_content_addressed_deduplication() {
        let temp_dir = new_test_tempdir();
        let store_dir = temp_dir.path().join("bundle_store");

        let policy = RetentionPolicy::default();
        let mut store = BundleStore::new(&store_dir, policy).expect("Failed to create BundleStore");

        // Same bundle data
        let bundle_data = b"{\"event\":\"duplicate\"}\n";

        let metadata1 = BundleMetadata {
            bundle_hash: B3Hash::hash(bundle_data),
            merkle_root: B3Hash::hash(b"test"),
            event_count: 1,
            signature: "sig1".to_string(),
            public_key: "key".to_string(),
            key_id: "key_id".to_string(),
            schema_version: 1,
            signed_at_us: 0,
            cpid: Some("test-cpid".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            sequence_no: Some(1),
            created_at: std::time::SystemTime::now(),
            prev_bundle_hash: None,
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
            stack_id: None,
            stack_version: None,
        };

        // Store first bundle
        let hash1 = store
            .store_bundle(bundle_data, metadata1)
            .expect("Failed to store bundle");

        // Try to store duplicate
        let metadata2 = BundleMetadata {
            bundle_hash: B3Hash::hash(bundle_data),
            merkle_root: B3Hash::hash(b"test"),
            event_count: 1,
            signature: "sig2".to_string(),
            public_key: "key".to_string(),
            key_id: "key_id".to_string(),
            schema_version: 1,
            signed_at_us: 0,
            cpid: Some("test-cpid".to_string()),
            tenant_id: Some("tenant-1".to_string()),
            sequence_no: Some(2),
            created_at: std::time::SystemTime::now(),
            prev_bundle_hash: None,
            is_incident_bundle: false,
            is_promotion_bundle: false,
            tags: vec![],
            stack_id: None,
            stack_version: None,
        };

        let hash2 = store
            .store_bundle(bundle_data, metadata2)
            .expect("Failed to store bundle");

        // Hashes should be identical (content-addressed)
        assert_eq!(hash1, hash2);
    }
}

#[cfg(test)]
mod uds_exporter_tests {
    use super::*;

    #[tokio::test]
    async fn test_uds_exporter_basic_setup() {
        let temp_dir = new_test_tempdir();
        let socket_path = temp_dir.path().join("metrics.sock");

        let mut exporter =
            UdsMetricsExporter::new(socket_path.clone()).expect("Failed to create exporter");

        // Bind to socket
        exporter.bind().await.expect("Failed to bind");

        // Verify socket file was created
        assert!(socket_path.exists());

        // Cleanup
        exporter.shutdown().await.expect("Failed to shutdown");
        assert!(!socket_path.exists());
    }

    #[tokio::test]
    async fn test_uds_exporter_metric_registration() {
        let temp_dir = new_test_tempdir();
        let socket_path = temp_dir.path().join("metrics.sock");

        let mut exporter = UdsMetricsExporter::new(socket_path).expect("Failed to create exporter");
        exporter.bind().await.expect("Failed to bind");

        // Register a counter metric
        exporter
            .register_metric(MetricMetadata {
                name: "test_counter".to_string(),
                help: "Test counter metric".to_string(),
                metric_type: "counter".to_string(),
                labels: HashMap::new(),
                value: MetricValue::Counter(0.0),
            })
            .await;

        // Increment counter
        exporter
            .increment_counter("test_counter", 5.0)
            .await
            .expect("Failed to increment counter");

        // Register a gauge metric
        exporter
            .register_metric(MetricMetadata {
                name: "test_gauge".to_string(),
                help: "Test gauge metric".to_string(),
                metric_type: "gauge".to_string(),
                labels: HashMap::new(),
                value: MetricValue::Gauge(0.0),
            })
            .await;

        // Set gauge value
        exporter
            .set_gauge("test_gauge", 42.5)
            .await
            .expect("Failed to set gauge");

        exporter.shutdown().await.expect("Failed to shutdown");
    }

    #[tokio::test]
    async fn test_uds_exporter_with_labels() {
        let temp_dir = new_test_tempdir();
        let socket_path = temp_dir.path().join("metrics.sock");

        let mut exporter = UdsMetricsExporter::new(socket_path).expect("Failed to create exporter");
        exporter.bind().await.expect("Failed to bind");

        // Register metric with labels
        let mut labels = HashMap::new();
        labels.insert("tenant_id".to_string(), "tenant-123".to_string());
        labels.insert("operation".to_string(), "inference".to_string());

        exporter
            .register_metric(MetricMetadata {
                name: "requests_total".to_string(),
                help: "Total requests".to_string(),
                metric_type: "counter".to_string(),
                labels,
                value: MetricValue::Counter(0.0),
            })
            .await;

        exporter.shutdown().await.expect("Failed to shutdown");
    }

    // Note: Prometheus format generation tests are in the uds_exporter module's internal tests
    // since format_prometheus_metrics is a private method. We test the public API here instead.
}

#[cfg(test)]
mod telemetry_writer_shutdown_tests {
    use super::*;
    use adapteros_core::identity::IdentityEnvelope;
    use adapteros_telemetry::unified_events::{
        EventType, LogLevel, TelemetryEvent, TelemetryEventBuilder,
    };
    use adapteros_telemetry::TelemetryWriter;
    use std::time::Duration;

    fn make_test_event() -> TelemetryEvent {
        let identity = IdentityEnvelope::new(
            "system".to_string(),
            "test".to_string(),
            "telemetry-test".to_string(),
            "1".to_string(),
        );
        TelemetryEventBuilder::new(
            EventType::SystemStart,
            LogLevel::Info,
            "test event".to_string(),
            identity,
        )
        .build()
        .expect("build event")
    }

    #[test]
    fn test_telemetry_writer_shutdown_graceful() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("telemetry");

        let writer =
            TelemetryWriter::new(&output_dir, 100, 1024 * 1024).expect("create telemetry writer");

        // Write a few events
        for _ in 0..3 {
            writer.log_event(make_test_event()).expect("log event");
        }

        // Graceful shutdown should succeed
        writer.shutdown().expect("shutdown should succeed");

        // Verify bundles were written
        assert!(
            output_dir.exists(),
            "output dir should exist after shutdown"
        );
    }

    #[test]
    fn test_telemetry_writer_shutdown_with_timeout() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("telemetry_timeout");

        let writer =
            TelemetryWriter::new(&output_dir, 100, 1024 * 1024).expect("create telemetry writer");

        // Write events
        writer.log_event(make_test_event()).expect("log event");

        // Shutdown with explicit timeout
        let result = writer.shutdown_with_timeout(Duration::from_secs(10));
        assert!(result.is_ok(), "shutdown with timeout should succeed");
    }

    #[test]
    fn test_telemetry_writer_flush() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("telemetry_flush");

        let writer =
            TelemetryWriter::new(&output_dir, 100, 1024 * 1024).expect("create telemetry writer");

        // Write events
        for _ in 0..5 {
            writer.log_event(make_test_event()).expect("log event");
        }

        // Flush should succeed
        writer.flush().expect("flush should succeed");

        // Shutdown after flush
        writer.shutdown().expect("shutdown should succeed");
    }

    #[test]
    fn test_telemetry_writer_flush_with_timeout() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("telemetry_flush_timeout");

        let writer =
            TelemetryWriter::new(&output_dir, 100, 1024 * 1024).expect("create telemetry writer");

        // Write events
        writer.log_event(make_test_event()).expect("log event");

        // Flush with timeout should succeed
        let result = writer.flush_with_timeout(Duration::from_secs(5));
        assert!(result.is_ok(), "flush with timeout should succeed");

        // Cleanup
        writer.shutdown().expect("shutdown should succeed");
    }

    #[test]
    fn test_telemetry_writer_empty_shutdown() {
        let temp_dir = new_test_tempdir();
        let output_dir = temp_dir.path().join("telemetry_empty");

        let writer =
            TelemetryWriter::new(&output_dir, 100, 1024 * 1024).expect("create telemetry writer");

        // Shutdown without writing any events should succeed
        writer.shutdown().expect("empty shutdown should succeed");
    }
}
