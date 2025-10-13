//! Test multi-agent tick synchronization
//!
//! Verifies that AgentBarrier correctly synchronizes agents at tick boundaries.

use adapteros_deterministic_exec::multi_agent::{
    AgentBarrier, CoordinatedAction, next_global_seq, reset_global_seq,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_agent_barrier_basic() {
    let barrier = Arc::new(AgentBarrier::new(vec![
        "agent-1".to_string(),
        "agent-2".to_string(),
    ]));

    let barrier1 = barrier.clone();
    let barrier2 = barrier.clone();

    // Spawn two agents
    let handle1 = tokio::spawn(async move {
        barrier1.wait("agent-1", 100).await
    });

    let handle2 = tokio::spawn(async move {
        barrier2.wait("agent-2", 100).await
    });

    // Both should complete
    handle1.await.unwrap().unwrap();
    handle2.await.unwrap().unwrap();

    // Generation should be incremented
    assert_eq!(barrier.generation(), 1);
}

#[tokio::test]
async fn test_agent_barrier_staggered_arrival() {
    let barrier = Arc::new(AgentBarrier::new(vec![
        "agent-1".to_string(),
        "agent-2".to_string(),
        "agent-3".to_string(),
    ]));

    let barrier1 = barrier.clone();
    let barrier2 = barrier.clone();
    let barrier3 = barrier.clone();

    // Agent 1 arrives first
    let handle1 = tokio::spawn(async move {
        barrier1.wait("agent-1", 100).await
    });

    // Small delay before agent 2
    tokio::time::sleep(Duration::from_millis(10)).await;

    let handle2 = tokio::spawn(async move {
        barrier2.wait("agent-2", 100).await
    });

    // Longer delay before agent 3
    tokio::time::sleep(Duration::from_millis(20)).await;

    let handle3 = tokio::spawn(async move {
        barrier3.wait("agent-3", 100).await
    });

    // All should complete
    handle1.await.unwrap().unwrap();
    handle2.await.unwrap().unwrap();
    handle3.await.unwrap().unwrap();

    assert_eq!(barrier.generation(), 1);
}

#[tokio::test]
async fn test_agent_barrier_multiple_rounds() {
    let barrier = Arc::new(AgentBarrier::new(vec![
        "agent-1".to_string(),
        "agent-2".to_string(),
    ]));

    // First synchronization at tick 100
    {
        let barrier1 = barrier.clone();
        let barrier2 = barrier.clone();

        let h1 = tokio::spawn(async move {
            barrier1.wait("agent-1", 100).await
        });

        let h2 = tokio::spawn(async move {
            barrier2.wait("agent-2", 100).await
        });

        h1.await.unwrap().unwrap();
        h2.await.unwrap().unwrap();
    }

    assert_eq!(barrier.generation(), 1);

    // Second synchronization at tick 200
    {
        let barrier1 = barrier.clone();
        let barrier2 = barrier.clone();

        let h1 = tokio::spawn(async move {
            barrier1.wait("agent-1", 200).await
        });

        let h2 = tokio::spawn(async move {
            barrier2.wait("agent-2", 200).await
        });

        h1.await.unwrap().unwrap();
        h2.await.unwrap().unwrap();
    }

    assert_eq!(barrier.generation(), 2);
}

#[tokio::test]
async fn test_agent_barrier_unregistered_agent() {
    let barrier = AgentBarrier::new(vec!["agent-1".to_string()]);

    // Try to wait with unregistered agent
    let result = barrier.wait("agent-2", 100).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_global_sequence_ordering() {
    reset_global_seq();

    let seq1 = next_global_seq();
    let seq2 = next_global_seq();
    let seq3 = next_global_seq();

    assert_eq!(seq1, 0);
    assert_eq!(seq2, 1);
    assert_eq!(seq3, 2);
}

#[tokio::test]
async fn test_coordinated_action_ordering() {
    reset_global_seq();

    let action1 = CoordinatedAction::new(
        "agent-1".to_string(),
        100,
        vec![1, 2, 3],
    );

    let action2 = CoordinatedAction::new(
        "agent-2".to_string(),
        100,
        vec![4, 5, 6],
    );

    let action3 = CoordinatedAction::new(
        "agent-1".to_string(),
        101,
        vec![7, 8, 9],
    );

    // Sequences should be strictly increasing
    assert_eq!(action1.sequence, 0);
    assert_eq!(action2.sequence, 1);
    assert_eq!(action3.sequence, 2);
}

#[tokio::test]
async fn test_coordinated_action_hash_determinism() {
    reset_global_seq();

    let action1 = CoordinatedAction::new(
        "agent-1".to_string(),
        100,
        vec![1, 2, 3],
    );

    // Hash should be deterministic
    let hash1 = action1.hash();
    let hash2 = action1.hash();

    assert_eq!(hash1, hash2);
}

#[tokio::test]
async fn test_coordinated_action_serialization() {
    reset_global_seq();

    let action = CoordinatedAction::new(
        "agent-1".to_string(),
        100,
        vec![1, 2, 3],
    );

    // Serialize and deserialize
    let json = serde_json::to_string(&action).unwrap();
    let deserialized: CoordinatedAction = serde_json::from_str(&json).unwrap();

    assert_eq!(action.sequence, deserialized.sequence);
    assert_eq!(action.agent_id, deserialized.agent_id);
    assert_eq!(action.tick, deserialized.tick);
    assert_eq!(action.payload, deserialized.payload);
}

#[tokio::test]
async fn test_agent_barrier_mixed_ticks() {
    let barrier = Arc::new(AgentBarrier::new(vec![
        "agent-1".to_string(),
        "agent-2".to_string(),
    ]));

    let barrier1 = barrier.clone();
    let barrier2 = barrier.clone();

    // Agent 1 at tick 100
    let handle1 = tokio::spawn(async move {
        barrier1.wait("agent-1", 100).await
    });

    // Agent 2 at tick 150 (ahead)
    let handle2 = tokio::spawn(async move {
        barrier2.wait("agent-2", 150).await
    });

    // Agent 2 should wait for agent 1 to catch up to at least 150
    // This will timeout in this test since agent 1 only goes to 100
    let result1 = handle1.await.unwrap();
    let result2 = tokio::time::timeout(Duration::from_millis(100), handle2).await;

    // Agent 1 should succeed at tick 100
    assert!(result1.is_ok());

    // Agent 2 should still be waiting (timeout)
    assert!(result2.is_err());
}
