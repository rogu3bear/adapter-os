//! Cross-Host Consistency Tests
//!
//! Tests for cross-host divergence detection using GlobalTickLedger's
//! Merkle chain verification and consistency checking.

use adapteros_core::{B3Hash, Result};
use adapteros_db::Db;
use adapteros_deterministic_exec::{global_ledger::GlobalTickLedger, ExecutorEvent, TaskId};
use std::sync::Arc;

// Global seed for deterministic task ID generation
const GLOBAL_SEED: [u8; 32] = [99u8; 32];

/// Helper to create a TaskId
fn create_task_id(seq: u64) -> TaskId {
    TaskId::from_seed_and_seq(&GLOBAL_SEED, seq)
}

/// Helper to create event hash
fn create_event_hash(data: &str) -> [u8; 32] {
    let hash = blake3::hash(data.as_bytes());
    *hash.as_bytes()
}

/// Test: Simulated network partition causing divergence
///
/// Two hosts record different events at the same ticks (simulating a partition),
/// and we verify that cross-host consistency checking detects the divergence.
#[tokio::test]
async fn test_network_partition_divergence() -> Result<()> {
    // Two ledgers representing two hosts
    let db_a = Db::connect("sqlite::memory:").await?;
    db_a.migrate().await?;
    let db_b = Db::connect("sqlite::memory:").await?;
    db_b.migrate().await?;

    let ledger_a = GlobalTickLedger::new(Arc::new(db_a), "tenant-1".into(), "host-a".into());

    let ledger_b = GlobalTickLedger::new(Arc::new(db_b), "tenant-1".into(), "host-b".into());

    // Host A records events at ticks 0-4
    for i in 0..5 {
        let task_id = create_task_id(i);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            tick: i,
            description: format!("Host A Task {}", i),
            agent_id: Some("host-a-agent".to_string()),
            hash: create_event_hash(&format!("host-a-task-{}", i)),
        };
        ledger_a.record_tick(task_id, &event).await?;
    }

    // Host B records DIFFERENT events at ticks 0-4 (partition scenario)
    for i in 0..5 {
        let task_id = create_task_id(i + 100); // Different task ID for host B
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            tick: i,
            description: format!("Host B Task {}", i), // Different description
            agent_id: Some("host-b-agent".to_string()),
            hash: create_event_hash(&format!("host-b-task-{}", i)),
        };
        ledger_b.record_tick(task_id, &event).await?;
    }

    // Retrieve entries from both hosts
    let entries_a = ledger_a.get_entries(0, 4).await?;
    let entries_b = ledger_b.get_entries(0, 4).await?;

    assert_eq!(entries_a.len(), 5, "Host A should have 5 entries");
    assert_eq!(entries_b.len(), 5, "Host B should have 5 entries");

    // Verify divergence: Same ticks, different hashes
    let mut divergence_count = 0;
    for i in 0..5 {
        if entries_a[i].event_hash != entries_b[i].event_hash {
            divergence_count += 1;
        }
    }

    assert_eq!(
        divergence_count, 5,
        "Should detect 5 divergent ticks (all events differ)"
    );

    // Verify Merkle chains are internally consistent but incompatible across hosts
    // Host A chain
    for i in 1..entries_a.len() {
        assert_eq!(
            entries_a[i].prev_entry_hash,
            Some(entries_a[i - 1].event_hash),
            "Host A Merkle chain broken at index {}",
            i
        );
    }

    // Host B chain
    for i in 1..entries_b.len() {
        assert_eq!(
            entries_b[i].prev_entry_hash,
            Some(entries_b[i - 1].event_hash),
            "Host B Merkle chain broken at index {}",
            i
        );
    }

    // But cross-host chains should NOT match (diverged execution)
    for i in 0..5 {
        assert_ne!(
            entries_a[i].event_hash, entries_b[i].event_hash,
            "Tick {} should have different hashes across hosts",
            i
        );
    }

    Ok(())
}

/// Test: Merkle chain break detection with corrupted data
///
/// Manually corrupts one entry's hash and verifies that Merkle chain
/// validation detects the break.
#[tokio::test]
async fn test_merkle_chain_break_detection() -> Result<()> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    let ledger = GlobalTickLedger::new(Arc::new(db.clone()), "tenant-1".into(), "host-a".into());

    // Record 10 events normally
    for i in 0..10 {
        let task_id = create_task_id(i);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            tick: i,
            description: format!("Task {}", i),
            agent_id: Some("host-a-agent".to_string()),
            hash: create_event_hash(&format!("task-{}", i)),
        };
        ledger.record_tick(task_id, &event).await?;
    }

    // Verify chain is initially intact
    let entries = ledger.get_entries(0, 9).await?;
    assert_eq!(entries.len(), 10, "Should have 10 entries");

    let mut chain_valid = true;
    for i in 1..entries.len() {
        if entries[i].prev_entry_hash != Some(entries[i - 1].event_hash) {
            chain_valid = false;
            break;
        }
    }
    assert!(chain_valid, "Chain should be initially valid");

    // Manually corrupt entry at tick 5 (simulate Byzantine failure or data corruption)
    let corrupted_hash = B3Hash::hash(b"CORRUPTED_DATA");
    sqlx::query("UPDATE tick_ledger_entries SET event_hash = ? WHERE tick = ? AND host_id = ?")
        .bind(corrupted_hash.to_hex())
        .bind(5)
        .bind("host-a")
        .execute(db.pool())
        .await?;

    // Retrieve entries again
    let entries_corrupted = ledger.get_entries(0, 9).await?;

    // Verify Merkle chain validation detects break
    let mut chain_valid_after = true;
    let mut break_index = None;
    for i in 1..entries_corrupted.len() {
        if entries_corrupted[i].prev_entry_hash != Some(entries_corrupted[i - 1].event_hash) {
            chain_valid_after = false;
            break_index = Some(i);
            break;
        }
    }

    assert!(
        !chain_valid_after,
        "Chain should be invalid after corruption"
    );
    assert_eq!(
        break_index,
        Some(6),
        "Chain should break at index 6 (tick 6's prev_hash no longer matches corrupted tick 5's hash)"
    );

    Ok(())
}

/// Test: Cross-host consistency with identical execution
///
/// Two hosts execute the SAME tasks and should have identical hashes
/// and Merkle chains (no divergence).
#[tokio::test]
async fn test_identical_execution_consistency() -> Result<()> {
    // Two ledgers representing two hosts
    let db_a = Db::connect("sqlite::memory:").await?;
    db_a.migrate().await?;
    let db_b = Db::connect("sqlite::memory:").await?;
    db_b.migrate().await?;

    let ledger_a = GlobalTickLedger::new(Arc::new(db_a), "tenant-1".into(), "host-a".into());

    let ledger_b = GlobalTickLedger::new(Arc::new(db_b), "tenant-1".into(), "host-b".into());

    // Both hosts record IDENTICAL events
    for i in 0..10 {
        let task_id = create_task_id(i);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            tick: i,
            description: format!("Deterministic Task {}", i), // Identical description
            agent_id: Some("shared-agent".to_string()),
            hash: create_event_hash(&format!("deterministic-task-{}", i)),
        };

        ledger_a.record_tick(task_id, &event).await?;
        ledger_b.record_tick(task_id, &event).await?;
    }

    // Retrieve entries from both hosts
    let entries_a = ledger_a.get_entries(0, 9).await?;
    let entries_b = ledger_b.get_entries(0, 9).await?;

    assert_eq!(entries_a.len(), 10, "Host A should have 10 entries");
    assert_eq!(entries_b.len(), 10, "Host B should have 10 entries");

    // Verify NO divergence: Same ticks, same hashes
    for i in 0..10 {
        assert_eq!(
            entries_a[i].event_hash, entries_b[i].event_hash,
            "Tick {} should have identical hash across hosts for deterministic execution",
            i
        );
    }

    // Verify Merkle chains are identical
    for i in 1..10 {
        assert_eq!(
            entries_a[i].prev_entry_hash, entries_b[i].prev_entry_hash,
            "Tick {} should have identical prev_entry_hash",
            i
        );
    }

    Ok(())
}

/// Test: Partial divergence (some ticks match, some don't)
///
/// Simulates a scenario where hosts diverge at tick 5 but were consistent before.
#[tokio::test]
async fn test_partial_divergence() -> Result<()> {
    let db_a = Db::connect("sqlite::memory:").await?;
    db_a.migrate().await?;
    let db_b = Db::connect("sqlite::memory:").await?;
    db_b.migrate().await?;

    let ledger_a = GlobalTickLedger::new(Arc::new(db_a), "tenant-1".into(), "host-a".into());

    let ledger_b = GlobalTickLedger::new(Arc::new(db_b), "tenant-1".into(), "host-b".into());

    // Ticks 0-4: Identical execution
    for i in 0..5 {
        let task_id = create_task_id(i);
        let event = ExecutorEvent::TaskSpawned {
            task_id,
            tick: i,
            description: format!("Consistent Task {}", i),
            agent_id: Some("shared-agent".to_string()),
            hash: create_event_hash(&format!("consistent-task-{}", i)),
        };

        ledger_a.record_tick(task_id, &event).await?;
        ledger_b.record_tick(task_id, &event).await?;
    }

    // Tick 5+: Divergence (different events)
    for i in 5..10 {
        let task_id_a = create_task_id(i + 100); // Different task IDs for divergence
        let task_id_b = create_task_id(i + 200);

        let event_a = ExecutorEvent::TaskSpawned {
            task_id: task_id_a,
            tick: i,
            description: format!("Host A Diverged Task {}", i),
            agent_id: Some("host-a-agent".to_string()),
            hash: create_event_hash(&format!("host-a-diverged-{}", i)),
        };

        let event_b = ExecutorEvent::TaskSpawned {
            task_id: task_id_b,
            tick: i,
            description: format!("Host B Diverged Task {}", i),
            agent_id: Some("host-b-agent".to_string()),
            hash: create_event_hash(&format!("host-b-diverged-{}", i)),
        };

        ledger_a.record_tick(task_id_a, &event_a).await?;
        ledger_b.record_tick(task_id_b, &event_b).await?;
    }

    let entries_a = ledger_a.get_entries(0, 9).await?;
    let entries_b = ledger_b.get_entries(0, 9).await?;

    // Verify ticks 0-4 match
    for i in 0..5 {
        assert_eq!(
            entries_a[i].event_hash, entries_b[i].event_hash,
            "Tick {} should match (before divergence)",
            i
        );
    }

    // Verify ticks 5-9 diverge
    for i in 5..10 {
        assert_ne!(
            entries_a[i].event_hash, entries_b[i].event_hash,
            "Tick {} should diverge (after divergence point)",
            i
        );
    }

    Ok(())
}
