//! Example demonstrating LoadCoordinator usage for thundering herd protection
//!
//! Run with: cargo run --example load_coordinator_example -p adapteros-server-api

use adapteros_lora_lifecycle::loader::{AdapterHandle, AdapterMetadata};
use adapteros_server_api::LoadCoordinator;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    println!("=== LoadCoordinator Example ===\n");

    let coordinator = Arc::new(LoadCoordinator::new());
    let load_count = Arc::new(AtomicU32::new(0));

    println!("Spawning 10 concurrent requests for 'test-adapter'...\n");

    let mut handles = vec![];

    // Spawn 10 concurrent requests for the same adapter
    for i in 0..10 {
        let coord = coordinator.clone();
        let count = load_count.clone();

        let handle = tokio::spawn(async move {
            let request_id = i;
            println!("Request {}: Starting", request_id);

            let result = coord
                .load_or_wait("test-adapter", || async move {
                    // Simulate expensive load operation
                    count.fetch_add(1, Ordering::SeqCst);
                    println!("  >>> Performing actual load (expensive operation)...");
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                    Ok(AdapterHandle {
                        adapter_id: 42,
                        path: PathBuf::from("/test/adapter.aos"),
                        memory_bytes: 1024 * 1024,
                        metadata: AdapterMetadata {
                            num_parameters: 1000,
                            rank: Some(8),
                            target_modules: vec!["q_proj".to_string(), "v_proj".to_string()],
                            ..Default::default()
                        },
                    })
                })
                .await;

            match result {
                Ok(handle) => {
                    println!(
                        "Request {}: Success (adapter_id={}, memory={}MB)",
                        request_id,
                        handle.adapter_id,
                        handle.memory_bytes / 1024 / 1024
                    );
                }
                Err(e) => {
                    println!("Request {}: Error: {}", request_id, e);
                }
            }
        });

        handles.push(handle);
    }

    // Wait for all requests to complete
    for handle in handles {
        handle.await.unwrap();
    }

    println!(
        "\n=== Results ===\nTotal load operations performed: {}\n",
        load_count.load(Ordering::SeqCst)
    );

    if load_count.load(Ordering::SeqCst) == 1 {
        println!("✓ SUCCESS: Only 1 load performed despite 10 concurrent requests!");
        println!("  The other 9 requests waited for the first to complete.");
    } else {
        println!(
            "✗ UNEXPECTED: {} loads performed (expected 1)",
            load_count.load(Ordering::SeqCst)
        );
    }

    // Show metrics
    let metrics = coordinator.metrics();
    println!("\nCoordinator Metrics:");
    println!("  Pending loads: {}", metrics.pending_loads);
    println!("  Total waiters: {}", metrics.total_waiters);
}
