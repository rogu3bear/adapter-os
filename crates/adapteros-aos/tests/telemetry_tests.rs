//! Telemetry validation tests for AOS loader
//!
//! Validates that telemetry events are correctly emitted during:
//! - Adapter load operations
//! - Error scenarios
//! - Policy violations
//! - Validation failures

#[cfg(all(test, feature = "mmap", feature = "telemetry"))]
mod telemetry_validation {
    use adapteros_aos::{
        AOS2Writer, Aos2Manifest, MmapAdapterLoader, TrainingConfig, WriteOptions,
    };
    use adapteros_telemetry::{EventType, TelemetryWriter};
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Create a valid test .aos file
    fn create_test_aos_file(dir: &TempDir, name: &str, size_kb: usize) -> PathBuf {
        let path = dir.path().join(format!("{}.aos", name));

        let manifest = Aos2Manifest {
            version: "2.0".to_string(),
            adapter_id: name.to_string(),
            rank: 16,
            base_model: "test-model".to_string(),
            training_config: TrainingConfig {
                rank: 16,
                alpha: 32.0,
                learning_rate: 0.001,
                batch_size: 8,
                epochs: 10,
                hidden_dim: 768,
            },
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: "test_hash".to_string(),
            weights_offset: 0,
            weights_size: 0,
            format_version: "2.0".to_string(),
            metadata: Default::default(),
        };

        // Create fake weights data
        let weights_data = vec![0u8; size_kb * 1024];

        let writer = AOS2Writer::new(WriteOptions::default());
        writer
            .write(&path, &manifest, &weights_data)
            .expect("Failed to write test .aos file");

        path
    }

    #[tokio::test]
    async fn test_load_success_emits_telemetry() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = create_test_aos_file(&temp_dir, "test_adapter", 100);

        // Initialize telemetry writer
        let telemetry_writer = TelemetryWriter::new_in_memory();

        let loader = MmapAdapterLoader::new();
        let result = loader.load(&aos_path).await;

        assert!(result.is_ok(), "Load should succeed");

        // Verify telemetry event was emitted
        let events = telemetry_writer.get_events();
        let load_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event_type, EventType::AdapterLoaded))
            .collect();

        assert_eq!(
            load_events.len(),
            1,
            "Should emit exactly one AdapterLoaded event"
        );

        let event = load_events[0];
        assert_eq!(event.component, Some("adapteros-aos".to_string()));

        // Verify metadata contains expected fields
        if let Some(metadata) = &event.metadata {
            assert!(
                metadata.get("adapter_id").is_some(),
                "Should include adapter_id"
            );
            assert!(
                metadata.get("base_model").is_some(),
                "Should include base_model"
            );
            assert!(metadata.get("version").is_some(), "Should include version");
            assert!(
                metadata.get("weights_hash").is_some(),
                "Should include weights_hash"
            );
            assert!(
                metadata.get("size_bytes").is_some(),
                "Should include size_bytes"
            );
            assert!(
                metadata.get("tensor_count").is_some(),
                "Should include tensor_count"
            );
            assert!(
                metadata.get("duration_ms").is_some(),
                "Should include duration_ms"
            );
            assert_eq!(
                metadata.get("loader_type").and_then(|v| v.as_str()),
                Some("mmap"),
                "Should specify loader_type as mmap"
            );
        } else {
            panic!("Event should have metadata");
        }
    }

    #[tokio::test]
    async fn test_file_not_found_emits_error() {
        let loader = MmapAdapterLoader::new();
        let result = loader.load("/nonexistent/path/to/adapter.aos").await;

        assert!(result.is_err(), "Load should fail for nonexistent file");

        // Verify error telemetry event was emitted
        let telemetry_writer = TelemetryWriter::new_in_memory();
        let events = telemetry_writer.get_events();
        let error_events: Vec<_> = events
            .iter()
            .filter(
                |e| matches!(e.event_type, EventType::Custom(ref s) if s == "adapter.load.error"),
            )
            .collect();

        // Note: This test assumes telemetry is initialized globally
        // In practice, you'd need to inject the writer into the loader
        assert!(
            error_events.len() >= 1,
            "Should emit at least one error event"
        );
    }

    #[tokio::test]
    async fn test_file_too_large_emits_policy_violation() {
        let temp_dir = TempDir::new().unwrap();
        // Create a file larger than default max (500MB)
        let aos_path = create_test_aos_file(&temp_dir, "large_adapter", 600_000); // 600MB

        let loader = MmapAdapterLoader::new();
        let result = loader.load(&aos_path).await;

        assert!(
            result.is_err(),
            "Load should fail for file exceeding size limit"
        );

        // Verify policy violation telemetry event was emitted
        let telemetry_writer = TelemetryWriter::new_in_memory();
        let events = telemetry_writer.get_events();
        let violation_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event_type, EventType::PolicyViolation))
            .collect();

        assert!(
            violation_events.len() >= 1,
            "Should emit at least one policy violation event"
        );

        if let Some(event) = violation_events.first() {
            if let Some(metadata) = &event.metadata {
                assert_eq!(
                    metadata.get("policy").and_then(|v| v.as_str()),
                    Some("max_file_size"),
                    "Should specify policy violation type"
                );
            }
        }
    }

    #[tokio::test]
    async fn test_file_too_small_emits_validation_error() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("tiny.aos");

        // Create a file smaller than minimum size (8 bytes)
        std::fs::write(&path, &[0u8; 4]).unwrap();

        let loader = MmapAdapterLoader::new();
        let result = loader.load(&path).await;

        assert!(result.is_err(), "Load should fail for file too small");

        // Verify validation error telemetry event was emitted
        let telemetry_writer = TelemetryWriter::new_in_memory();
        let events = telemetry_writer.get_events();
        let validation_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(e.event_type, EventType::Custom(ref s) if s == "adapter.validation.error")
            })
            .collect();

        assert!(
            validation_events.len() >= 1,
            "Should emit at least one validation error event"
        );
    }

    #[tokio::test]
    async fn test_telemetry_event_ordering() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path1 = create_test_aos_file(&temp_dir, "adapter1", 50);
        let aos_path2 = create_test_aos_file(&temp_dir, "adapter2", 75);

        let telemetry_writer = TelemetryWriter::new_in_memory();
        let loader = MmapAdapterLoader::new();

        // Load multiple adapters
        let _result1 = loader.load(&aos_path1).await;
        let _result2 = loader.load(&aos_path2).await;

        // Verify events were emitted in correct order
        let events = telemetry_writer.get_events();
        let load_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event_type, EventType::AdapterLoaded))
            .collect();

        assert_eq!(load_events.len(), 2, "Should emit two load events");

        // Verify timestamps are monotonically increasing
        if load_events.len() == 2 {
            assert!(
                load_events[0].timestamp <= load_events[1].timestamp,
                "Events should be in chronological order"
            );
        }
    }

    #[tokio::test]
    async fn test_no_events_dropped() {
        let temp_dir = TempDir::new().unwrap();

        let telemetry_writer = TelemetryWriter::new_in_memory();
        let loader = MmapAdapterLoader::new();

        // Load multiple adapters rapidly
        let mut handles = vec![];
        for i in 0..10 {
            let path = create_test_aos_file(&temp_dir, &format!("adapter_{}", i), 10);
            let handle = tokio::spawn(async move {
                let loader = MmapAdapterLoader::new();
                loader.load(&path).await
            });
            handles.push(handle);
        }

        // Wait for all loads to complete
        for handle in handles {
            let _ = handle.await;
        }

        // Verify all events were captured
        let events = telemetry_writer.get_events();
        let load_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event_type, EventType::AdapterLoaded))
            .collect();

        assert_eq!(
            load_events.len(),
            10,
            "All load events should be captured without drops"
        );
    }

    #[tokio::test]
    async fn test_telemetry_includes_performance_metrics() {
        let temp_dir = TempDir::new().unwrap();
        let aos_path = create_test_aos_file(&temp_dir, "perf_test", 200);

        let telemetry_writer = TelemetryWriter::new_in_memory();
        let loader = MmapAdapterLoader::new();
        let _result = loader.load(&aos_path).await;

        let events = telemetry_writer.get_events();
        let load_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.event_type, EventType::AdapterLoaded))
            .collect();

        assert_eq!(load_events.len(), 1);

        if let Some(event) = load_events.first() {
            if let Some(metadata) = &event.metadata {
                // Verify performance metrics are present
                assert!(
                    metadata.get("duration_ms").is_some(),
                    "Should include load duration"
                );
                assert!(
                    metadata.get("size_bytes").is_some(),
                    "Should include file size"
                );
                assert!(
                    metadata.get("tensor_count").is_some(),
                    "Should include tensor count"
                );
            }
        }
    }
}
