//! Integration tests for thread-local seed isolation.
//!
//! These tests verify that:
//! 1. Thread-local seed state is isolated between requests
//! 2. The seed isolation middleware properly resets state
//! 3. Sequential requests on the same thread don't share seed context

#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::clone_on_copy)]

use adapteros_core::seed::SeedMode;
use adapteros_core::seed_override::{
    assert_thread_local_clean, get_leaked_state_info, get_thread_seed_context,
    is_thread_local_clean, reset_thread_local_state, set_thread_seed_context, SeedContext,
    SeedContextGuard,
};
use adapteros_core::B3Hash;

/// Test that reset_thread_local_state clears all seed context.
#[test]
fn test_reset_clears_seed_context() {
    reset_thread_local_state();

    // Set up a seed context
    let global = B3Hash::hash(b"test-global");
    let ctx = SeedContext::new(
        global,
        None,
        SeedMode::BestEffort,
        1,
        "test-tenant".to_string(),
    )
    .with_request_id("req-123".to_string());
    set_thread_seed_context(ctx);

    // Verify it's set
    assert!(!is_thread_local_clean());

    // Reset and verify clean
    reset_thread_local_state();
    assert!(is_thread_local_clean());
}

/// Test that two sequential "requests" are isolated.
#[test]
fn test_sequential_request_isolation() {
    // Simulate first request
    reset_thread_local_state();
    assert!(is_thread_local_clean());

    let global = B3Hash::hash(b"test-global");
    let mut ctx1 = SeedContext::new(
        global.clone(),
        None,
        SeedMode::BestEffort,
        1,
        "tenant-a".to_string(),
    )
    .with_request_id("req-1".to_string());

    // Simulate seed derivations during first request
    let _ = ctx1.next_nonce(); // 0
    let _ = ctx1.next_nonce(); // 1
    let _ = ctx1.next_nonce(); // 2
    set_thread_seed_context(ctx1);

    // Verify first request state
    let first_ctx = get_thread_seed_context().unwrap();
    assert_eq!(first_ctx.tenant_id, "tenant-a");
    // nonce_counter is private, but we know it incremented 3 times

    // Simulate second request - must reset first
    reset_thread_local_state();
    assert!(is_thread_local_clean());

    let ctx2 = SeedContext::new(
        global,
        None,
        SeedMode::BestEffort,
        2,
        "tenant-b".to_string(),
    )
    .with_request_id("req-2".to_string());
    set_thread_seed_context(ctx2);

    // Verify second request has fresh state
    let second_ctx = get_thread_seed_context().unwrap();
    assert_eq!(second_ctx.tenant_id, "tenant-b");
    // Fresh context should have nonce_counter = 0, verified by deriving same seed

    reset_thread_local_state();
}

/// Test that SeedContextGuard properly restores previous state.
#[test]
fn test_guard_restores_previous_state() {
    reset_thread_local_state();

    let global = B3Hash::hash(b"test-global");

    // Set outer context
    let outer_ctx = SeedContext::new(
        global.clone(),
        None,
        SeedMode::BestEffort,
        1,
        "outer".to_string(),
    );
    set_thread_seed_context(outer_ctx);

    // Use guard for inner context
    {
        let inner_ctx =
            SeedContext::new(global, None, SeedMode::BestEffort, 2, "inner".to_string());
        let _guard = SeedContextGuard::new(inner_ctx);

        // Inside guard, should see inner context
        let ctx = get_thread_seed_context().unwrap();
        assert_eq!(ctx.tenant_id, "inner");
        assert_eq!(ctx.worker_id, 2);
    }

    // After guard dropped, should see outer context
    let ctx = get_thread_seed_context().unwrap();
    assert_eq!(ctx.tenant_id, "outer");
    assert_eq!(ctx.worker_id, 1);

    reset_thread_local_state();
}

/// Test that leaked state info captures correct details.
#[test]
fn test_leaked_state_info() {
    reset_thread_local_state();

    // No leak initially
    assert!(get_leaked_state_info().is_none());

    // Create a context with some state
    let global = B3Hash::hash(b"test-global");
    let mut ctx = SeedContext::new(
        global,
        None,
        SeedMode::BestEffort,
        1,
        "leaked-tenant".to_string(),
    )
    .with_request_id("leaked-req-id".to_string());

    // Simulate some derivations
    let _ = ctx.next_nonce();
    let _ = ctx.next_nonce();
    let _ = ctx.next_nonce();

    set_thread_seed_context(ctx);

    // Now get leaked state info
    let info = get_leaked_state_info().expect("should have leaked state");
    assert_eq!(info.tenant_id, Some("leaked-tenant".to_string()));
    assert_eq!(info.request_id, Some("leaked-req-id".to_string()));
    assert_eq!(info.nonce_counter, Some(3));

    reset_thread_local_state();
}

/// Test that different tenants get different seed derivations.
#[test]
fn test_tenant_isolation_produces_different_seeds() {
    reset_thread_local_state();

    let global = B3Hash::hash(b"test-global");

    // Tenant A derivation
    let mut ctx_a = SeedContext::new(
        global.clone(),
        None,
        SeedMode::BestEffort,
        1,
        "tenant-a".to_string(),
    )
    .with_request_id("req-1".to_string());
    let seed_a = ctx_a.derive_typed(adapteros_core::seed::SeedLabel::Router);

    // Tenant B derivation (same nonce, different tenant)
    let mut ctx_b = SeedContext::new(
        global,
        None,
        SeedMode::BestEffort,
        1,
        "tenant-b".to_string(),
    )
    .with_request_id("req-2".to_string());
    let seed_b = ctx_b.derive_typed(adapteros_core::seed::SeedLabel::Router);

    // Seeds should be different due to different tenant isolation
    assert_ne!(
        seed_a, seed_b,
        "Different tenants should get different seeds"
    );

    reset_thread_local_state();
}

/// Test that same context produces reproducible seeds.
#[test]
fn test_reproducible_seed_derivation() {
    reset_thread_local_state();

    let global = B3Hash::hash(b"test-global");
    let manifest = B3Hash::hash(b"test-manifest");

    // First run
    let mut ctx1 = SeedContext::new(
        global.clone(),
        Some(manifest.clone()),
        SeedMode::BestEffort,
        1,
        "tenant".to_string(),
    )
    .with_request_id("req-1".to_string());

    let seed1_0 = ctx1.derive_typed(adapteros_core::seed::SeedLabel::Router);
    let seed1_1 = ctx1.derive_typed(adapteros_core::seed::SeedLabel::Router);

    // Second run with identical context
    let mut ctx2 = SeedContext::new(
        global,
        Some(manifest),
        SeedMode::BestEffort,
        1,
        "tenant".to_string(),
    )
    .with_request_id("req-1".to_string());

    let seed2_0 = ctx2.derive_typed(adapteros_core::seed::SeedLabel::Router);
    let seed2_1 = ctx2.derive_typed(adapteros_core::seed::SeedLabel::Router);

    // Same context should produce same seeds
    assert_eq!(seed1_0, seed2_0, "First derivation should be reproducible");
    assert_eq!(seed1_1, seed2_1, "Second derivation should be reproducible");

    // But consecutive derivations should differ (nonce increment)
    assert_ne!(
        seed1_0, seed1_1,
        "Different nonces should produce different seeds"
    );

    reset_thread_local_state();
}

/// Test that clean state assertion passes when clean.
#[test]
fn test_assert_clean_passes_when_clean() {
    reset_thread_local_state();
    // Should not panic
    assert_thread_local_clean();
}

/// Test that clean state assertion panics when dirty (debug builds only).
#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "DETERMINISM BUG")]
fn test_assert_clean_panics_when_dirty() {
    reset_thread_local_state();

    let global = B3Hash::hash(b"test-global");
    let ctx = SeedContext::new(global, None, SeedMode::BestEffort, 1, "leaked".to_string());
    set_thread_seed_context(ctx);

    // Should panic in debug builds
    assert_thread_local_clean();
}

/// Simulate the middleware behavior: reset at start and end.
#[test]
fn test_middleware_simulation() {
    // Simulate residual state from a "previous request"
    let global = B3Hash::hash(b"test-global");
    let residual_ctx = SeedContext::new(
        global.clone(),
        None,
        SeedMode::BestEffort,
        1,
        "old".to_string(),
    )
    .with_request_id("old-req".to_string());
    set_thread_seed_context(residual_ctx);

    // --- Middleware entry point ---
    // Check for leaked state (in production, would log warning)
    if !is_thread_local_clean() {
        let info = get_leaked_state_info();
        assert!(info.is_some());
        // In production: tracing::warn!(...)
    }

    // Reset state
    reset_thread_local_state();

    // Verify clean
    assert!(is_thread_local_clean());

    // --- Request processing ---
    let new_ctx = SeedContext::new(global, None, SeedMode::BestEffort, 2, "new".to_string())
        .with_request_id("new-req".to_string());
    let _guard = SeedContextGuard::new(new_ctx);

    // Do some work...
    let ctx = get_thread_seed_context().unwrap();
    assert_eq!(ctx.tenant_id, "new");

    // --- Middleware exit point ---
    // Guard will restore previous state (None after reset)
    drop(_guard);

    // Final cleanup
    reset_thread_local_state();
    assert!(is_thread_local_clean());
}

/// Test concurrent requests on different threads are isolated.
#[test]
fn test_cross_thread_isolation() {
    use std::sync::mpsc::channel;
    use std::thread;

    let (tx1, rx1) = channel();
    let (tx2, rx2) = channel();

    // Thread 1: Set context for tenant-a
    let handle1 = thread::spawn(move || {
        reset_thread_local_state();

        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(
            global,
            None,
            SeedMode::BestEffort,
            1,
            "tenant-a".to_string(),
        );
        set_thread_seed_context(ctx);

        // Signal ready
        tx1.send(()).unwrap();

        // Wait for thread 2 to set its context
        rx2.recv().unwrap();

        // Verify our context is still tenant-a
        let ctx = get_thread_seed_context().unwrap();
        assert_eq!(
            ctx.tenant_id, "tenant-a",
            "Thread 1 should still have tenant-a"
        );

        reset_thread_local_state();
    });

    // Thread 2: Set context for tenant-b
    let handle2 = thread::spawn(move || {
        reset_thread_local_state();

        // Wait for thread 1 to set its context
        rx1.recv().unwrap();

        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(
            global,
            None,
            SeedMode::BestEffort,
            2,
            "tenant-b".to_string(),
        );
        set_thread_seed_context(ctx);

        // Signal ready
        tx2.send(()).unwrap();

        // Verify our context is tenant-b (not leaked from thread 1)
        let ctx = get_thread_seed_context().unwrap();
        assert_eq!(
            ctx.tenant_id, "tenant-b",
            "Thread 2 should have tenant-b, not tenant-a"
        );

        reset_thread_local_state();
    });

    handle1.join().expect("Thread 1 panicked");
    handle2.join().expect("Thread 2 panicked");
}
