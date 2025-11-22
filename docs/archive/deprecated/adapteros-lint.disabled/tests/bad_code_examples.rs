//! Unit tests for bad code examples that should trigger determinism guards
//!
//! These tests demonstrate code patterns that violate determinism guarantees
//! and should be caught by the AdapterOS lint rules.

use adapteros_lint::{runtime_guards, strict_mode};

/// Test that demonstrates nondeterministic spawn_blocking usage
#[cfg(test)]
mod spawn_blocking_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_spawn_blocking_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_spawn_blocking();
    }

    #[test]
    fn test_spawn_blocking_without_guards() {
        // This should not panic when guards are disabled
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: false,
            strict_mode: false,
            max_violations: 1,
            log_violations: false,
        });

        runtime_guards::guard_spawn_blocking();
        // Should not panic
    }
}

/// Test that demonstrates wall-clock time usage
#[cfg(test)]
mod wall_clock_time_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_system_time_now_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_wall_clock_time("SystemTime::now()");
    }

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_instant_now_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_wall_clock_time("Instant::now()");
    }

    #[test]
    fn test_guarded_system_time_now() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        let _now = runtime_guards::guarded_system_time_now();
    }

    #[test]
    fn test_guarded_instant_now() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        let _instant = runtime_guards::guarded_instant_now();
    }
}

/// Test that demonstrates random number generation
#[cfg(test)]
mod random_generation_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_random_generation_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_random_generation("rand::random()");
    }

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_thread_rng_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_random_generation("rand::thread_rng()");
    }

    #[test]
    fn test_guarded_random() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        let _value: u32 = runtime_guards::guarded_random();
    }

    #[test]
    fn test_guarded_thread_rng() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        let _rng = runtime_guards::guarded_thread_rng();
    }
}

/// Test that demonstrates file I/O operations
#[cfg(test)]
mod file_io_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_file_io_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_file_io("std::fs::read_to_string");
    }

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_file_write_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_file_io("std::fs::write");
    }
}

/// Test that demonstrates system calls
#[cfg(test)]
mod syscall_tests {
    use super::*;

    #[test]
    #[should_panic(expected = "NONDETERMINISM VIOLATION")]
    fn test_syscall_violation() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: true,
            max_violations: 1,
            log_violations: true,
        });

        // This should trigger a violation
        runtime_guards::guard_syscall("std::process::Command");
    }
}

/// Test that demonstrates strict mode functionality
#[cfg(test)]
mod strict_mode_tests {
    use super::*;

    #[test]
    fn test_strict_mode_disabled_by_default() {
        assert!(!strict_mode::is_strict_mode());
    }

    #[test]
    fn test_enable_disable_strict_mode() {
        assert!(!strict_mode::is_strict_mode());
        
        strict_mode::enable_strict_mode();
        assert!(strict_mode::is_strict_mode());
        
        strict_mode::disable_strict_mode();
        assert!(!strict_mode::is_strict_mode());
    }

    #[test]
    fn test_violation_count_tracking() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: false, // Don't panic, just count
            max_violations: 10,
            log_violations: false,
        });

        let initial_count = runtime_guards::violation_count();
        
        // Report some violations
        runtime_guards::guard_spawn_blocking();
        runtime_guards::guard_wall_clock_time("test");
        runtime_guards::guard_random_generation("test");
        
        let final_count = runtime_guards::violation_count();
        assert_eq!(final_count, initial_count + 3);
        
        // Reset count
        runtime_guards::reset_violation_count();
        assert_eq!(runtime_guards::violation_count(), 0);
    }
}

/// Integration test that demonstrates multiple violation types
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_multiple_violation_types() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: false, // Don't panic, just count
            max_violations: 10,
            log_violations: false,
        });

        let initial_count = runtime_guards::violation_count();
        
        // Test different violation types
        runtime_guards::guard_spawn_blocking();
        runtime_guards::guard_wall_clock_time("SystemTime::now()");
        runtime_guards::guard_random_generation("rand::random()");
        runtime_guards::guard_file_io("std::fs::read_to_string");
        runtime_guards::guard_syscall("std::process::Command");
        
        let final_count = runtime_guards::violation_count();
        assert_eq!(final_count, initial_count + 5);
    }

    #[test]
    #[should_panic(expected = "Too many nondeterminism violations")]
    fn test_max_violations_exceeded() {
        runtime_guards::init_guards(runtime_guards::GuardConfig {
            enabled: true,
            strict_mode: false,
            max_violations: 3,
            log_violations: false,
        });

        // Report more violations than the maximum
        for _ in 0..5 {
            runtime_guards::guard_spawn_blocking();
        }
    }
}
