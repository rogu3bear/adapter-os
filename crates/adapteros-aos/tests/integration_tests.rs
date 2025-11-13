//! Integration tests for adapteros-aos

use adapteros_aos::{AosManager, MmapAdapterLoader};
use std::path::PathBuf;
use tracing::{info, warn, error};

fn test_adapter_path() -> Option<PathBuf> {
    let adapters_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()?
        .parent()?
        .join("adapters");

    if !adapters_dir.exists() {
        return None;
    }

    std::fs::read_dir(adapters_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.path().extension().and_then(|s| s.to_str()) == Some("aos"))
        .map(|entry| entry.path())
}

#[tokio::test]
async fn test_load_adapter() {
    let Some(adapter_path) = test_adapter_path() else {
        eprintln!("No .aos files found, skipping test");
        return;
    };

    let loader = MmapAdapterLoader::new();
    let result = loader.load(&adapter_path).await;

    match result {
        Ok(adapter) => {
            println!("Loaded: {}", adapter.adapter_id());
            println!("Version: {}", adapter.version());
            println!("Size: {} bytes", adapter.size_bytes());
            assert!(!adapter.adapter_id().is_empty());
        }
        Err(e) => {
            eprintln!("Failed to load: {}", e);
        }
    }
}

#[tokio::test]
async fn test_manager_with_cache() {
    let Some(adapter_path) = test_adapter_path() else {
        eprintln!("No .aos files found, skipping test");
        return;
    };

    let manager = AosManager::builder()
        .with_cache(1024 * 1024 * 1024)
        .build()
        .unwrap();

    let result1 = manager.load(&adapter_path).await;

    if let Ok(_adapter1) = result1 {
        let result2 = manager.load(&adapter_path).await;
        assert!(result2.is_ok());

        if let Some(cache) = manager.cache() {
            assert_eq!(cache.len(), 1);
            assert!(cache.metrics().hits() > 0);
        }
    }
}

#[tokio::test]
async fn test_hot_swap() {
    let Some(adapter_path) = test_adapter_path() else {
        eprintln!("No .aos files found, skipping test");
        return;
    };

    let manager = AosManager::builder().with_hot_swap().build().unwrap();

    let result = manager.preload("slot1", &adapter_path).await;

    if result.is_ok() {
        let swap_result = manager.commit_swap(&["slot1".to_string()]);
        assert!(swap_result.is_ok());

        if let Some(hot_swap) = manager.hot_swap_manager() {
            let active_slots = hot_swap.active_slots();
            assert!(active_slots.contains(&"slot1".to_string()));
        }
    }
}
