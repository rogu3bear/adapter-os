use adapteros_server_api::model_runtime::{ModelRuntime, ModelKey, LoadModelSpec, ProgressEvent};
use std::path::PathBuf;

#[tokio::test]
async fn model_runtime_trait_smoke_test() {
    // Test that the ModelRuntime trait can be implemented and called
    use adapteros_server_api::model_runtime::{ModelRuntime, ModelKey, LoadModelSpec};

    struct TestRuntime;
    #[async_trait::async_trait]
    impl ModelRuntime for TestRuntime {
        async fn load_model_async_with_progress<F>(
            &self,
            req: LoadModelSpec,
            on_progress: F,
        ) -> Result<adapteros_server_api::model_runtime::ModelHandle, adapteros_server_api::model_runtime::ModelLoadError>
        where
            F: Fn(ProgressEvent) + Send + Sync + 'static,
        {
            on_progress(ProgressEvent { pct: 50.0, message: "test progress".to_string() });
            Ok(adapteros_server_api::model_runtime::ModelHandle {
                key: ModelKey {
                    tenant_id: req.tenant_id,
                    model_id: req.model_id,
                },
                memory_usage_mb: 1024,
            })
        }

        fn is_loaded(&self, _key: &ModelKey) -> bool {
            false
        }

        async fn unload(&self, _key: &ModelKey) -> Result<(), adapteros_server_api::model_runtime::ModelLoadError> {
            Ok(())
        }
    }

    let runtime = TestRuntime;
    let mut progress_events = Vec::new();

    let result = runtime.load_model_async_with_progress(
        LoadModelSpec {
            tenant_id: "test".to_string(),
            model_id: "model".to_string(),
            model_path: PathBuf::from("/tmp/test"),
            adapter_path: None,
            quantization: None,
        },
        |ev| progress_events.push(ev),
    ).await;

    assert!(result.is_ok());
    assert_eq!(progress_events.len(), 1);
    assert_eq!(progress_events[0].pct, 50.0);
    assert_eq!(progress_events[0].message, "test progress");
    assert_eq!(result.unwrap().memory_usage_mb, 1024);
}

#[tokio::test]
async fn model_load_error_types() {
    // Test that ModelLoadError variants work correctly
    use adapteros_server_api::model_runtime::ModelLoadError;

    let not_found = ModelLoadError::NotFound("test model".to_string());
    assert_eq!(format!("{}", not_found), "model not found: test model");

    let invalid = ModelLoadError::Invalid("bad config".to_string());
    assert_eq!(format!("{}", invalid), "invalid model: bad config");

    let io = ModelLoadError::Io("disk error".to_string());
    assert_eq!(format!("{}", io), "io error: disk error");

    let backend = ModelLoadError::Backend("mlx error".to_string());
    assert_eq!(format!("{}", backend), "backend error: mlx error");

    let canceled = ModelLoadError::Canceled;
    assert_eq!(format!("{}", canceled), "canceled");
}

#[tokio::test]
async fn progress_event_structure() {
    // Test that ProgressEvent has the expected structure
    use adapteros_server_api::model_runtime::ProgressEvent;

    let event = ProgressEvent {
        pct: 75.5,
        message: "loading components".to_string(),
    };

    assert_eq!(event.pct, 75.5);
    assert_eq!(event.message, "loading components");
}

#[tokio::test]
async fn load_model_spec_structure() {
    // Test that LoadModelSpec has the expected structure
    use adapteros_server_api::model_runtime::LoadModelSpec;

    let spec = LoadModelSpec {
        tenant_id: "tenant1".to_string(),
        model_id: "model1".to_string(),
        model_path: PathBuf::from("/path/to/model"),
        adapter_path: Some(PathBuf::from("/path/to/adapter")),
        quantization: Some("4bit".to_string()),
    };

    assert_eq!(spec.tenant_id, "tenant1");
    assert_eq!(spec.model_id, "model1");
    assert_eq!(spec.model_path, PathBuf::from("/path/to/model"));
    assert_eq!(spec.adapter_path, Some(PathBuf::from("/path/to/adapter")));
    assert_eq!(spec.quantization, Some("4bit".to_string()));
}

#[tokio::test]
async fn model_key_hash_and_eq() {
    // Test that ModelKey implements Hash and Eq correctly
    use adapteros_server_api::model_runtime::ModelKey;
    use std::collections::HashMap;

    let key1 = ModelKey {
        tenant_id: "tenant1".to_string(),
        model_id: "model1".to_string(),
    };

    let key2 = ModelKey {
        tenant_id: "tenant1".to_string(),
        model_id: "model1".to_string(),
    };

    let key3 = ModelKey {
        tenant_id: "tenant2".to_string(),
        model_id: "model1".to_string(),
    };

    assert_eq!(key1, key2);
    assert_ne!(key1, key3);

    let mut map = HashMap::new();
    map.insert(key1.clone(), "value1".to_string());
    assert_eq!(map.get(&key2), Some(&"value1".to_string()));
    assert_eq!(map.get(&key3), None);
}

#[tokio::test]
async fn model_handle_structure() {
    // Test that ModelHandle has the expected structure
    use adapteros_server_api::model_runtime::{ModelHandle, ModelKey};

    let handle = ModelHandle {
        key: ModelKey {
            tenant_id: "tenant1".to_string(),
            model_id: "model1".to_string(),
        },
        memory_usage_mb: 2048,
    };

    assert_eq!(handle.key.tenant_id, "tenant1");
    assert_eq!(handle.key.model_id, "model1");
    assert_eq!(handle.memory_usage_mb, 2048);
}

#[tokio::test]
async fn memory_usage_calculation() {
    // Test memory usage calculation with realistic model parameters
    // Using a 7B parameter model as example (approximate)
    let hidden_size = 4096;
    let num_layers = 32;
    let num_heads = 32;

    let num_parameters = hidden_size * num_layers * num_heads;
    let bytes_per_param = 2; // FP16
    let total_bytes = num_parameters * bytes_per_param * 2; // ×2 for weights + gradients/kv cache
    let expected_mb = total_bytes / (1024 * 1024);

    // Should be at least 512MB minimum
    assert!(expected_mb >= 512);
    // Should be a reasonable size for a 7B model (typically 13-14GB for weights alone)
    assert!(expected_mb > 1000); // More than 1GB
    assert!(expected_mb < 50000); // Less than 50GB (reasonable upper bound)
}
