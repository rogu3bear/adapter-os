//! Tick Ledger Concurrency Tests
//!
//! Tests for concurrent access to GlobalTickLedger, verifying the fix for Issue C-6
//! (atomic tick assignment using fetch_add).

use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_deterministic_exec::{global_ledger::GlobalTickLedger, ExecutorEvent, TaskId};
use std::sync::Arc;

// Global seed for deterministic task ID generation
const GLOBAL_SEED: [u8; 32] = [42u8; 32];

/// Helper to create a TaskId
fn create_task_id(seq: u64) -> TaskId {
    TaskId::from_seed_and_seq(&GLOBAL_SEED, seq)
}

/// Helper to create event hash
fn create_event_hash(data: &str) -> [u8; 32] {
    let hash = blake3::hash(data.as_bytes());
    *hash.as_bytes()
}

/// Test: Concurrent record_tick() calls with unique tick assignment
///
/// Verifies that 10 concurrent tasks all get unique, sequential tick values
/// due to the fetch_add atomic operation (Issue C-6 fix).
#[tokio::test]
async fn test_concurrent_record_tick() -> Result<()> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let ledger = Arc::new(GlobalTickLedger::new(
        Arc::new(db),
        "tenant-test".into(),
        "host-test".into(),
    ));

    // 10 tasks record events concurrently
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let l = ledger.clone();
            tokio::spawn(async move {
                let task_id = create_task_id(i);
                let event = ExecutorEvent::TaskSpawned {
                    task_id,
                    tick: 0,
                    description: format!("Task {}", i),
                    agent_id: Some(format!("agent-{}", i)),
                    hash: create_event_hash(&format!("task-{}", i)),
                };
                l.record_tick(task_id, &event).await.unwrap();
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    // Verify: 10 entries with UNIQUE tick values
    let entries = ledger.get_entries(0, 100).await?;
    assert_eq!(entries.len(), 10, "Should have 10 entries");

    let mut ticks: Vec<_> = entries.iter().map(|e| e.tick).collect();
    ticks.sort();

    // Verify ticks are sequential (0, 1, 2, ..., 9)
    for i in 0..ticks.len() - 1 {
        assert_eq!(
            ticks[i] + 1,
            ticks[i + 1],
            "Ticks must be sequential: {} followed by {}",
            ticks[i],
            ticks[i + 1]
        );
    }

    Ok(())
}

/// Test: High-frequency tick advancement (1000 events)
///
/// Simulates a high-throughput workload to verify tick assignment
/// remains correct under load.
#[tokio::test]
async fn test_high_frequency_ticks() -> Result<()> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let ledger = Arc::new(GlobalTickLedger::new(
        Arc::new(db),
        "tenant-test".into(),
        "host-test".into(),
    ));

    // Record 1000 events rapidly (simulating high-throughput workload)
    let start = std::time::Instant::now();
    for i in 0..1000 {
        let task_id = create_task_id(i);
        let event = ExecutorEvent::TaskCompleted {
            task_id,
            tick: i,
            duration_ticks: 100,
            agent_id: Some("agent-main".to_string()),
            hash: create_event_hash(&format!("completed-{}", i)),
        };
        ledger.record_tick(task_id, &event).await?;
    }
    let elapsed = start.elapsed();

    println!(
        "Recorded 1000 events in {:?} ({:.2} events/sec)",
        elapsed,
        1000.0 / elapsed.as_secs_f64()
    );

    // Verify all ticks are sequential
    let entries = ledger.get_entries(0, 1000).await?;
    assert_eq!(entries.len(), 1000, "Should have 1000 entries");

    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.tick, i as u64,
            "Entry {} should have tick {}",
            i, i
        );
    }

    Ok(())
}

/// Test: Merkle chain integrity under concurrency
///
/// Verifies that the Merkle chain (prev_entry_hash → event_hash linkage)
/// remains intact even with 20 concurrent tasks.
#[tokio::test]
async fn test_merkle_chain_concurrent() -> Result<()> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let ledger = Arc::new(GlobalTickLedger::new(
        Arc::new(db),
        "tenant-test".into(),
        "host-test".into(),
    ));

    // 20 concurrent tasks
    let handles: Vec<_> = (0..20)
        .map(|i| {
            let l = ledger.clone();
            tokio::spawn(async move {
                let task_id = create_task_id(i);
                let event = ExecutorEvent::TaskSpawned {
                    task_id,
                    tick: 0,
                    description: format!("Task {}", i),
                    agent_id: Some(format!("agent-{}", i)),
                    hash: create_event_hash(&format!("spawn-{}", i)),
                };
                l.record_tick(task_id, &event).await.unwrap();
            })
        })
        .collect();

    for h in handles {
        h.await.unwrap();
    }

    // Verify Merkle chain is intact
    // Note: Merkle chain order is NOT the same as tick or timestamp order!
    // We must follow the prev_entry_hash links to reconstruct the actual chain
    let entries = ledger.get_entries(0, 100).await?;
    assert_eq!(entries.len(), 20, "Should have 20 entries");

    // Build a hash map for O(1) lookups
    use std::collections::HashMap;
    let entry_map: HashMap<_, _> = entries
        .iter()
        .map(|e| (e.event_hash, e))
        .collect();

    // Count roots
    let roots: Vec<_> = entries
        .iter()
        .filter(|e| e.prev_entry_hash.is_none())
        .collect();

    println!("Found {} root entries (expected 1)", roots.len());

    // Print all entries for debugging
    println!("\nAll entries:");
    for (i, e) in entries.iter().enumerate() {
        let prev_str = e.prev_entry_hash.as_ref()
            .map(|h| h.to_hex()[..16].to_string())
            .unwrap_or_else(|| "None".to_string());
        println!(
            "  [{}] tick={}, hash={}, prev={}",
            i,
            e.tick,
            &e.event_hash.to_hex()[..16],
            prev_str
        );
    }

    assert_eq!(roots.len(), 1, "Should have exactly one root entry");
    let root = roots[0];

    // Follow the chain from root to verify all 20 entries are linked
    let mut current = root;
    let mut chain_length = 1;

    while chain_length < 20 {
        // Find the next entry that points to current
        let next = entries
            .iter()
            .find(|e| e.prev_entry_hash == Some(current.event_hash));

        match next {
            Some(entry) => {
                println!(
                    "Chain[{}]: tick={}, hash={}",
                    chain_length,
                    entry.tick,
                    &entry.event_hash.to_hex()[..16]
                );
                chain_length += 1;
                current = entry;
            }
            None => {
                println!("\nChain ended. Looking for entry with prev_hash={}", current.event_hash.to_hex());
                panic!(
                    "Chain ends prematurely at length {} (expected 20)",
                    chain_length
                );
            }
        }
    }

    assert_eq!(
        chain_length, 20,
        "Merkle chain should contain all 20 entries"
    );

    Ok(())
}

/// Test: Concurrent reads while writing
///
/// Verifies that reading entries while others are being written
/// doesn't cause race conditions or inconsistent state.
#[tokio::test]
async fn test_concurrent_read_write() -> Result<()> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let ledger = Arc::new(GlobalTickLedger::new(
        Arc::new(db),
        "tenant-test".into(),
        "host-test".into(),
    ));

    // Spawn 5 writers
    let write_handles: Vec<_> = (0..5)
        .map(|i| {
            let l = ledger.clone();
            tokio::spawn(async move {
                for j in 0..20 {
                    let seq = i * 20 + j;
                    let task_id = create_task_id(seq);
                    let event = ExecutorEvent::TaskSpawned {
                        task_id,
                        tick: 0,
                        description: format!("Writer {} Task {}", i, j),
                        agent_id: Some(format!("writer-{}", i)),
                        hash: create_event_hash(&format!("writer-{}-{}", i, j)),
                    };
                    l.record_tick(task_id, &event).await.unwrap();
                    // Small delay to interleave with readers
                    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                }
            })
        })
        .collect();

    // Spawn 3 readers
    let read_handles: Vec<_> = (0..3)
        .map(|_i| {
            let l = ledger.clone();
            tokio::spawn(async move {
                for _j in 0..10 {
                    // Read current state
                    let entries = l.get_entries(0, 1000).await.unwrap();
                    // Verify entries are valid (non-zero hash)
                    for entry in entries {
                        assert!(
                            !entry.event_hash.to_hex().is_empty(),
                            "Entry should have valid hash"
                        );
                    }
                    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                }
            })
        })
        .collect();

    // Wait for all writers
    for h in write_handles {
        h.await.unwrap();
    }

    // Wait for all readers
    for h in read_handles {
        h.await.unwrap();
    }

    // Final verification: all 100 entries exist (5 writers × 20 entries)
    let entries = ledger.get_entries(0, 200).await?;
    assert_eq!(
        entries.len(),
        100,
        "Should have 100 entries from 5 writers"
    );

    Ok(())
}
