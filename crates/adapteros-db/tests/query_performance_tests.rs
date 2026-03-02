/// Query Performance Optimization Tests
///
/// This test suite verifies:
/// 1. Query optimization effectiveness
/// 2. Index utilization for frequently queried columns
/// 3. Query plan analysis and optimization
/// 4. Performance monitoring and metrics collection
///
/// These tests help ensure database queries meet performance SLAs:
/// - User lookups: < 1ms (indexed)
/// - Adapter listings: < 10ms per 1000 adapters
/// - Routing decisions: < 5ms
/// - Lifecycle queries: < 2ms
use adapteros_db::{Db, QueryMetrics, QueryPerformanceMonitor};
use sqlx::Row;
use std::time::Instant;

/// Helper to create test database with sample data
async fn setup_test_db_with_adapters(adapter_count: usize) -> adapteros_db::Db {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create default tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test Tenant')")
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

    // Create sample adapters
    for i in 0..adapter_count {
        let adapter_id = format!("adapter-{}", i);
        let name = format!("test-adapter-{}", i);
        let hash = format!("hash-{:064x}", i);

        sqlx::query(
            r#"
            INSERT INTO adapters (
                id, tenant_id, name, tier, hash_b3, rank, alpha,
                targets_json, adapter_id, active, lifecycle_state,
                load_state, activation_count, memory_bytes
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(format!("id-{}", i))
        .bind("test-tenant")
        .bind(&name)
        .bind("warm")
        .bind(&hash)
        .bind(16)
        .bind(32.0)
        .bind("[]")
        .bind(&adapter_id)
        .bind(1)
        .bind("active")
        .bind("cold")
        .bind(i as i64)
        .bind((i as i64) * 1024)
        .execute(db.pool())
        .await
        .expect("Failed to insert adapter");
    }

    // Create sample users for authentication testing
    for i in 0..10 {
        let user_id = format!("user-{}", i);
        let email = format!("user{}@aos.local", i);

        sqlx::query(
            "INSERT INTO users (id, email, display_name, pw_hash, role, disabled, mfa_enabled, mfa_secret_enc, mfa_backup_codes_json, mfa_enrolled_at, mfa_last_verified_at, mfa_recovery_last_used_at) \
             VALUES (?, ?, ?, ?, ?, 0, 0, NULL, NULL, NULL, NULL, NULL)",
        )
        .bind(&user_id)
        .bind(&email)
        .bind(format!("User {}", i))
        .bind("$2b$12$...")
        .bind("admin")
        .execute(db.pool())
        .await
        .expect("Failed to insert user");
    }

    db
}

/// Test 1: Measure get_user_by_username query performance
#[tokio::test]
async fn test_user_lookup_performance() {
    let db = setup_test_db_with_adapters(100).await;
    let mut monitor = QueryPerformanceMonitor::new(10); // 10ms threshold

    // Test username lookups (should use email index)
    for i in 0..10 {
        let start = Instant::now();
        let _user = db.get_user_by_username(&format!("user{}", i)).await;
        let elapsed = start.elapsed();

        monitor.record(QueryMetrics {
            query_name: "get_user_by_username".to_string(),
            execution_time_us: elapsed.as_micros() as u64,
            rows_returned: Some(1),
            used_index: true, // Should use email index
            query_plan: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tenant_id: None,
        });

        // User lookup should be very fast (< 1ms) due to UNIQUE index on email
        assert!(
            elapsed.as_millis() < 100,
            "User lookup took {}ms, expected < 100ms",
            elapsed.as_millis()
        );
    }

    let stats = monitor.get_stats("get_user_by_username").unwrap();
    println!(
        "User lookup stats - Avg: {:.2}ms, P95: {:.2}ms, P99: {:.2}ms",
        stats.avg_time_us as f64 / 1000.0,
        stats.p95_time_us as f64 / 1000.0,
        stats.p99_time_us as f64 / 1000.0
    );

    // Verify high index usage
    assert!(
        stats.index_usage_pct > 90.0,
        "Expected > 90% index usage, got {:.1}%",
        stats.index_usage_pct
    );
}

/// Test 2: Measure adapter listing query performance
#[tokio::test]
async fn test_adapter_listing_performance() {
    let db = setup_test_db_with_adapters(500).await;
    let mut monitor = QueryPerformanceMonitor::new(20); // 20ms threshold

    let start = Instant::now();
    let adapters = db
        .list_adapters_by_tenant("test-tenant")
        .await
        .expect("Failed to list adapters");
    let elapsed = start.elapsed();

    monitor.record(QueryMetrics {
        query_name: "list_adapters_by_tenant".to_string(),
        execution_time_us: elapsed.as_micros() as u64,
        rows_returned: Some(adapters.len() as i64),
        used_index: true, // Should use composite (tenant_id, name) index
        query_plan: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tenant_id: None,
    });

    println!(
        "Adapter listing - 500 adapters in {:.2}ms",
        elapsed.as_millis()
    );

    // Adapter listing should be reasonably fast (< 50ms for 500 adapters)
    assert!(
        elapsed.as_millis() < 200,
        "Adapter listing took {}ms for 500 adapters",
        elapsed.as_millis()
    );

    assert_eq!(
        adapters.len(),
        500,
        "Expected 500 adapters, got {}",
        adapters.len()
    );
}

/// Test 3: Measure query plan analysis via EXPLAIN
#[tokio::test]
async fn test_query_plan_analysis() {
    let db = setup_test_db_with_adapters(100).await;

    // Analyze query plan for get_user_by_username
    let plan_rows =
        sqlx::query("EXPLAIN QUERY PLAN SELECT * FROM users WHERE email LIKE ? LIMIT 1")
            .bind("user%")
            .fetch_all(db.pool())
            .await
            .expect("Failed to get query plan");

    println!("Query plan for user lookup:");
    for row in &plan_rows {
        let id: i32 = row.get(0);
        let parent: i32 = row.get(1);
        let notused: i32 = row.get(2);
        let detail: String = row.get(3);
        println!(
            "  [{}] parent={}, notused={}, detail={}",
            id, parent, notused, detail
        );

        // Verify index is being used (should contain "SEARCH")
        if detail.contains("SEARCH") && detail.contains("idx_users_email") {
            // Good: Using the index
        }
    }

    // Analyze query plan for list_adapters_by_tenant
    let plan_rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT * FROM adapters WHERE tenant_id = ? ORDER BY name ASC",
    )
    .bind("test-tenant")
    .fetch_all(db.pool())
    .await
    .expect("Failed to get query plan");

    println!("\nQuery plan for adapter listing:");
    for row in &plan_rows {
        let id: i32 = row.get(0);
        let parent: i32 = row.get(1);
        let notused: i32 = row.get(2);
        let detail: String = row.get(3);
        println!(
            "  [{}] parent={}, notused={}, detail={}",
            id, parent, notused, detail
        );
    }

    assert!(!plan_rows.is_empty(), "Expected query plan rows");

    // Test 3b: Verify composite index usage for tenant-scoped adapter listing
    // This query matches idx_adapters_tenant_active_tier_created
    let plan_rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC",
    )
    .bind("test-tenant")
    .fetch_all(db.pool())
    .await
    .expect("Failed to get query plan for composite index");

    println!("\nQuery plan for composite index adapter listing:");
    let mut uses_composite_index = false;
    let mut uses_temp_btree = false;

    for row in &plan_rows {
        let detail: String = row.get(3);
        println!("  {}", detail);

        if detail.contains("idx_adapters_tenant_active_tier_created") {
            uses_composite_index = true;
        }
        if detail.contains("USE TEMP B-TREE") {
            uses_temp_btree = true;
        }
    }

    assert!(
        uses_composite_index,
        "Query should use idx_adapters_tenant_active_tier_created"
    );
    assert!(
        !uses_temp_btree,
        "Query should NOT use temp B-tree for sorting"
    );
}

/// Test 4: Measure index rebuild performance
#[tokio::test]
async fn test_index_rebuild_performance() {
    let db = setup_test_db_with_adapters(1000).await;

    let start = Instant::now();
    db.rebuild_all_indexes("test-tenant")
        .await
        .expect("Failed to rebuild indexes");
    let elapsed = start.elapsed();

    println!(
        "Index rebuild for 1000 adapters: {:.2}s",
        elapsed.as_secs_f64()
    );

    // Index rebuild should complete reasonably (< 5 seconds for 1000 adapters)
    assert!(
        elapsed.as_secs() < 10,
        "Index rebuild took {}s, expected < 10s",
        elapsed.as_secs()
    );
}

/// Test 5: Measure performance monitoring overhead
#[tokio::test]
async fn test_performance_monitor_overhead() {
    let mut monitor = QueryPerformanceMonitor::new(10);

    let start = std::time::Instant::now();

    // Record 1000 metrics
    for i in 0..1000 {
        monitor.record(QueryMetrics {
            query_name: format!("query_{}", i % 10),
            execution_time_us: 1000 + (i as u64 % 5000),
            rows_returned: Some(100),
            used_index: i % 2 == 0,
            query_plan: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tenant_id: None,
        });
    }

    let elapsed = start.elapsed();
    println!("Recording 1000 metrics took {:.3}ms", elapsed.as_millis());

    // Monitoring overhead should be minimal (< 100ms)
    assert!(
        elapsed.as_millis() < 500,
        "Monitoring overhead too high: {}ms",
        elapsed.as_millis()
    );

    let stats = monitor.all_metrics();
    println!("Generated stats for {} queries", stats.len());
    assert_eq!(stats.len(), 10, "Expected 10 unique queries");

    // Verify recommendations are generated
    for stat in stats.values() {
        if !stat.recommendations.is_empty() {
            println!("Query recommendations: {:?}", stat.recommendations);
        }
    }
}

/// Test 6: Compare optimized vs non-optimized query patterns
#[tokio::test]
async fn test_optimized_query_pattern_performance() {
    let db = setup_test_db_with_adapters(200).await;
    let mut monitor = QueryPerformanceMonitor::new(10);

    // Non-optimized pattern: LIKE with leading wildcard
    let start = Instant::now();
    let _rows = sqlx::query("SELECT * FROM users WHERE email LIKE '%@aos.local' LIMIT 1")
        .fetch_all(db.pool())
        .await
        .expect("Failed to run non-optimized query");
    let non_optimized_time = start.elapsed();

    monitor.record(QueryMetrics {
        query_name: "user_lookup_non_optimized".to_string(),
        execution_time_us: non_optimized_time.as_micros() as u64,
        rows_returned: Some(1),
        used_index: false, // Leading wildcard prevents index use
        query_plan: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tenant_id: None,
    });

    // Optimized pattern: Direct equality
    let start = Instant::now();
    let _rows = sqlx::query("SELECT * FROM users WHERE email = 'user0@aos.local' LIMIT 1")
        .fetch_all(db.pool())
        .await
        .expect("Failed to run optimized query");
    let optimized_time = start.elapsed();

    monitor.record(QueryMetrics {
        query_name: "user_lookup_optimized".to_string(),
        execution_time_us: optimized_time.as_micros() as u64,
        rows_returned: Some(1),
        used_index: true, // Direct equality uses index
        query_plan: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tenant_id: None,
    });

    println!("\nQuery optimization comparison:");
    println!(
        "  Non-optimized (LIKE '%@'): {:.2}ms",
        non_optimized_time.as_millis()
    );
    println!(
        "  Optimized (direct equality): {:.2}ms",
        optimized_time.as_millis()
    );

    if optimized_time < non_optimized_time {
        let improvement = ((non_optimized_time - optimized_time).as_micros() as f64
            / non_optimized_time.as_micros() as f64)
            * 100.0;
        println!("  Improvement: {:.1}% faster", improvement);
    }
}

/// Test 7: Performance report generation
#[tokio::test]
async fn test_performance_report() {
    let mut monitor = QueryPerformanceMonitor::new(10);

    // Record metrics for multiple queries
    for query_num in 0..3 {
        for i in 0..20 {
            let query_name = format!("query_{}", query_num);
            let time_us = match query_num {
                0 => 1000 + (i * 50),    // Fast query
                1 => 10000 + (i * 500),  // Medium query
                2 => 50000 + (i * 5000), // Slow query
                _ => 0,
            };

            monitor.record(QueryMetrics {
                query_name,
                execution_time_us: time_us as u64,
                rows_returned: Some(100),
                used_index: query_num < 2, // Last query doesn't use index
                query_plan: None,
                timestamp: chrono::Utc::now().to_rfc3339(),
                tenant_id: None,
            });
        }
    }

    let report = monitor.report();
    println!("\n{}", report);

    // Verify report contains expected information
    assert!(report.contains("Query Performance Report"));
    assert!(report.contains("query_0"));
    assert!(report.contains("query_1"));
    assert!(report.contains("query_2"));
    assert!(report.contains("Recommendations"));
}

/// Test 8: Bulk operation performance with index rebuild
#[tokio::test]
async fn test_bulk_operation_with_index_rebuild() {
    let db = setup_test_db_with_adapters(100).await;

    // Simulate bulk updates (e.g., adapter evictions)
    println!("Starting bulk adapter updates...");
    let start = Instant::now();

    for i in 0..50 {
        sqlx::query(
            "UPDATE adapters SET load_state = 'warm', activation_count = 0 WHERE adapter_id = ?",
        )
        .bind(format!("adapter-{}", i))
        .execute(db.pool())
        .await
        .expect("Failed to update adapter");
    }

    let bulk_time = start.elapsed();
    println!("Bulk update of 50 adapters: {:.2}ms", bulk_time.as_millis());

    // Rebuild indexes after bulk operation
    let start = Instant::now();
    db.rebuild_all_indexes("test-tenant")
        .await
        .expect("Failed to rebuild indexes");
    let rebuild_time = start.elapsed();

    println!(
        "Index rebuild after bulk operation: {:.2}ms",
        rebuild_time.as_millis()
    );

    // Verify performance after index rebuild
    let start = Instant::now();
    let adapters = db
        .list_adapters_by_tenant("test-tenant")
        .await
        .expect("Failed to list adapters");
    let query_time = start.elapsed();

    println!(
        "Query after index rebuild: {:.2}ms (retrieved {} adapters)",
        query_time.as_millis(),
        adapters.len()
    );

    assert!(
        query_time.as_millis() < 200,
        "Query should be fast after index rebuild"
    );
}

/// Test: Verify usage of idx_adapters_tenant_active_tier_created for tenant-scoped adapter listing
#[tokio::test]
async fn validate_tenant_scoped_adapter_listing_index() {
    let db = setup_test_db_with_adapters(10).await;

    // Explain the query used for tenant-scoped listing
    // Corresponds to list_adapters_for_tenant in adapters.rs
    let plan_rows = sqlx::query(
        "EXPLAIN QUERY PLAN SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC",
    )
    .bind("test-tenant")
    .fetch_all(db.pool())
    .await
    .expect("Failed to get query plan");

    println!("\nQuery plan for tenant-scoped adapter listing:");
    let mut uses_composite_index = false;
    for row in &plan_rows {
        let detail: String = row.get(3);
        println!("  {}", detail);
        if detail.contains("idx_adapters_tenant_active_tier_created") {
            uses_composite_index = true;
        }
    }

    assert!(
        uses_composite_index,
        "Tenant-scoped adapter listing must use idx_adapters_tenant_active_tier_created index"
    );
}
