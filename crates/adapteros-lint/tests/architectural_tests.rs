//! Tests for architectural lint rules

use adapteros_lint::architectural::{check_file, ArchitecturalViolation};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("create temp dir")
}

#[test]
fn test_non_transactional_fallback_detection() {
    let test_code = r#"
pub async fn load_adapter_handler(state: AppState) -> Result<()> {
    if let Some(ref lifecycle) = state.lifecycle_manager {
        // Use lifecycle manager
    } else {
        // Violation: Should use update_adapter_state_tx
        state.db.update_adapter_state(&adapter_id, "loading", "fallback").await?;
    }
    Ok(())
}
"#;

    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test_handlers.rs");
    fs::write(&test_file, test_code).unwrap();

    let violations = check_file(&test_file);
    assert!(violations
        .iter()
        .any(|v| matches!(v, ArchitecturalViolation::NonTransactionalFallback { .. })));
}

#[test]
fn test_acceptable_select_query() {
    let test_code = r#"
pub async fn get_status(state: AppState) -> Result<()> {
    // Acceptable: Read-only SELECT query
    let result = sqlx::query("SELECT last_run FROM determinism_checks LIMIT 1")
        .fetch_optional(state.db.pool())
        .await?;
    Ok(())
}
"#;

    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test_handlers.rs");
    fs::write(&test_file, test_code).unwrap();

    let violations = check_file(&test_file);
    // SELECT queries should not be flagged
    assert!(!violations
        .iter()
        .any(|v| matches!(v, ArchitecturalViolation::DirectSqlInHandler { .. })));
}

#[test]
fn test_transaction_context_acceptable() {
    let test_code = r#"
pub async fn update_in_transaction(state: AppState) -> Result<()> {
    let mut tx = state.db.pool().begin().await?;
    // Acceptable: Inside transaction
    sqlx::query("UPDATE adapters SET tier = ? WHERE adapter_id = ?")
        .bind(&new_tier)
        .bind(&adapter_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
"#;

    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test_handlers.rs");
    fs::write(&test_file, test_code).unwrap();

    let violations = check_file(&test_file);
    // UPDATE in transaction context should not be flagged
    assert!(!violations
        .iter()
        .any(|v| matches!(v, ArchitecturalViolation::DirectSqlInHandler { .. })));
}

#[test]
fn test_lifecycle_manager_bypass_detection() {
    let test_code = r#"
pub async fn bad_handler(state: AppState) -> Result<()> {
    // Violation: Direct DB update before lifecycle manager check
    state.db.update_adapter_state_tx(&adapter_id, "cold", "direct").await?;
    // No lifecycle manager usage
    Ok(())
}
"#;

    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test_handlers.rs");
    fs::write(&test_file, test_code).unwrap();

    let violations = check_file(&test_file);
    assert!(violations
        .iter()
        .any(|v| matches!(v, ArchitecturalViolation::LifecycleManagerBypass { .. })));
}

#[test]
fn test_acceptable_lifecycle_manager_pattern() {
    let test_code = r#"
pub async fn good_handler(state: AppState) -> Result<()> {
    if let Some(ref lifecycle) = state.lifecycle_manager {
        let manager = lifecycle.lock().await;
        manager.update_adapter_state(adapter_idx, AdapterState::Cold, "reason").await?;
    } else {
        // Acceptable fallback
        state.db.update_adapter_state_tx(&adapter_id, "cold", "fallback").await?;
    }
    Ok(())
}
"#;

    let temp_dir = new_test_tempdir();
    let test_file = temp_dir.path().join("test_handlers.rs");
    fs::write(&test_file, test_code).unwrap();

    let violations = check_file(&test_file);
    // Should not flag lifecycle manager pattern or transactional fallback
    assert!(!violations
        .iter()
        .any(|v| matches!(v, ArchitecturalViolation::LifecycleManagerBypass { .. })));
    assert!(!violations
        .iter()
        .any(|v| matches!(v, ArchitecturalViolation::NonTransactionalFallback { .. })));
}
