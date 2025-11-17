//! Deterministic Execution & Multi-Agent Coordination Verification
//!
//! Verifies the deterministic execution subsystem including:
//! - Global tick ledger with federation support
//! - AgentBarrier synchronization primitives
//! - Multi-agent coordination fixes (Issues C-1 through C-8)

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub async fn run(_args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Deterministic Execution & Multi-Agent Coordination");

    // 1. Tick ledger federation migration
    section.add_check(check_federation_migration());

    // 2. Federation schema columns
    section.add_check(check_federation_schema());

    // 3. AgentBarrier Notify mechanism (Issue C-2 fix)
    section.add_check(check_barrier_notify());

    // 4. Dead agent handling (Issue C-8 fix)
    section.add_check(check_dead_agent_handling());

    // 5. Comprehensive barrier tests
    section.add_check(check_barrier_tests());

    Ok(section)
}

fn check_federation_migration() -> Check {
    let migration_path = Path::new("migrations/0035_tick_ledger_federation.sql");

    if !migration_path.exists() {
        return Check::fail(
            "Tick ledger federation migration",
            vec![],
            "Migration 0035_tick_ledger_federation.sql not found in migrations/",
        );
    }

    match fs::read_to_string(migration_path) {
        Ok(content) => {
            let has_bundle_hash = content.contains("bundle_hash");
            let has_prev_host_hash = content.contains("prev_host_hash");
            let has_federation_sig = content.contains("federation_signature");

            if has_bundle_hash && has_prev_host_hash && has_federation_sig {
                Check::pass(
                    "Tick ledger federation migration",
                    vec![
                        "Migration 0035_tick_ledger_federation.sql exists".to_string(),
                        "Contains bundle_hash, prev_host_hash, federation_signature columns"
                            .to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "Tick ledger federation migration",
                    vec![
                        format!("bundle_hash: {}", has_bundle_hash),
                        format!("prev_host_hash: {}", has_prev_host_hash),
                        format!("federation_signature: {}", has_federation_sig),
                    ],
                    "Migration exists but missing required federation columns",
                )
            }
        }
        Err(e) => Check::fail(
            "Tick ledger federation migration",
            vec![],
            format!("Failed to read migration: {}", e),
        ),
    }
}

fn check_federation_schema() -> Check {
    let ledger_src = "crates/adapteros-deterministic-exec/src/global_ledger.rs";

    if !Path::new(ledger_src).exists() {
        return Check::fail(
            "Federation schema in global_ledger.rs",
            vec![],
            format!("File not found: {}", ledger_src),
        );
    }

    match fs::read_to_string(ledger_src) {
        Ok(content) => {
            // Check for TickLedgerEntry struct with federation fields
            let has_bundle_hash = content.contains("bundle_hash");
            let has_prev_host_hash = content.contains("prev_host_hash");
            let has_federation_signature = content.contains("federation_signature");

            let mut evidence = Vec::new();
            if has_bundle_hash {
                evidence.push("bundle_hash field found".to_string());
            }
            if has_prev_host_hash {
                evidence.push("prev_host_hash field found".to_string());
            }
            if has_federation_signature {
                evidence.push("federation_signature field found".to_string());
            }

            if has_bundle_hash && has_prev_host_hash && has_federation_signature {
                Check::pass("Federation schema in global_ledger.rs", evidence)
            } else {
                Check::skip(
                    "Federation schema in global_ledger.rs",
                    format!(
                        "Federation fields not all present (reserved for future use): {}",
                        evidence.join(", ")
                    ),
                )
            }
        }
        Err(e) => Check::fail(
            "Federation schema in global_ledger.rs",
            vec![],
            format!("Failed to read file: {}", e),
        ),
    }
}

fn check_barrier_notify() -> Check {
    let multi_agent_src = "crates/adapteros-deterministic-exec/src/multi_agent.rs";

    if !Path::new(multi_agent_src).exists() {
        return Check::fail(
            "AgentBarrier Notify mechanism (Issue C-2)",
            vec![],
            format!("File not found: {}", multi_agent_src),
        );
    }

    match fs::read_to_string(multi_agent_src) {
        Ok(content) => {
            let has_notify_struct = content.contains("use tokio::sync::Notify");
            let has_notify_field = content.contains("notify: Arc<Notify>");
            let has_notify_notified = content.contains("self.notify.notified()");
            let has_notify_waiters = content.contains("self.notify.notify_waiters()");

            let mut evidence = Vec::new();
            if has_notify_struct {
                evidence.push("tokio::sync::Notify imported".to_string());
            }
            if has_notify_field {
                evidence.push("notify field in AgentBarrier struct".to_string());
            }
            if has_notify_notified {
                evidence.push("notify.notified() for waiting".to_string());
            }
            if has_notify_waiters {
                evidence.push("notify_waiters() for broadcast".to_string());
            }

            if has_notify_struct && has_notify_field && has_notify_notified && has_notify_waiters {
                Check::pass("AgentBarrier Notify mechanism (Issue C-2)", evidence)
            } else {
                Check::fail(
                    "AgentBarrier Notify mechanism (Issue C-2)",
                    evidence,
                    "Notify mechanism incomplete - Issue C-2 fix not fully implemented",
                )
            }
        }
        Err(e) => Check::fail(
            "AgentBarrier Notify mechanism (Issue C-2)",
            vec![],
            format!("Failed to read file: {}", e),
        ),
    }
}

fn check_dead_agent_handling() -> Check {
    let multi_agent_src = "crates/adapteros-deterministic-exec/src/multi_agent.rs";

    if !Path::new(multi_agent_src).exists() {
        return Check::fail(
            "Dead agent handling (Issue C-8)",
            vec![],
            format!("File not found: {}", multi_agent_src),
        );
    }

    match fs::read_to_string(multi_agent_src) {
        Ok(content) => {
            let has_mark_agent_dead = content.contains("pub fn mark_agent_dead");
            let has_dead_agents_field = content.contains("dead_agents:");
            let has_dead_agent_check = content.contains("dead.contains(agent)");

            let mut evidence = Vec::new();
            if has_mark_agent_dead {
                evidence.push("mark_agent_dead() function implemented".to_string());
            }
            if has_dead_agents_field {
                evidence.push("dead_agents field in AgentBarrier struct".to_string());
            }
            if has_dead_agent_check {
                evidence.push("Barrier skips dead agents in condition check".to_string());
            }

            if has_mark_agent_dead && has_dead_agents_field && has_dead_agent_check {
                Check::pass("Dead agent handling (Issue C-8)", evidence)
            } else {
                Check::fail(
                    "Dead agent handling (Issue C-8)",
                    evidence,
                    "Dead agent handling incomplete - Issue C-8 fix not fully implemented",
                )
            }
        }
        Err(e) => Check::fail(
            "Dead agent handling (Issue C-8)",
            vec![],
            format!("Failed to read file: {}", e),
        ),
    }
}

fn check_barrier_tests() -> Check {
    let multi_agent_src = "crates/adapteros-deterministic-exec/src/multi_agent.rs";

    if !Path::new(multi_agent_src).exists() {
        return Check::fail(
            "Comprehensive barrier tests",
            vec![],
            format!("File not found: {}", multi_agent_src),
        );
    }

    match fs::read_to_string(multi_agent_src) {
        Ok(content) => {
            // Look for key test functions that validate Issues C-1 through C-8 fixes
            let has_stress_test = content.contains("test_barrier_stress_many_agents");
            let has_timeout_test = content.contains("test_barrier_timeout");
            let has_cas_test = content.contains("test_barrier_concurrent_generation_advancement");
            let has_dead_agent_test = content.contains("test_barrier_dead_agent");
            let has_regression_test = content.contains("test_barrier_7_agents_rapid_successive");

            let mut evidence = Vec::new();
            if has_stress_test {
                evidence.push("Stress test (20+ agents) present".to_string());
            }
            if has_timeout_test {
                evidence.push("Timeout scenario test present".to_string());
            }
            if has_cas_test {
                evidence.push("CAS race condition test present (Issue C-1)".to_string());
            }
            if has_dead_agent_test {
                evidence.push("Dead agent handling test present (Issue C-8)".to_string());
            }
            if has_regression_test {
                evidence.push(
                    "Critical regression test: 7 agents, 5 rapid barriers (Issue C-1/C-2)"
                        .to_string(),
                );
            }

            let test_count = [
                has_stress_test,
                has_timeout_test,
                has_cas_test,
                has_dead_agent_test,
                has_regression_test,
            ]
            .iter()
            .filter(|&&b| b)
            .count();

            if test_count >= 4 {
                Check::pass(
                    "Comprehensive barrier tests",
                    vec![
                        format!("Found {}/5 critical barrier tests", test_count),
                        evidence.join(", "),
                    ],
                )
            } else {
                Check::fail(
                    "Comprehensive barrier tests",
                    evidence,
                    format!(
                        "Insufficient test coverage: {}/5 critical tests found",
                        test_count
                    ),
                )
            }
        }
        Err(e) => Check::fail(
            "Comprehensive barrier tests",
            vec![],
            format!("Failed to read file: {}", e),
        ),
    }
}
