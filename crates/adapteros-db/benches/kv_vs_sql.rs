//! Benchmark comparing KV vs SQL adapter operations
//!
//! This benchmark suite measures the performance difference between:
//! - SQL-based adapter operations (legacy)
//! - KV-based adapter operations (new storage backend)
//!
//! Tests cover the most common operations:
//! 1. get_adapter - Single adapter retrieval by ID
//! 2. list_adapters - Listing all adapters for a tenant
//! 3. register_adapter - Creating a new adapter (write operation)
//! 4. update_adapter_state - Updating adapter state (write operation)
//!
//! Usage:
//! ```bash
//! cargo bench --package adapteros-db --bench kv_vs_sql
//! ```
//!
//! Citation: AGENTS.md - Storage migration path validation
//! Copyright JKCA | 2025 James KC Auchterlonie

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::{Db, ProtectedDb, StorageMode};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Test database configuration for benchmarks
struct BenchDb {
    db: ProtectedDb,
    tenant_id: String,
    _temp_sql_dir: Option<TempDir>,
    _temp_kv_dir: Option<TempDir>,
}

impl BenchDb {
    /// Create a new test database with migrations and seed data
    async fn new(mode: StorageMode) -> Self {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp for benches");
        let temp_sql_dir = TempDir::new_in(&tmp_root).expect("Failed to create temp SQL dir");
        let temp_kv_dir = TempDir::new_in(&tmp_root).expect("Failed to create temp KV dir");

        let db_path = temp_sql_dir.path().join("bench.db");
        let kv_path = temp_kv_dir.path().join("bench.redb");

        let mut db = Db::connect(db_path.to_str().unwrap())
            .await
            .expect("Failed to create test database");

        // Apply migrations
        db.migrate().await.expect("Failed to apply migrations");

        // Initialize KV backend if needed
        if mode != StorageMode::SqlOnly {
            db.init_kv_backend(&kv_path)
                .expect("Failed to init KV backend");
            db.set_storage_mode(mode)
                .expect("Failed to set storage mode for benchmark");
        }

        // Create default tenant
        sqlx::query(
            "INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')",
        )
        .execute(db.pool())
        .await
        .expect("Failed to create test tenant");

        let db = ProtectedDb::new(db);

        Self {
            db,
            tenant_id: "default-tenant".to_string(),
            _temp_sql_dir: Some(temp_sql_dir),
            _temp_kv_dir: Some(temp_kv_dir),
        }
    }

    /// Create test adapters for benchmarking
    async fn create_test_adapters(&self, count: usize) -> Vec<String> {
        let mut adapter_ids = Vec::with_capacity(count);

        for i in 0..count {
            let adapter_id = format!("bench-adapter-{}", i);
            let params = AdapterRegistrationBuilder::new()
                .adapter_id(&adapter_id)
                .name(format!("Benchmark Adapter {}", i))
                .hash_b3(format!("b3:bench_hash_{}", i))
                .rank(16)
                .tier("warm")
                .category("code")
                .scope("global")
                .build()
                .expect("Failed to build adapter params");

            self.db
                .register_adapter(params)
                .await
                .expect("Failed to register test adapter");

            adapter_ids.push(adapter_id);
        }

        adapter_ids
    }
}

/// Benchmark get_adapter operation
fn bench_get_adapter(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Setup: Create databases with test data
    let sql_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::SqlOnly).await;
        bench_db.create_test_adapters(100).await;
        bench_db
    });

    let kv_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::KvOnly).await;
        bench_db.create_test_adapters(100).await;
        bench_db
    });

    let dual_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::DualWrite).await;
        bench_db.create_test_adapters(100).await;
        bench_db
    });

    let kv_primary_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::KvPrimary).await;
        bench_db.create_test_adapters(100).await;
        bench_db
    });

    let mut group = c.benchmark_group("get_adapter");

    // SQL-only mode
    group.bench_function(BenchmarkId::new("sql_only", "get"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = sql_db.db.get_adapter(black_box("bench-adapter-50")).await;
                black_box(result)
            })
        });
    });

    // KV-only mode
    group.bench_function(BenchmarkId::new("kv_only", "get"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = kv_db.db.get_adapter(black_box("bench-adapter-50")).await;
                black_box(result)
            })
        });
    });

    // DualWrite mode (reads from SQL)
    group.bench_function(BenchmarkId::new("dual_write", "get"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = dual_db.db.get_adapter(black_box("bench-adapter-50")).await;
                black_box(result)
            })
        });
    });

    // KvPrimary mode (reads from KV)
    group.bench_function(BenchmarkId::new("kv_primary", "get"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = kv_primary_db
                    .db
                    .get_adapter(black_box("bench-adapter-50"))
                    .await;
                black_box(result)
            })
        });
    });

    group.finish();
}

/// Benchmark list_adapters operation
fn bench_list_adapters(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Create databases with varying numbers of adapters
    let sizes = [10, 50, 100];

    for size in sizes.iter() {
        let sql_db = rt.block_on(async {
            let bench_db = BenchDb::new(StorageMode::SqlOnly).await;
            bench_db.create_test_adapters(*size).await;
            bench_db
        });

        let kv_db = rt.block_on(async {
            let bench_db = BenchDb::new(StorageMode::KvOnly).await;
            bench_db.create_test_adapters(*size).await;
            bench_db
        });

        let dual_db = rt.block_on(async {
            let bench_db = BenchDb::new(StorageMode::DualWrite).await;
            bench_db.create_test_adapters(*size).await;
            bench_db
        });

        let kv_primary_db = rt.block_on(async {
            let bench_db = BenchDb::new(StorageMode::KvPrimary).await;
            bench_db.create_test_adapters(*size).await;
            bench_db
        });

        let mut group = c.benchmark_group(format!("list_adapters_{}", size));

        // SQL-only mode
        group.bench_function(BenchmarkId::new("sql_only", "list"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let result = sql_db
                        .db
                        .list_adapters_for_tenant(black_box(&sql_db.tenant_id))
                        .await;
                    black_box(result)
                })
            });
        });

        // KV-only mode
        group.bench_function(BenchmarkId::new("kv_only", "list"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let result = kv_db
                        .db
                        .list_adapters_for_tenant(black_box(&kv_db.tenant_id))
                        .await;
                    black_box(result)
                })
            });
        });

        // DualWrite mode
        group.bench_function(BenchmarkId::new("dual_write", "list"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let result = dual_db
                        .db
                        .list_adapters_for_tenant(black_box(&dual_db.tenant_id))
                        .await;
                    black_box(result)
                })
            });
        });

        // KvPrimary mode
        group.bench_function(BenchmarkId::new("kv_primary", "list"), |b| {
            b.iter(|| {
                rt.block_on(async {
                    let result = kv_primary_db
                        .db
                        .list_adapters_for_tenant(black_box(&kv_primary_db.tenant_id))
                        .await;
                    black_box(result)
                })
            });
        });

        group.finish();
    }
}

/// Benchmark register_adapter (write operation)
fn bench_register_adapter(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("register_adapter");

    // SQL-only mode
    group.bench_function(BenchmarkId::new("sql_only", "write"), |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let count = counter;
            rt.block_on(async move {
                let db = BenchDb::new(StorageMode::SqlOnly).await;
                let params = AdapterRegistrationBuilder::new()
                    .adapter_id(format!("write-bench-sql-{}", count))
                    .name("Write Benchmark Adapter")
                    .hash_b3(format!("b3:write_bench_sql_{}", count))
                    .rank(16)
                    .tier("warm")
                    .category("code")
                    .scope("global")
                    .build()
                    .unwrap();

                let result = db.db.register_adapter(black_box(params)).await;
                black_box(result)
            })
        });
    });

    // KV-only mode
    group.bench_function(BenchmarkId::new("kv_only", "write"), |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let count = counter;
            rt.block_on(async move {
                let db = BenchDb::new(StorageMode::KvOnly).await;
                let params = AdapterRegistrationBuilder::new()
                    .adapter_id(format!("write-bench-kv-{}", count))
                    .name("Write Benchmark Adapter")
                    .hash_b3(format!("b3:write_bench_kv_{}", count))
                    .rank(16)
                    .tier("warm")
                    .category("code")
                    .scope("global")
                    .build()
                    .unwrap();

                let result = db.db.register_adapter(black_box(params)).await;
                black_box(result)
            })
        });
    });

    // DualWrite mode (writes to both)
    group.bench_function(BenchmarkId::new("dual_write", "write"), |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let count = counter;
            rt.block_on(async move {
                let db = BenchDb::new(StorageMode::DualWrite).await;
                let params = AdapterRegistrationBuilder::new()
                    .adapter_id(format!("write-bench-dual-{}", count))
                    .name("Write Benchmark Adapter")
                    .hash_b3(format!("b3:write_bench_dual_{}", count))
                    .rank(16)
                    .tier("warm")
                    .category("code")
                    .scope("global")
                    .build()
                    .unwrap();

                let result = db.db.register_adapter(black_box(params)).await;
                black_box(result)
            })
        });
    });

    // KvPrimary mode (writes to both)
    group.bench_function(BenchmarkId::new("kv_primary", "write"), |b| {
        let mut counter = 0;
        b.iter(|| {
            counter += 1;
            let count = counter;
            rt.block_on(async move {
                let db = BenchDb::new(StorageMode::KvPrimary).await;
                let params = AdapterRegistrationBuilder::new()
                    .adapter_id(format!("write-bench-kvp-{}", count))
                    .name("Write Benchmark Adapter")
                    .hash_b3(format!("b3:write_bench_kvp_{}", count))
                    .rank(16)
                    .tier("warm")
                    .category("code")
                    .scope("global")
                    .build()
                    .unwrap();

                let result = db.db.register_adapter(black_box(params)).await;
                black_box(result)
            })
        });
    });

    group.finish();
}

/// Benchmark update_adapter_state (write operation)
fn bench_update_adapter_state(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Setup: Create databases with test data
    let sql_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::SqlOnly).await;
        bench_db.create_test_adapters(10).await;
        bench_db
    });

    let kv_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::KvOnly).await;
        bench_db.create_test_adapters(10).await;
        bench_db
    });

    let dual_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::DualWrite).await;
        bench_db.create_test_adapters(10).await;
        bench_db
    });

    let kv_primary_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::KvPrimary).await;
        bench_db.create_test_adapters(10).await;
        bench_db
    });

    let mut group = c.benchmark_group("update_adapter_state");

    // SQL-only mode
    group.bench_function(BenchmarkId::new("sql_only", "update"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = sql_db
                    .db
                    .write(sql_db.db.lifecycle_token())
                    .update_adapter_state_tx(
                        black_box("bench-adapter-5"),
                        black_box("warm"),
                        black_box("benchmark test"),
                    )
                    .await;
                black_box(result)
            })
        });
    });

    // KV-only mode
    group.bench_function(BenchmarkId::new("kv_only", "update"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = kv_db
                    .db
                    .write(kv_db.db.lifecycle_token())
                    .update_adapter_state_tx(
                        black_box("bench-adapter-5"),
                        black_box("warm"),
                        black_box("benchmark test"),
                    )
                    .await;
                black_box(result)
            })
        });
    });

    // DualWrite mode (updates both)
    group.bench_function(BenchmarkId::new("dual_write", "update"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = dual_db
                    .db
                    .write(dual_db.db.lifecycle_token())
                    .update_adapter_state_tx(
                        black_box("bench-adapter-5"),
                        black_box("warm"),
                        black_box("benchmark test"),
                    )
                    .await;
                black_box(result)
            })
        });
    });

    // KvPrimary mode (updates both)
    group.bench_function(BenchmarkId::new("kv_primary", "update"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = kv_primary_db
                    .db
                    .write(kv_primary_db.db.lifecycle_token())
                    .update_adapter_state_tx(
                        black_box("bench-adapter-5"),
                        black_box("warm"),
                        black_box("benchmark test"),
                    )
                    .await;
                black_box(result)
            })
        });
    });

    group.finish();
}

/// Benchmark lineage queries (complex traversal operation)
fn bench_adapter_lineage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    // Setup: Create databases with lineage hierarchy
    // parent -> child1 -> grandchild1
    //        -> child2 -> grandchild2
    let sql_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::SqlOnly).await;

        // Create parent
        let parent_params = AdapterRegistrationBuilder::new()
            .adapter_id("parent-adapter")
            .name("Parent Adapter")
            .hash_b3("b3:parent")
            .rank(16)
            .tier("warm")
            .category("code")
            .scope("global")
            .build()
            .unwrap();
        bench_db.db.register_adapter(parent_params).await.unwrap();

        // Create children
        for i in 1..=2 {
            let child_params = AdapterRegistrationBuilder::new()
                .adapter_id(format!("child-adapter-{}", i))
                .name(format!("Child Adapter {}", i))
                .hash_b3(format!("b3:child_{}", i))
                .rank(16)
                .tier("warm")
                .category("code")
                .scope("global")
                .parent_id(Some("parent-adapter".to_string()))
                .build()
                .unwrap();
            bench_db.db.register_adapter(child_params).await.unwrap();

            // Create grandchildren
            let grandchild_params = AdapterRegistrationBuilder::new()
                .adapter_id(format!("grandchild-adapter-{}", i))
                .name(format!("Grandchild Adapter {}", i))
                .hash_b3(format!("b3:grandchild_{}", i))
                .rank(16)
                .tier("warm")
                .category("code")
                .scope("global")
                .parent_id(Some(format!("child-adapter-{}", i)))
                .build()
                .unwrap();
            bench_db
                .db
                .register_adapter(grandchild_params)
                .await
                .unwrap();
        }

        bench_db
    });

    let kv_db = rt.block_on(async {
        let bench_db = BenchDb::new(StorageMode::KvOnly).await;

        // Create same hierarchy
        let parent_params = AdapterRegistrationBuilder::new()
            .adapter_id("parent-adapter")
            .name("Parent Adapter")
            .hash_b3("b3:parent")
            .rank(16)
            .tier("warm")
            .category("code")
            .scope("global")
            .build()
            .unwrap();
        bench_db.db.register_adapter(parent_params).await.unwrap();

        for i in 1..=2 {
            let child_params = AdapterRegistrationBuilder::new()
                .adapter_id(format!("child-adapter-{}", i))
                .name(format!("Child Adapter {}", i))
                .hash_b3(format!("b3:child_{}", i))
                .rank(16)
                .tier("warm")
                .category("code")
                .scope("global")
                .parent_id(Some("parent-adapter".to_string()))
                .build()
                .unwrap();
            bench_db.db.register_adapter(child_params).await.unwrap();

            let grandchild_params = AdapterRegistrationBuilder::new()
                .adapter_id(format!("grandchild-adapter-{}", i))
                .name(format!("Grandchild Adapter {}", i))
                .hash_b3(format!("b3:grandchild_{}", i))
                .rank(16)
                .tier("warm")
                .category("code")
                .scope("global")
                .parent_id(Some(format!("child-adapter-{}", i)))
                .build()
                .unwrap();
            bench_db
                .db
                .register_adapter(grandchild_params)
                .await
                .unwrap();
        }

        bench_db
    });

    let mut group = c.benchmark_group("adapter_lineage");

    // SQL mode (uses CTE for recursive query)
    group.bench_function(BenchmarkId::new("sql_only", "lineage"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = sql_db
                    .db
                    .get_adapter_lineage(black_box("child-adapter-1"))
                    .await;
                black_box(result)
            })
        });
    });

    // KV mode (uses Rust-based graph traversal)
    group.bench_function(BenchmarkId::new("kv_only", "lineage"), |b| {
        b.iter(|| {
            rt.block_on(async {
                let result = kv_db
                    .db
                    .get_adapter_lineage(black_box("child-adapter-1"))
                    .await;
                black_box(result)
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_get_adapter,
    bench_list_adapters,
    bench_register_adapter,
    bench_update_adapter_state,
    bench_adapter_lineage
);
criterion_main!(benches);
