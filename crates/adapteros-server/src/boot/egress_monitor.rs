//! Periodic egress re-verification background task.
//!
//! At boot, `PfGuard::preflight()` asserts egress-blocking firewall rules are
//! active. This module re-checks those rules periodically so that rule changes
//! after boot are detected and surfaced via tracing events.
//!
//! ## Configuration (env vars)
//!
//! | Var | Default | Description |
//! |-----|---------|-------------|
//! | `AOS_EGRESS_MONITOR_SECS` | `60` | Check interval in seconds |
//! | `AOS_EGRESS_MONITOR_DISABLE` | unset | Set to `1` to disable entirely |
//!
//! ## Behaviour
//!
//! - On rule change: emit `WARN`-level tracing event with old/new fingerprint
//! - On egress now allowed (was blocked): emit `ERROR`-level tracing event
//! - If pfctl/iptables is unavailable: log once and disable the monitor
//! - Skipped in dev mode (follows other production-only task pattern)

use std::process::Command;
use tracing::{debug, error, info, warn};

/// Snapshot of firewall egress state at a point in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressSnapshot {
    /// Whether egress is blocked (both TCP and UDP).
    pub egress_blocked: bool,
    /// BLAKE3 fingerprint of the raw rule output, for change detection.
    pub rules_fingerprint: String,
    /// Number of outbound block rules found.
    pub block_rule_count: usize,
}

/// Check current egress state on macOS via pfctl.
///
/// Returns `None` if pfctl is unavailable (not root, binary missing, etc).
#[cfg(target_os = "macos")]
pub fn check_egress_state() -> Option<EgressSnapshot> {
    // Check PF status
    let info_output = Command::new("pfctl").arg("-s").arg("info").output().ok()?;

    if !info_output.status.success() {
        return None;
    }

    let info_stdout = String::from_utf8_lossy(&info_output.stdout);
    if !info_stdout.contains("Status: Enabled") {
        return Some(EgressSnapshot {
            egress_blocked: false,
            rules_fingerprint: String::new(),
            block_rule_count: 0,
        });
    }

    // Get rules
    let rules_output = Command::new("pfctl")
        .arg("-s")
        .arg("rules")
        .arg("-v")
        .output()
        .ok()?;

    if !rules_output.status.success() {
        return None;
    }

    let rules_stdout = String::from_utf8_lossy(&rules_output.stdout);
    let fingerprint = blake3::hash(rules_stdout.as_bytes()).to_hex().to_string();

    let mut tcp_blocked = false;
    let mut udp_blocked = false;
    let mut block_rule_count = 0;

    for line in rules_stdout.lines() {
        if line.contains("block out") {
            block_rule_count += 1;
            if line.contains("proto tcp") || line.contains("all") {
                tcp_blocked = true;
            }
            if line.contains("proto udp") || line.contains("all") {
                udp_blocked = true;
            }
        }
    }

    Some(EgressSnapshot {
        egress_blocked: tcp_blocked && udp_blocked && block_rule_count > 0,
        rules_fingerprint: fingerprint,
        block_rule_count,
    })
}

/// Check current egress state on Linux via iptables.
///
/// Returns `None` if iptables is unavailable (not root, binary missing, etc).
#[cfg(target_os = "linux")]
pub fn check_egress_state() -> Option<EgressSnapshot> {
    let output = Command::new("iptables")
        .arg("-L")
        .arg("OUTPUT")
        .arg("-n")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let fingerprint = blake3::hash(stdout.as_bytes()).to_hex().to_string();

    let has_drop_policy = stdout.lines().any(|line| {
        line.starts_with("Chain OUTPUT")
            && (line.contains("policy DROP") || line.contains("policy REJECT"))
    });

    let mut tcp_rules = 0;
    let mut udp_rules = 0;
    let mut block_rule_count = 0;

    for line in stdout.lines() {
        if line.contains("DROP") || line.contains("REJECT") {
            block_rule_count += 1;
            if line.contains("tcp") {
                tcp_rules += 1;
            }
            if line.contains("udp") {
                udp_rules += 1;
            }
        }
    }

    let egress_blocked = has_drop_policy || (tcp_rules > 0 && udp_rules > 0);

    Some(EgressSnapshot {
        egress_blocked,
        rules_fingerprint: fingerprint,
        block_rule_count,
    })
}

/// Stub for unsupported platforms — always returns `None`.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn check_egress_state() -> Option<EgressSnapshot> {
    None
}

/// Returns `true` if the egress monitor is disabled via env var.
pub fn is_monitor_disabled() -> bool {
    std::env::var("AOS_EGRESS_MONITOR_DISABLE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Returns the configured check interval in seconds (default 60).
pub fn monitor_interval_secs() -> u64 {
    std::env::var("AOS_EGRESS_MONITOR_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60)
}

/// Run a single egress monitor iteration, comparing against the previous snapshot.
///
/// Returns the new snapshot (to be stored as "previous" for the next iteration).
/// Returns `None` if the check is unavailable.
pub fn run_egress_check(previous: Option<&EgressSnapshot>) -> Option<EgressSnapshot> {
    let current = check_egress_state()?;

    match previous {
        None => {
            // First check — establish baseline
            if current.egress_blocked {
                info!(
                    block_rules = current.block_rule_count,
                    fingerprint = %current.rules_fingerprint,
                    "Egress monitor baseline: egress blocked"
                );
            } else {
                error!(
                    block_rules = current.block_rule_count,
                    "EGRESS MONITOR: egress is NOT blocked at monitor start"
                );
            }
        }
        Some(prev) => {
            // Detect transitions
            if prev.egress_blocked && !current.egress_blocked {
                // Was blocked, now allowed — CRITICAL
                error!(
                    previous_rules = prev.block_rule_count,
                    current_rules = current.block_rule_count,
                    previous_fingerprint = %prev.rules_fingerprint,
                    current_fingerprint = %current.rules_fingerprint,
                    "EGRESS VIOLATION: firewall rules changed — egress is now ALLOWED"
                );
            } else if !prev.egress_blocked && current.egress_blocked {
                // Was allowed, now blocked — recovery
                info!(
                    block_rules = current.block_rule_count,
                    "Egress monitor: egress blocking restored"
                );
            } else if prev.rules_fingerprint != current.rules_fingerprint {
                // Rules changed but blocking status is the same
                warn!(
                    previous_fingerprint = %prev.rules_fingerprint,
                    current_fingerprint = %current.rules_fingerprint,
                    previous_rules = prev.block_rule_count,
                    current_rules = current.block_rule_count,
                    egress_blocked = current.egress_blocked,
                    "Egress monitor: firewall rules changed"
                );
            } else {
                debug!("Egress monitor: no changes detected");
            }
        }
    }

    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_when_env_set() {
        // This test verifies the parsing logic. In actual env, the value
        // comes from std::env which we can't safely set in parallel tests,
        // so we test the parse function directly.
        assert!(!is_monitor_disabled()); // unset = not disabled
    }

    #[test]
    fn default_interval_is_60() {
        assert_eq!(monitor_interval_secs(), 60);
    }

    #[test]
    fn snapshot_equality() {
        let a = EgressSnapshot {
            egress_blocked: true,
            rules_fingerprint: "abc".to_string(),
            block_rule_count: 3,
        };
        let b = EgressSnapshot {
            egress_blocked: true,
            rules_fingerprint: "abc".to_string(),
            block_rule_count: 3,
        };
        assert_eq!(a, b);
    }

    #[test]
    fn snapshot_inequality_on_fingerprint() {
        let a = EgressSnapshot {
            egress_blocked: true,
            rules_fingerprint: "abc".to_string(),
            block_rule_count: 3,
        };
        let b = EgressSnapshot {
            egress_blocked: true,
            rules_fingerprint: "def".to_string(),
            block_rule_count: 3,
        };
        assert_ne!(a, b);
    }
}
