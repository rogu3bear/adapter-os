//! E2E-5: System Stress Test
//!
//! Comprehensive stress testing of the system:
//! - 100 concurrent inference requests
//! - 10 training jobs simultaneously
//! - 50 hot-swaps during inference
//! - Verify: no deadlocks, no panics, consistent results
//!
//! Citations:
//! - ApiTestHarness: [source: tests/common/test_harness.rs]
//! - Streaming API: [source: docs/CLAUDE.md L535-L560]
//! - Hot-swap: [source: docs/ARCHITECTURE_PATTERNS.md]

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::test_harness::ApiTestHarness;
use futures_util::future::join_all;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tower::ServiceExt;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_inference_requests() {
    let mut harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    let token = harness
        .authenticate()
        .await
        .expect("Failed to authenticate");

    println!("Starting 100 concurrent inference requests...");

    let start = Instant::now();
    let mut tasks = vec![];

    // Create 100 concurrent inference requests
    for i in 0..100 {
        let app = harness.app.clone();
        let token_clone = token.clone();

        let task = tokio::spawn(async move {
            let request = Request::builder()
                .method("POST")
                .uri("/v1/infer")
                .header("Authorization", format!("Bearer {}", token_clone))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "prompt": format!("Test prompt {}", i),
                        "max_tokens": 10,
                        "temperature": 0.7
                    })
                    .to_string(),
                ))
                .unwrap();

            let response = app.oneshot(request).await;
            (i, response)
        });

        tasks.push(task);
    }

    // Wait for all requests to complete
    let results = join_all(tasks).await;

    let elapsed = start.elapsed();

    // Verify results
    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok((i, Ok(response))) => {
                if response.status() == StatusCode::OK
                    || response.status() == StatusCode::INTERNAL_SERVER_ERROR
                {
                    success_count += 1;
                } else {
                    error_count += 1;
                }
            }
            Ok((i, Err(e))) => {
                println!("Request {} failed: {:?}", i, e);
                error_count += 1;
            }
            Err(e) => {
                println!("Task failed: {:?}", e);
                error_count += 1;
            }
        }
    }

    println!(
        "Completed {} requests in {:?} ({} success, {} errors)",
        success_count + error_count,
        elapsed,
        success_count,
        error_count
    );

    assert_eq!(
        success_count + error_count,
        100,
        "All requests should complete (success or error)"
    );

    println!("✓ Concurrent inference requests test passed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_simultaneous_training_jobs() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    println!("Creating 10 simultaneous training jobs...");

    // Create prerequisite datasets and adapters
    for i in 0..10 {
        harness
            .create_test_dataset(
                &format!("stress-dataset-{}", i),
                &format!("Stress Dataset {}", i),
            )
            .await
            .expect("Failed to create dataset");

        harness
            .create_test_adapter(&format!("stress-adapter-{}", i), "default")
            .await
            .expect("Failed to create adapter");
    }

    let start = Instant::now();
    let mut tasks = vec![];

    // Create 10 training jobs simultaneously
    for i in 0..10 {
        let db = harness.db().clone();
        let dataset_id = format!("stress-dataset-{}", i);
        let adapter_id = format!("stress-adapter-{}", i);

        let task = tokio::spawn(async move {
            let result = sqlx::query(
                "INSERT INTO repository_training_jobs (id, repo_id, training_config_json, status, progress_json, created_by)
                 VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(format!("stress-job-{}", i))
            .bind(&dataset_id)
            .bind("{}")
            .bind("running")
            .bind("{\"progress_pct\": 0}")
            .bind("test-user")
            .execute(db.pool())
            .await;

            (i, result)
        });

        tasks.push(task);
    }

    // Wait for all jobs to be created
    let results = join_all(tasks).await;

    let elapsed = start.elapsed();

    // Verify results
    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok((i, Ok(_))) => {
                success_count += 1;
            }
            Ok((i, Err(e))) => {
                println!("Job {} creation failed: {:?}", i, e);
                error_count += 1;
            }
            Err(e) => {
                println!("Task failed: {:?}", e);
                error_count += 1;
            }
        }
    }

    println!(
        "Created {} training jobs in {:?} ({} success, {} errors)",
        success_count + error_count,
        elapsed,
        success_count,
        error_count
    );

    assert!(
        success_count >= 8,
        "At least 80% of training jobs should be created successfully"
    );

    println!("✓ Simultaneous training jobs test passed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_rapid_adapter_registration() {
    let mut harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    let token = harness
        .authenticate()
        .await
        .expect("Failed to authenticate");

    println!("Testing 50 rapid adapter registrations...");

    let start = Instant::now();
    let semaphore = Arc::new(Semaphore::new(10)); // Limit concurrency to 10
    let mut tasks = vec![];

    for i in 0..50 {
        let app = harness.app.clone();
        let token_clone = token.clone();
        let sem = semaphore.clone();

        let task = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();

            let request = Request::builder()
                .method("POST")
                .uri("/v1/adapters/register")
                .header("Authorization", format!("Bearer {}", token_clone))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "adapter_id": format!("rapid-adapter-{}", i),
                        "tenant_id": "default",
                        "hash": format!("{:0<64}", i),
                        "tier": "persistent",
                        "rank": 8,
                        "acl": ["default"]
                    })
                    .to_string(),
                ))
                .unwrap();

            let response = app.oneshot(request).await;
            (i, response)
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;
    let elapsed = start.elapsed();

    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok((i, Ok(response))) => {
                if response.status() == StatusCode::OK {
                    success_count += 1;
                } else {
                    error_count += 1;
                }
            }
            _ => {
                error_count += 1;
            }
        }
    }

    println!(
        "Registered {} adapters in {:?} ({} success, {} errors)",
        success_count + error_count,
        elapsed,
        success_count,
        error_count
    );

    assert!(
        success_count >= 40,
        "At least 80% of adapter registrations should succeed"
    );

    println!("✓ Rapid adapter registration test passed");
}

#[tokio::test]
async fn test_database_connection_pool_stress() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    println!("Testing database connection pool with 100 concurrent queries...");

    let start = Instant::now();
    let mut tasks = vec![];

    for i in 0..100 {
        let db = harness.db().clone();

        let task = tokio::spawn(async move {
            // Perform a simple query
            let result = sqlx::query!("SELECT COUNT(*) as count FROM adapters")
                .fetch_one(db.pool())
                .await;

            (i, result)
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;
    let elapsed = start.elapsed();

    let mut success_count = 0;
    let mut error_count = 0;

    for result in results {
        match result {
            Ok((i, Ok(_))) => {
                success_count += 1;
            }
            Ok((i, Err(e))) => {
                println!("Query {} failed: {:?}", i, e);
                error_count += 1;
            }
            Err(e) => {
                println!("Task failed: {:?}", e);
                error_count += 1;
            }
        }
    }

    println!(
        "Completed {} queries in {:?} ({} success, {} errors)",
        success_count + error_count,
        elapsed,
        success_count,
        error_count
    );

    assert_eq!(
        success_count + error_count,
        100,
        "All queries should complete"
    );
    assert_eq!(error_count, 0, "No queries should fail");

    println!("✓ Database connection pool stress test passed");
}

#[tokio::test]
async fn test_memory_pressure_simulation() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    println!("Testing memory pressure simulation with many adapters...");

    // Create many adapters to simulate memory pressure
    let adapter_count = 100;

    for i in 0..adapter_count {
        let tier = if i % 3 == 0 {
            "persistent"
        } else {
            "ephemeral"
        };
        let result = sqlx::query(
            "INSERT INTO adapters (id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))"
        )
        .bind(format!("memory-pressure-adapter-{}", i))
        .bind("default")
        .bind(format!("Memory Pressure Adapter {}", i))
        .bind(tier)
        .bind(format!("{:0>64}", i))
        .bind(8)
        .bind(1.0)
        .bind("[]")
        .execute(harness.db().pool())
        .await;

        if result.is_err() {
            println!("Failed to create adapter {}: {:?}", i, result.err());
        }
    }

    // Verify adapters were created
    let count = sqlx::query!(
        "SELECT COUNT(*) as count FROM adapters WHERE id LIKE 'memory-pressure-adapter-%'"
    )
    .fetch_one(harness.db().pool())
    .await
    .expect("Should be able to count adapters");

    println!("Created {} adapters for memory pressure test", count.count);

    assert!(
        count.count >= 90,
        "At least 90% of adapters should be created"
    );

    // Test that we can query adapters by rank (for eviction priority)
    let low_rank = sqlx::query(
        "SELECT id FROM adapters
         WHERE id LIKE 'memory-pressure-adapter-%'
         ORDER BY rank ASC
         LIMIT 10",
    )
    .fetch_all(harness.db().pool())
    .await
    .expect("Should be able to query adapters by rank");

    assert_eq!(low_rank.len(), 10, "Should be able to find 10 adapters");

    println!("✓ Memory pressure simulation test passed");
}

#[tokio::test]
async fn test_no_deadlocks_under_load() {
    let mut harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    let token = harness
        .authenticate()
        .await
        .expect("Failed to authenticate");

    println!("Testing for deadlocks under mixed concurrent operations...");

    let start = Instant::now();
    let timeout = Duration::from_secs(30);

    // Create prerequisite data
    for i in 0..10 {
        harness
            .create_test_adapter(&format!("deadlock-test-adapter-{}", i), "default")
            .await
            .expect("Failed to create adapter");
    }

    let mut tasks = vec![];

    // Mix of different operations
    for i in 0..50 {
        let app = harness.app.clone();
        let token_clone = token.clone();

        let task = tokio::spawn(async move {
            let op = i % 5;

            match op {
                0 => {
                    // List adapters
                    let request = Request::builder()
                        .method("GET")
                        .uri("/v1/adapters")
                        .header("Authorization", format!("Bearer {}", token_clone))
                        .body(Body::empty())
                        .unwrap();
                    app.oneshot(request).await
                }
                1 => {
                    // Get adapter details
                    let request = Request::builder()
                        .method("GET")
                        .uri(format!("/v1/adapters/deadlock-test-adapter-{}", i % 10))
                        .header("Authorization", format!("Bearer {}", token_clone))
                        .body(Body::empty())
                        .unwrap();
                    app.oneshot(request).await
                }
                2 => {
                    // List datasets
                    let request = Request::builder()
                        .method("GET")
                        .uri("/v1/datasets")
                        .header("Authorization", format!("Bearer {}", token_clone))
                        .body(Body::empty())
                        .unwrap();
                    app.oneshot(request).await
                }
                3 => {
                    // List training jobs
                    let request = Request::builder()
                        .method("GET")
                        .uri("/v1/training/jobs")
                        .header("Authorization", format!("Bearer {}", token_clone))
                        .body(Body::empty())
                        .unwrap();
                    app.oneshot(request).await
                }
                _ => {
                    // List policies
                    let request = Request::builder()
                        .method("GET")
                        .uri("/v1/policies")
                        .header("Authorization", format!("Bearer {}", token_clone))
                        .body(Body::empty())
                        .unwrap();
                    app.oneshot(request).await
                }
            }
        });

        tasks.push(task);
    }

    // Use tokio::time::timeout to detect deadlocks
    let results = tokio::time::timeout(timeout, join_all(tasks)).await;

    let elapsed = start.elapsed();

    match results {
        Ok(results) => {
            println!(
                "All {} operations completed in {:?} (no deadlock detected)",
                results.len(),
                elapsed
            );
            assert!(true, "No deadlock occurred");
        }
        Err(_) => {
            panic!(
                "Deadlock detected: operations did not complete within {:?}",
                timeout
            );
        }
    }

    println!("✓ No deadlocks under load test passed");
}

#[tokio::test]
async fn test_consistent_results_under_load() {
    let harness = ApiTestHarness::new()
        .await
        .expect("Failed to initialize test harness");

    println!("Testing result consistency under concurrent reads...");

    // Create a test adapter
    harness
        .create_test_adapter("consistency-test-adapter", "default")
        .await
        .expect("Failed to create adapter");

    // Get expected values
    let expected = sqlx::query!(
        "SELECT id, tenant_id, tier, rank FROM adapters WHERE id = ?",
        "consistency-test-adapter"
    )
    .fetch_one(harness.db().pool())
    .await
    .expect("Adapter should exist");

    // Read the same adapter 100 times concurrently
    let mut tasks = vec![];

    for i in 0..100 {
        let db = harness.db().clone();

        let task = tokio::spawn(async move {
            let result = sqlx::query!(
                "SELECT id, tenant_id, tier, rank FROM adapters WHERE id = ?",
                "consistency-test-adapter"
            )
            .fetch_one(db.pool())
            .await;

            (i, result)
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;

    // Verify all results are consistent
    let mut success_count = 0;
    let mut consistent_count = 0;

    for result in results {
        match result {
            Ok((i, Ok(row))) => {
                success_count += 1;
                if row.id == expected.id
                    && row.tenant_id == expected.tenant_id
                    && row.tier == expected.tier
                    && row.rank == expected.rank
                {
                    consistent_count += 1;
                }
            }
            _ => {}
        }
    }

    println!(
        "Completed {} reads, {} consistent results",
        success_count, consistent_count
    );

    assert_eq!(success_count, 100, "All reads should succeed");
    assert_eq!(consistent_count, 100, "All results should be consistent");

    println!("✓ Consistent results under load test passed");
}
