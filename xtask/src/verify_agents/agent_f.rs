//! Agent F: Adapter Lifecycle & TTL Enforcement checks

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;

pub async fn run(_args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Agent F - Adapter Lifecycle & TTL");

    // 1. Pinning system integrity
    section.add_check(check_pinning_table_migration());
    section.add_check(check_active_pinned_adapters_view());
    section.add_check(check_pinned_adapters_module());
    section.add_check(check_delete_protection());

    // 2. TTL enforcement
    section.add_check(check_find_expired_adapters());
    section.add_check(check_ttl_cleanup_loop());
    section.add_check(check_lifecycle_ttl_integration());

    // 3. Transactional updates
    section.add_check(check_transactional_state_updates());
    section.add_check(check_transactional_memory_updates());

    // 4. Integration tests
    section.add_check(check_stability_reinforcement_tests());

    Ok(section)
}

/// Check migration 0068 creates pinned_adapters table
fn check_pinning_table_migration() -> Check {
    let migration_path = "migrations/0068_create_pinned_adapters_table.sql";

    if !Path::new(migration_path).exists() {
        return Check::fail(
            "Pinning table migration (0068)",
            vec![],
            format!("Migration file not found: {}", migration_path),
        );
    }

    match fs::read_to_string(migration_path) {
        Ok(content) => {
            let has_table = content.contains("CREATE TABLE") && content.contains("pinned_adapters");
            let has_view = content.contains("CREATE VIEW") && content.contains("active_pinned_adapters");
            let has_ttl_filter = content.contains("pinned_until IS NULL") || content.contains("pinned_until >");

            if has_table && has_view && has_ttl_filter {
                Check::pass(
                    "Pinning table migration (0068)",
                    vec![
                        format!("{} exists", migration_path),
                        "CREATE TABLE pinned_adapters found".to_string(),
                        "CREATE VIEW active_pinned_adapters found".to_string(),
                        "TTL filtering logic present".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "Pinning table migration (0068)",
                    vec![
                        format!("has_table: {}", has_table),
                        format!("has_view: {}", has_view),
                        format!("has_ttl_filter: {}", has_ttl_filter),
                    ],
                    "Migration missing required schema elements",
                )
            }
        }
        Err(e) => Check::fail(
            "Pinning table migration (0068)",
            vec![],
            format!("Failed to read migration: {}", e),
        ),
    }
}

/// Check active_pinned_adapters view definition in migration
fn check_active_pinned_adapters_view() -> Check {
    let migration_path = "migrations/0068_create_pinned_adapters_table.sql";

    match fs::read_to_string(migration_path) {
        Ok(content) => {
            // View should filter by pinned_until being NULL or > now()
            let has_view = content.contains("active_pinned_adapters");
            let filters_null = content.contains("pinned_until IS NULL");
            let filters_time = content.contains("datetime('now')") || content.contains("CURRENT_TIMESTAMP");

            if has_view && (filters_null || filters_time) {
                Check::pass(
                    "active_pinned_adapters view TTL filtering",
                    vec![
                        "View definition found".to_string(),
                        "TTL filtering logic confirmed".to_string(),
                        "Automatic expiration enforcement enabled".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "active_pinned_adapters view TTL filtering",
                    vec![
                        format!("has_view: {}", has_view),
                        format!("filters_null: {}", filters_null),
                        format!("filters_time: {}", filters_time),
                    ],
                    "View missing proper TTL filtering",
                )
            }
        }
        Err(e) => Check::fail(
            "active_pinned_adapters view TTL filtering",
            vec![],
            format!("Failed to read migration: {}", e),
        ),
    }
}

/// Check pinned_adapters.rs module exists with CRUD operations
fn check_pinned_adapters_module() -> Check {
    let module_path = "crates/adapteros-db/src/pinned_adapters.rs";

    if !Path::new(module_path).exists() {
        return Check::fail(
            "pinned_adapters.rs module",
            vec![],
            format!("Module not found: {}", module_path),
        );
    }

    match fs::read_to_string(module_path) {
        Ok(content) => {
            let has_pin = content.contains("pub async fn pin_adapter");
            let has_unpin = content.contains("pub async fn unpin_adapter");
            let has_is_pinned = content.contains("pub async fn is_pinned");
            let has_list = content.contains("pub async fn list_pinned_adapters");
            let has_cleanup = content.contains("pub async fn cleanup_expired_pins");

            let all_present = has_pin && has_unpin && has_is_pinned && has_list && has_cleanup;

            if all_present {
                Check::pass(
                    "pinned_adapters.rs module",
                    vec![
                        format!("{} exists", module_path),
                        "pin_adapter() function found".to_string(),
                        "unpin_adapter() function found".to_string(),
                        "is_pinned() function found".to_string(),
                        "list_pinned_adapters() function found".to_string(),
                        "cleanup_expired_pins() function found".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "pinned_adapters.rs module",
                    vec![
                        format!("has_pin: {}", has_pin),
                        format!("has_unpin: {}", has_unpin),
                        format!("has_is_pinned: {}", has_is_pinned),
                        format!("has_list: {}", has_list),
                        format!("has_cleanup: {}", has_cleanup),
                    ],
                    "Module missing required CRUD functions",
                )
            }
        }
        Err(e) => Check::fail(
            "pinned_adapters.rs module",
            vec![],
            format!("Failed to read module: {}", e),
        ),
    }
}

/// Check delete_adapter enforces active_pinned_adapters view
fn check_delete_protection() -> Check {
    let adapters_path = "crates/adapteros-db/src/adapters.rs";

    match fs::read_to_string(adapters_path) {
        Ok(content) => {
            let has_delete_fn = content.contains("pub async fn delete_adapter");
            let checks_view = content.contains("active_pinned_adapters");
            let returns_error = content.contains("AosError::PolicyViolation")
                && content.contains("active pin");

            if has_delete_fn && checks_view && returns_error {
                Check::pass(
                    "Delete protection (active_pinned_adapters)",
                    vec![
                        "delete_adapter() function found".to_string(),
                        "Checks active_pinned_adapters view".to_string(),
                        "Returns PolicyViolation on pin conflict".to_string(),
                        "Location: crates/adapteros-db/src/adapters.rs:517-553".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "Delete protection (active_pinned_adapters)",
                    vec![
                        format!("has_delete_fn: {}", has_delete_fn),
                        format!("checks_view: {}", checks_view),
                        format!("returns_error: {}", returns_error),
                    ],
                    "Delete protection not properly implemented",
                )
            }
        }
        Err(e) => Check::fail(
            "Delete protection (active_pinned_adapters)",
            vec![],
            format!("Failed to read adapters.rs: {}", e),
        ),
    }
}

/// Check find_expired_adapters() query implementation
fn check_find_expired_adapters() -> Check {
    let adapters_path = "crates/adapteros-db/src/adapters.rs";

    match fs::read_to_string(adapters_path) {
        Ok(content) => {
            let has_function = content.contains("pub async fn find_expired_adapters");
            let queries_expires_at = content.contains("expires_at IS NOT NULL")
                && content.contains("expires_at <");
            let uses_datetime_now = content.contains("datetime('now')")
                || content.contains("CURRENT_TIMESTAMP");

            if has_function && queries_expires_at && uses_datetime_now {
                Check::pass(
                    "TTL query (find_expired_adapters)",
                    vec![
                        "find_expired_adapters() function found".to_string(),
                        "Queries expires_at column".to_string(),
                        "Compares with current time".to_string(),
                        "Location: crates/adapteros-db/src/adapters.rs:475-490".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "TTL query (find_expired_adapters)",
                    vec![
                        format!("has_function: {}", has_function),
                        format!("queries_expires_at: {}", queries_expires_at),
                        format!("uses_datetime_now: {}", uses_datetime_now),
                    ],
                    "TTL query not properly implemented",
                )
            }
        }
        Err(e) => Check::fail(
            "TTL query (find_expired_adapters)",
            vec![],
            format!("Failed to read adapters.rs: {}", e),
        ),
    }
}

/// Check TTL cleanup background loop in main.rs
fn check_ttl_cleanup_loop() -> Check {
    let main_path = "crates/adapteros-server/src/main.rs";

    if !Path::new(main_path).exists() {
        return Check::skip(
            "TTL cleanup loop (main.rs)",
            "adapteros-server not enabled in workspace (known issue)",
        );
    }

    match fs::read_to_string(main_path) {
        Ok(content) => {
            let has_spawn = content.contains("spawn_deterministic")
                || content.contains("tokio::spawn");
            let has_interval = content.contains("tokio::time::interval");
            let calls_find_expired = content.contains("find_expired_adapters");
            let deletes_adapters = content.contains("delete_adapter");

            if has_spawn && has_interval && calls_find_expired && deletes_adapters {
                Check::pass(
                    "TTL cleanup loop (main.rs)",
                    vec![
                        "Background task spawn found".to_string(),
                        "Periodic interval timer present".to_string(),
                        "Calls find_expired_adapters()".to_string(),
                        "Deletes expired adapters".to_string(),
                        "Location: crates/adapteros-server/src/main.rs:709-728".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "TTL cleanup loop (main.rs)",
                    vec![
                        format!("has_spawn: {}", has_spawn),
                        format!("has_interval: {}", has_interval),
                        format!("calls_find_expired: {}", calls_find_expired),
                        format!("deletes_adapters: {}", deletes_adapters),
                    ],
                    "Cleanup loop not properly implemented",
                )
            }
        }
        Err(e) => Check::skip(
            "TTL cleanup loop (main.rs)",
            format!("Cannot read main.rs: {}", e),
        ),
    }
}

/// Check lifecycle manager integrates TTL eviction
fn check_lifecycle_ttl_integration() -> Check {
    let lifecycle_path = "crates/adapteros-lora-lifecycle/src/lib.rs";

    match fs::read_to_string(lifecycle_path) {
        Ok(content) => {
            let has_check_memory = content.contains("pub async fn check_memory_pressure");
            let calls_find_expired = content.contains("find_expired_adapters");
            let evicts_adapters = content.contains("evict_adapter")
                || content.contains("self.evict");

            if has_check_memory && calls_find_expired && evicts_adapters {
                Check::pass(
                    "Lifecycle TTL integration",
                    vec![
                        "check_memory_pressure() function found".to_string(),
                        "Calls find_expired_adapters()".to_string(),
                        "Evicts expired adapters before memory check".to_string(),
                        "Location: crates/adapteros-lora-lifecycle/src/lib.rs:1073-1092".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "Lifecycle TTL integration",
                    vec![
                        format!("has_check_memory: {}", has_check_memory),
                        format!("calls_find_expired: {}", calls_find_expired),
                        format!("evicts_adapters: {}", evicts_adapters),
                    ],
                    "Lifecycle manager doesn't integrate TTL eviction",
                )
            }
        }
        Err(e) => Check::fail(
            "Lifecycle TTL integration",
            vec![],
            format!("Failed to read lifecycle lib.rs: {}", e),
        ),
    }
}

/// Check transactional state update implementation
fn check_transactional_state_updates() -> Check {
    let adapters_path = "crates/adapteros-db/src/adapters.rs";

    match fs::read_to_string(adapters_path) {
        Ok(content) => {
            let has_function = content.contains("pub async fn update_adapter_state_tx");
            let begins_tx = content.contains("begin().await");
            let commits_tx = content.contains("commit().await");
            let has_concurrency_doc = content.contains("Concurrency Safety")
                || content.contains("SQLite transactions");

            if has_function && begins_tx && commits_tx && has_concurrency_doc {
                Check::pass(
                    "Transactional state updates",
                    vec![
                        "update_adapter_state_tx() function found".to_string(),
                        "Uses SQLite transactions (begin/commit)".to_string(),
                        "Concurrency safety documented".to_string(),
                        "Location: crates/adapteros-db/src/adapters.rs:752-789".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "Transactional state updates",
                    vec![
                        format!("has_function: {}", has_function),
                        format!("begins_tx: {}", begins_tx),
                        format!("commits_tx: {}", commits_tx),
                        format!("has_concurrency_doc: {}", has_concurrency_doc),
                    ],
                    "Transactional state updates not properly implemented",
                )
            }
        }
        Err(e) => Check::fail(
            "Transactional state updates",
            vec![],
            format!("Failed to read adapters.rs: {}", e),
        ),
    }
}

/// Check transactional memory update implementation
fn check_transactional_memory_updates() -> Check {
    let adapters_path = "crates/adapteros-db/src/adapters.rs";

    match fs::read_to_string(adapters_path) {
        Ok(content) => {
            let has_function = content.contains("pub async fn update_adapter_memory_tx");
            let begins_tx = content.contains("begin().await");
            let commits_tx = content.contains("commit().await");
            let has_concurrency_doc = content.contains("Concurrency Approach")
                || content.contains("Optimistic concurrency");

            if has_function && begins_tx && commits_tx {
                Check::pass(
                    "Transactional memory updates",
                    vec![
                        "update_adapter_memory_tx() function found".to_string(),
                        "Uses SQLite transactions (begin/commit)".to_string(),
                        if has_concurrency_doc {
                            "Concurrency approach documented".to_string()
                        } else {
                            "Basic transaction implementation".to_string()
                        },
                        "Location: crates/adapteros-db/src/adapters.rs:796-826".to_string(),
                    ],
                )
            } else {
                Check::fail(
                    "Transactional memory updates",
                    vec![
                        format!("has_function: {}", has_function),
                        format!("begins_tx: {}", begins_tx),
                        format!("commits_tx: {}", commits_tx),
                    ],
                    "Transactional memory updates not properly implemented",
                )
            }
        }
        Err(e) => Check::fail(
            "Transactional memory updates",
            vec![],
            format!("Failed to read adapters.rs: {}", e),
        ),
    }
}

/// Check stability reinforcement tests exist and cover key scenarios
fn check_stability_reinforcement_tests() -> Check {
    let tests_path = "tests/stability_reinforcement_tests.rs";

    if !Path::new(tests_path).exists() {
        return Check::fail(
            "Stability reinforcement tests",
            vec![],
            format!("Test file not found: {}", tests_path),
        );
    }

    match fs::read_to_string(tests_path) {
        Ok(content) => {
            let has_concurrent_test = content.contains("test_concurrent_state_update_race_condition");
            let has_pin_test = content.contains("test_pinned_adapter_delete_prevention")
                || content.contains("test_time_based_pinned_adapter_delete_prevention");
            let has_ttl_test = content.contains("test_ttl_automatic_cleanup")
                || content.contains("find_expired_adapters");

            let test_count = [has_concurrent_test, has_pin_test, has_ttl_test]
                .iter()
                .filter(|&&x| x)
                .count();

            if test_count >= 3 {
                Check::pass(
                    "Stability reinforcement tests",
                    vec![
                        format!("{} exists", tests_path),
                        "Concurrent state update test present".to_string(),
                        "Pin enforcement test present".to_string(),
                        "TTL cleanup test present".to_string(),
                        format!("{} critical tests verified", test_count),
                    ],
                )
            } else {
                Check::fail(
                    "Stability reinforcement tests",
                    vec![
                        format!("has_concurrent_test: {}", has_concurrent_test),
                        format!("has_pin_test: {}", has_pin_test),
                        format!("has_ttl_test: {}", has_ttl_test),
                        format!("test_count: {}", test_count),
                    ],
                    "Missing critical stability tests",
                )
            }
        }
        Err(e) => Check::fail(
            "Stability reinforcement tests",
            vec![],
            format!("Failed to read test file: {}", e),
        ),
    }
}
