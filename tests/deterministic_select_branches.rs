#![cfg(all(test, feature = "extended-tests"))]

//! Test deterministic select branching
//!
//! Verifies that select operations are deterministic and reproducible.

use adapteros_deterministic_exec::select::{select_2, select_3, SelectResult2, SelectResult3};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[tokio::test]
async fn test_deterministic_select_2_ordering() {
    // Both futures ready immediately - should always pick first
    let fut1 = async { 42 };
    let fut2 = async { 100 };

    match select_2(fut1, fut2).await {
        SelectResult2::First(val) => assert_eq!(val, 42),
        SelectResult2::Second(_) => panic!("Should always select first when both ready"),
    }
}

#[tokio::test]
async fn test_deterministic_select_2_second_ready() {
    // Only second future completes
    let fut1 = async {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        42
    };

    let fut2 = async { 100 };

    match select_2(fut1, fut2).await {
        SelectResult2::First(_) => panic!("First future should timeout"),
        SelectResult2::Second(val) => assert_eq!(val, 100),
    }
}

#[tokio::test]
async fn test_deterministic_select_3_ordering() {
    // All ready immediately - should always pick first
    let fut1 = async { 1 };
    let fut2 = async { 2 };
    let fut3 = async { 3 };

    match select_3(fut1, fut2, fut3).await {
        SelectResult3::First(val) => assert_eq!(val, 1),
        _ => panic!("Should always select first when all ready"),
    }
}

#[tokio::test]
async fn test_deterministic_select_3_middle() {
    // Only middle future completes
    let fut1 = async {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        1
    };

    let fut2 = async { 2 };

    let fut3 = async {
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        3
    };

    match select_3(fut1, fut2, fut3).await {
        SelectResult3::Second(val) => assert_eq!(val, 2),
        _ => panic!("Should select middle future"),
    }
}

#[tokio::test]
async fn test_deterministic_select_reproduces() {
    // Run same select twice - should get same result
    let counter = Arc::new(AtomicU32::new(0));

    for _ in 0..2 {
        let counter_clone = counter.clone();
        let fut1 = async move {
            counter_clone.fetch_add(1, Ordering::Relaxed);
            "first"
        };

        let fut2 = async { "second" };

        match select_2(fut1, fut2).await {
            SelectResult2::First(val) => assert_eq!(val, "first"),
            SelectResult2::Second(_) => panic!("Non-deterministic result"),
        }
    }

    // Counter should be incremented exactly twice
    assert_eq!(counter.load(Ordering::Relaxed), 2);
}

#[tokio::test]
async fn test_select_with_delays() {
    // Test with tick-based delays to ensure determinism
    let fut1 = async {
        // Simulate work
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        "slow"
    };

    let fut2 = async { "fast" };

    match select_2(fut1, fut2).await {
        SelectResult2::First(_) => panic!("Fast future should complete first"),
        SelectResult2::Second(val) => assert_eq!(val, "fast"),
    }
}

#[tokio::test]
async fn test_select_3_all_delayed() {
    let fut1 = async {
        for _ in 0..3 {
            tokio::task::yield_now().await;
        }
        1
    };

    let fut2 = async {
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
        2
    };

    let fut3 = async {
        for _ in 0..7 {
            tokio::task::yield_now().await;
        }
        3
    };

    match select_3(fut1, fut2, fut3).await {
        SelectResult3::First(val) => assert_eq!(val, 1),
        _ => panic!("Shortest delay should complete first"),
    }
}
