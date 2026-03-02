//! Focused tests for replay guard health semantics in `/readyz`.

use adapteros_server_api::handlers::health::{
    evaluate_replay_guard_status, replay_guard_allows_ready, ReadinessMode, ReplayGuardState,
};
use chrono::{Duration as ChronoDuration, Utc};
use std::time::Duration;

#[test]
fn replay_guard_fresh_pass_is_ok() {
    let now = Utc::now();
    let last_run = (now - ChronoDuration::seconds(60))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let check = evaluate_replay_guard_status(
        Some(&last_run),
        Some("pass"),
        Some(3),
        Some(0),
        now,
        Duration::from_secs(900),
    );

    assert!(check.ok);
    assert_eq!(check.state, ReplayGuardState::Fresh);
    assert_eq!(check.age_seconds, Some(60));
}

#[test]
fn replay_guard_stale_pass_is_not_ok() {
    let now = Utc::now();
    let last_run = (now - ChronoDuration::seconds(901))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let check = evaluate_replay_guard_status(
        Some(&last_run),
        Some("pass"),
        Some(10),
        Some(0),
        now,
        Duration::from_secs(900),
    );

    assert!(!check.ok);
    assert_eq!(check.state, ReplayGuardState::Stale);
    assert_eq!(check.age_seconds, Some(901));
}

#[test]
fn replay_guard_fail_result_is_not_ok() {
    let now = Utc::now();
    let last_run = (now - ChronoDuration::seconds(10))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    let check = evaluate_replay_guard_status(
        Some(&last_run),
        Some("fail"),
        Some(5),
        Some(2),
        now,
        Duration::from_secs(900),
    );

    assert!(!check.ok);
    assert_eq!(check.state, ReplayGuardState::Failing);
    assert_eq!(check.result.as_deref(), Some("fail"));
}

#[test]
fn replay_guard_missing_row_is_not_ok() {
    let check =
        evaluate_replay_guard_status(None, None, None, None, Utc::now(), Duration::from_secs(900));

    assert!(!check.ok);
    assert_eq!(check.state, ReplayGuardState::Missing);
}

#[test]
fn strict_mode_fail_closed_on_replay_guard() {
    let now = Utc::now();
    let stale_guard = evaluate_replay_guard_status(
        Some("2000-01-01 00:00:00"),
        Some("pass"),
        Some(1),
        Some(0),
        now,
        Duration::from_secs(900),
    );
    let fresh_last_run = (now - ChronoDuration::seconds(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let fresh_guard = evaluate_replay_guard_status(
        Some(&fresh_last_run),
        Some("pass"),
        Some(1),
        Some(0),
        now,
        Duration::from_secs(u64::MAX),
    );

    assert!(!replay_guard_allows_ready(
        &ReadinessMode::Strict,
        Some(&stale_guard)
    ));
    assert!(replay_guard_allows_ready(
        &ReadinessMode::Relaxed {
            relaxed_checks: vec!["worker".to_string()]
        },
        Some(&stale_guard)
    ));
    assert!(replay_guard_allows_ready(
        &ReadinessMode::DevBypass,
        Some(&stale_guard)
    ));
    assert!(replay_guard_allows_ready(
        &ReadinessMode::Strict,
        Some(&fresh_guard)
    ));
}
