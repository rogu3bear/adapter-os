//! Integration tests for .aos file format as first-class filetype
//!
//! Tests the complete lifecycle: store, index, resolve, load, hot-swap, and federate.

use adapteros_core::{B3Hash, Result};
use adapteros_federation::{AosSyncCoordinator, SyncStrategy};
use adapteros_lora_lifecycle::{AosDirectLoader, AosMmapHandle};
use adapteros_registry::{AosDependencyResolver, AosIndex, AosStore};
use adapteros_single_file_adapter::{
    LineageInfo, SingleFileAdapter, SingleFileAdapterPackager, TrainingConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_adapter(id: &str, parent_hash: Option<String>) -> SingleFileAdapter {
    SingleFileAdapter::create(
        id.to_string(),
        vec![1, 2, 3, 4, 5],
        vec![],
        TrainingConfig::default(),
        LineageInfo {
            adapter_id: id.to_string(),
            version: "1.0.0".to_string(),
            parent_version: None,
            parent_hash,
            mutations: vec![],
            quality_delta: 0.0,
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    )
    .unwrap()
}

#[tokio::test]
async fn test_aos_complete_lifecycle() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let store_dir = temp_dir.path().join("store");

    // 1. Create AOS store
    let aos_store = Arc::new(AosStore::new(store_dir.clone()).await?);

    // 2. Create and store base adapter
    let base_adapter = create_test_adapter("base_adapter", None).await;
    let base_path = temp_dir.path().join("base.aos");
    SingleFileAdapterPackager::save(&base_adapter, &base_path).await?;
    let base_hash = aos_store.store(&base_path).await?;

    // 3. Create and store child adapter (delta)
    let child_adapter = create_test_adapter("child_adapter", Some(base_hash.to_hex())).await;
    let child_path = temp_dir.path().join("child.aos");
    SingleFileAdapterPackager::save(&child_adapter, &child_path).await?;
    let child_hash = aos_store.store(&child_path).await?;

    // 4. Build index
    let aos_index = AosIndex::new();
    aos_index.rebuild(&aos_store).await?;

    // 5. Resolve by ID
    let resolved_hash = aos_index.resolve("child_adapter").unwrap();
    assert_eq!(resolved_hash, child_hash);

    // 6. Dependency resolution
    let dep_resolver = AosDependencyResolver::new(aos_store.clone());
    let chain = dep_resolver.resolve_chain(&child_hash).await?;
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0], base_hash);
    assert_eq!(chain[1], child_hash);

    // 7. Direct loading with memory-mapping
    let direct_loader = AosDirectLoader::new(aos_store.clone());
    let handle = direct_loader.load(&child_hash).await?;
    assert_eq!(handle.manifest().adapter_id, "child_adapter");
    assert!(handle.is_mapped());

    // 8. Hot-swap to new version
    let child_v2 = create_test_adapter("child_adapter", Some(base_hash.to_hex())).await;
    let child_v2_path = temp_dir.path().join("child_v2.aos");
    SingleFileAdapterPackager::save(&child_v2, &child_v2_path).await?;
    let child_v2_hash = aos_store.store(&child_v2_path).await?;

    let swap_result = direct_loader
        .hot_swap("child_adapter", &child_v2_hash)
        .await?;
    assert_eq!(swap_result.old_hash, Some(child_hash));
    assert_eq!(swap_result.new_hash, child_v2_hash);

    // 9. Verify store statistics
    let stats = aos_store.stats();
    assert_eq!(stats.total_adapters, 3); // base + child + child_v2
    assert_eq!(stats.unique_adapter_ids, 2);

    // 10. Export/import for federation
    let export_dir = temp_dir.path().join("export");
    let sync_coordinator = AosSyncCoordinator::new(aos_store.clone(), SyncStrategy::All);
    let exported = sync_coordinator
        .export_to_directory(&export_dir, None)
        .await?;
    assert_eq!(exported, 3);

    // 11. Import to new store
    let store2_dir = temp_dir.path().join("store2");
    let aos_store2 = Arc::new(AosStore::new(store2_dir).await?);
    let sync_coordinator2 = AosSyncCoordinator::new(aos_store2.clone(), SyncStrategy::All);
    let imported = sync_coordinator2.import_from_directory(&export_dir).await?;
    assert_eq!(imported, 3);

    // 12. Verify imported store
    assert!(aos_store2.exists(&base_hash));
    assert!(aos_store2.exists(&child_v2_hash));

    println!("✅ Complete .aos lifecycle test passed!");
    Ok(())
}

#[tokio::test]
async fn test_aos_category_filtering() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let store_dir = temp_dir.path().join("store");
    let aos_store = Arc::new(AosStore::new(store_dir).await?);

    // Create adapters with different categories
    let code_adapter = create_test_adapter("code_adapter", None).await;
    let code_path = temp_dir.path().join("code.aos");
    SingleFileAdapterPackager::save(&code_adapter, &code_path).await?;
    aos_store.store(&code_path).await?;

    // Build index
    let aos_index = AosIndex::new();
    aos_index.rebuild(&aos_store).await?;

    // Query by category
    let code_adapters = aos_index.query_by_category("code");
    assert_eq!(code_adapters.len(), 1);

    println!("✅ Category filtering test passed!");
    Ok(())
}

#[tokio::test]
async fn test_aos_dependency_chain() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let store_dir = temp_dir.path().join("store");
    let aos_store = Arc::new(AosStore::new(store_dir).await?);

    // Create 3-level dependency chain: base -> mid -> top
    let base = create_test_adapter("base", None).await;
    let base_path = temp_dir.path().join("base.aos");
    SingleFileAdapterPackager::save(&base, &base_path).await?;
    let base_hash = aos_store.store(&base_path).await?;

    let mid = create_test_adapter("mid", Some(base_hash.to_hex())).await;
    let mid_path = temp_dir.path().join("mid.aos");
    SingleFileAdapterPackager::save(&mid, &mid_path).await?;
    let mid_hash = aos_store.store(&mid_path).await?;

    let top = create_test_adapter("top", Some(mid_hash.to_hex())).await;
    let top_path = temp_dir.path().join("top.aos");
    SingleFileAdapterPackager::save(&top, &top_path).await?;
    let top_hash = aos_store.store(&top_path).await?;

    // Resolve full chain
    let dep_resolver = AosDependencyResolver::new(aos_store.clone());
    let chain = dep_resolver.resolve_chain(&top_hash).await?;

    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0], base_hash);
    assert_eq!(chain[1], mid_hash);
    assert_eq!(chain[2], top_hash);

    // Get dependency tree
    let tree = dep_resolver.get_dependency_tree(&base_hash).await?;
    assert_eq!(tree.depth(), 3);
    assert_eq!(tree.count(), 3);

    println!("✅ Dependency chain test passed!");
    Ok(())
}

#[tokio::test]
async fn test_aos_hot_swap_performance() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let store_dir = temp_dir.path().join("store");
    let aos_store = Arc::new(AosStore::new(store_dir).await?);

    // Create initial version
    let v1 = create_test_adapter("perf_test", None).await;
    let v1_path = temp_dir.path().join("v1.aos");
    SingleFileAdapterPackager::save(&v1, &v1_path).await?;
    let v1_hash = aos_store.store(&v1_path).await?;

    // Load with direct loader
    let direct_loader = AosDirectLoader::new(aos_store.clone());
    direct_loader.load(&v1_hash).await?;

    // Create v2
    let v2 = create_test_adapter("perf_test", None).await;
    let v2_path = temp_dir.path().join("v2.aos");
    SingleFileAdapterPackager::save(&v2, &v2_path).await?;
    let v2_hash = aos_store.store(&v2_path).await?;

    // Hot-swap and measure time
    let start = std::time::Instant::now();
    let result = direct_loader.hot_swap("perf_test", &v2_hash).await?;
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 50,
        "Hot-swap took too long: {:?}",
        elapsed
    );
    assert_eq!(result.new_hash, v2_hash);

    println!("✅ Hot-swap completed in {:?}", elapsed);
    Ok(())
}

#[tokio::test]
async fn test_aos_index_query_performance() -> Result<()> {
    let temp_dir = TempDir::new().unwrap();
    let store_dir = temp_dir.path().join("store");
    let aos_store = Arc::new(AosStore::new(store_dir).await?);

    // Create 100 adapters
    for i in 0..100 {
        let adapter = create_test_adapter(&format!("adapter_{}", i), None).await;
        let path = temp_dir.path().join(format!("adapter_{}.aos", i));
        SingleFileAdapterPackager::save(&adapter, &path).await?;
        aos_store.store(&path).await?;
    }

    // Build index
    let aos_index = AosIndex::new();
    let start = std::time::Instant::now();
    aos_index.rebuild(&aos_store).await?;
    let build_time = start.elapsed();

    // Query 1000 times
    let start = std::time::Instant::now();
    for i in 0..1000 {
        let id = format!("adapter_{}", i % 100);
        let _ = aos_index.resolve(&id);
    }
    let query_time = start.elapsed();

    let avg_query_time = query_time.as_micros() / 1000;
    println!("✅ Index build time: {:?}", build_time);
    println!("✅ Average query time: {}μs", avg_query_time);

    // Should be sub-millisecond
    assert!(avg_query_time < 100, "Query too slow: {}μs", avg_query_time);

    Ok(())
}
