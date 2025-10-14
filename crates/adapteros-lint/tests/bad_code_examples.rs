// This file is intended to contain "bad code examples" that would trigger
// the determinism guards at runtime. These tests are designed to panic
// when the guards are enabled and strict mode is active.

use adapteros_lint::{runtime_guards, strict_mode};
use rand::Rng;

// Helper to set up strict mode for tests
fn setup_strict_mode_test() {
    runtime_guards::reset_violation_count();
    strict_mode::enable_strict_mode();
    runtime_guards::init_guards(runtime_guards::GuardConfig {
        enabled: true,
        strict_mode: true,
        max_violations: 1, // Should panic on first violation
        log_violations: false,
    });
}

#[test]
#[should_panic(expected = "spawn_blocking")]
fn test_spawn_blocking_guard() {
    setup_strict_mode_test();
    runtime_guards::guard_spawn_blocking();
}

#[test]
#[should_panic(expected = "SystemTime::now()")]
fn test_system_time_now_guard() {
    setup_strict_mode_test();
    let _now = runtime_guards::guarded_system_time_now();
}

#[test]
#[should_panic(expected = "Instant::now()")]
fn test_instant_now_guard() {
    setup_strict_mode_test();
    let _start = runtime_guards::guarded_instant_now();
}

#[test]
#[should_panic(expected = "rand::random()")]
fn test_rand_random_guard() {
    setup_strict_mode_test();
    let _random_val: u32 = runtime_guards::guarded_random();
}

#[test]
#[should_panic(expected = "rand::thread_rng()")]
fn test_thread_rng_guard() {
    setup_strict_mode_test();
    let mut rng = runtime_guards::guarded_thread_rng();
    let _random_val: u32 = rng.gen();
}

#[test]
#[should_panic(expected = "File I/O operation")]
fn test_file_io_guard() {
    setup_strict_mode_test();
    runtime_guards::guard_file_io("read");
}
