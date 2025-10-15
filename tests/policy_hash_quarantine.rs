//! Policy Hash Quarantine Integration Tests
//!
//! End-to-end tests for the policy hash watcher and quarantine system.
//! Tests the complete flow from hash registration to quarantine enforcement.

use adapteros_core::{AosError, B3Hash};
use adapteros_db::Db;
use adapteros_policy::{PolicyHashWatcher, QuarantineManager, QuarantineOperation};
use adapteros_telemetry::{TelemetryWriter, ValidationStatus};
use std::sync::Arc;
use tempfile::TempDir;

/// Setup test environment with database and telemetry
async fn setup_test_env() -> (Arc<Db>, Arc<TelemetryWriter>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}", db_path.display());
    
    let db = Db::connect(&db_url).await.unwrap();
    db.migrate().await.unwrap();

    let telemetry_dir = temp_dir.path().join("telemetry");
    std::fs::create_dir_all(&telemetry_dir).unwrap();
    let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();

    (Arc::new(db), Arc::new(telemetry), temp_dir)
}

#[tokio::test]
async fn test_e2e_hash_mismatch_detection() {
    let (db, telemetry, _temp) = setup_test_env().await;
    
    // Create watcher
    let watcher = PolicyHashWatcher::new(
        db.clone(),
        telemetry.clone(),
        Some("test-cp".to_string()),
    );
    
    // Register baseline hash
    let baseline_hash = B3Hash::hash(b"original policy config");
    watcher
        .register_baseline("TestPolicy", &baseline_hash, None)
        .await
        .unwrap();
    
    // Validate with matching hash - should pass
    let result = watcher
        .validate_policy_pack("TestPolicy", &baseline_hash)
        .await
        .unwrap();
    assert!(result.valid);
    assert_eq!(result.status, ValidationStatus::Valid);
    assert!(!watcher.is_quarantined());
    
    // Simulate policy mutation
    let mutated_hash = B3Hash::hash(b"MUTATED policy config");
    
    // Validate with mutated hash - should fail
    let result = watcher
        .validate_policy_pack("TestPolicy", &mutated_hash)
        .await
        .unwrap();
    assert!(!result.valid);
    assert_eq!(result.status, ValidationStatus::Mismatch);
    
    // System should now be quarantined
    assert!(watcher.is_quarantined());
    assert_eq!(watcher.violation_count(), 1);
    
    // Get violations
    let violations = watcher.get_violations();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].policy_pack_id, "TestPolicy");
    assert_eq!(violations[0].expected_hash, baseline_hash);
    assert_eq!(violations[0].actual_hash, mutated_hash);
}

#[tokio::test]
async fn test_e2e_quarantine_enforcement() {
    let (db, telemetry, _temp) = setup_test_env().await;
    
    let watcher = PolicyHashWatcher::new(
        db.clone(),
        telemetry.clone(),
        Some("test-cp".to_string()),
    );
    
    // Register and mutate to trigger quarantine
    let baseline = B3Hash::hash(b"config v1");
    watcher.register_baseline("Policy1", &baseline, None).await.unwrap();
    
    let mutated = B3Hash::hash(b"config v2");
    watcher.validate_policy_pack("Policy1", &mutated).await.unwrap();
    
    assert!(watcher.is_quarantined());
    
    // Create quarantine manager
    let mut qm = QuarantineManager::new();
    let violation_summary = format!(
        "Policy hash mismatch detected: {} violation(s)",
        watcher.violation_count()
    );
    qm.set_quarantined(true, violation_summary);
    
    // Test operation enforcement
    assert!(qm.check_operation(QuarantineOperation::Inference).is_err());
    assert!(qm.check_operation(QuarantineOperation::AdapterLoad).is_err());
    assert!(qm.check_operation(QuarantineOperation::Training).is_err());
    
    // Audit operations should still work
    assert!(qm.check_operation(QuarantineOperation::Audit).is_ok());
    assert!(qm.check_operation(QuarantineOperation::Status).is_ok());
    assert!(qm.check_operation(QuarantineOperation::Metrics).is_ok());
}

#[tokio::test]
async fn test_e2e_quarantine_resolution() {
    let (db, telemetry, _temp) = setup_test_env().await;
    
    let watcher = PolicyHashWatcher::new(
        db.clone(),
        telemetry.clone(),
        Some("test-cp".to_string()),
    );
    
    // Trigger quarantine
    let baseline = B3Hash::hash(b"original");
    watcher.register_baseline("Policy1", &baseline, None).await.unwrap();
    
    let mutated = B3Hash::hash(b"mutated");
    watcher.validate_policy_pack("Policy1", &mutated).await.unwrap();
    
    assert!(watcher.is_quarantined());
    
    // Operator clears violation
    watcher.clear_violations("Policy1").unwrap();
    
    // System should no longer be quarantined
    assert!(!watcher.is_quarantined());
    assert_eq!(watcher.violation_count(), 0);
}

#[tokio::test]
async fn test_e2e_database_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}", db_path.display());
    
    let telemetry_dir = temp_dir.path().join("telemetry");
    std::fs::create_dir_all(&telemetry_dir).unwrap();
    
    // First session: register baseline
    {
        let db = Db::connect(&db_url).await.unwrap();
        db.migrate().await.unwrap();
        
        let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();
        
        let watcher = PolicyHashWatcher::new(
            Arc::new(db),
            Arc::new(telemetry),
            Some("test-cp".to_string()),
        );
        
        let hash = B3Hash::hash(b"persistent config");
        watcher.register_baseline("PersistentPolicy", &hash, None).await.unwrap();
    }
    
    // Second session: load from database
    {
        let db = Db::connect(&db_url).await.unwrap();
        let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();
        
        let watcher = PolicyHashWatcher::new(
            Arc::new(db),
            Arc::new(telemetry),
            Some("test-cp".to_string()),
        );
        
        // Load cache from database
        watcher.load_cache().await.unwrap();
        
        // Validate - should find baseline in database
        let hash = B3Hash::hash(b"persistent config");
        let result = watcher
            .validate_policy_pack("PersistentPolicy", &hash)
            .await
            .unwrap();
        
        assert!(result.valid);
        assert_eq!(result.status, ValidationStatus::Valid);
    }
}

#[tokio::test]
async fn test_e2e_telemetry_logging() {
    let (db, telemetry, temp) = setup_test_env().await;
    
    let watcher = PolicyHashWatcher::new(
        db.clone(),
        telemetry.clone(),
        Some("test-cp".to_string()),
    );
    
    let baseline = B3Hash::hash(b"logged config");
    watcher.register_baseline("LogPolicy", &baseline, None).await.unwrap();
    
    // Valid hash - should log "valid" event
    watcher.validate_policy_pack("LogPolicy", &baseline).await.unwrap();
    
    // Mismatched hash - should log "mismatch" event
    let mutated = B3Hash::hash(b"changed config");
    watcher.validate_policy_pack("LogPolicy", &mutated).await.unwrap();
    
    // Missing baseline - should log "missing" event
    let new_hash = B3Hash::hash(b"new config");
    watcher.validate_policy_pack("UnknownPolicy", &new_hash).await.unwrap();
    
    // Verify telemetry files were created
    let telemetry_dir = temp.path().join("telemetry");
    assert!(telemetry_dir.exists());
    
    // Note: Full telemetry verification would require parsing NDJSON files
    // For now, we verify that events were logged without errors
}

#[tokio::test]
async fn test_e2e_multiple_violations() {
    let (db, telemetry, _temp) = setup_test_env().await;
    
    let watcher = PolicyHashWatcher::new(
        db.clone(),
        telemetry.clone(),
        Some("test-cp".to_string()),
    );
    
    // Register multiple policy packs
    let hash1 = B3Hash::hash(b"policy1 config");
    let hash2 = B3Hash::hash(b"policy2 config");
    let hash3 = B3Hash::hash(b"policy3 config");
    
    watcher.register_baseline("Policy1", &hash1, None).await.unwrap();
    watcher.register_baseline("Policy2", &hash2, None).await.unwrap();
    watcher.register_baseline("Policy3", &hash3, None).await.unwrap();
    
    // Mutate two of them
    let mut1 = B3Hash::hash(b"policy1 MUTATED");
    let mut3 = B3Hash::hash(b"policy3 MUTATED");
    
    watcher.validate_policy_pack("Policy1", &mut1).await.unwrap();
    watcher.validate_policy_pack("Policy2", &hash2).await.unwrap();  // Still valid
    watcher.validate_policy_pack("Policy3", &mut3).await.unwrap();
    
    // Should have 2 violations
    assert!(watcher.is_quarantined());
    assert_eq!(watcher.violation_count(), 2);
    
    let violations = watcher.get_violations();
    assert_eq!(violations.len(), 2);
    
    // Clear one violation
    watcher.clear_violations("Policy1").unwrap();
    
    // Should still be quarantined (Policy3 still violated)
    assert!(watcher.is_quarantined());
    assert_eq!(watcher.violation_count(), 1);
    
    // Clear remaining violation
    watcher.clear_violations("Policy3").unwrap();
    
    // Now should be clear
    assert!(!watcher.is_quarantined());
    assert_eq!(watcher.violation_count(), 0);
}

