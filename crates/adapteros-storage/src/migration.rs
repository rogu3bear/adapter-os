//! SQL to KV migration infrastructure
//!
//! Provides types and traits for migrating data from SQL database to
//! key-value storage backend. The actual migration implementation
//! lives in adapteros-db to avoid circular dependencies.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Migration error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationError {
    pub record_id: String,
    pub error: String,
}

/// Migration report for a single entity type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub entity_type: String,
    pub total_records: usize,
    pub migrated: usize,
    pub failed: usize,
    pub errors: Vec<MigrationError>,
    pub duration: Duration,
}

impl MigrationReport {
    /// Create a new report
    pub fn new(entity_type: &str) -> Self {
        Self {
            entity_type: entity_type.to_string(),
            total_records: 0,
            migrated: 0,
            failed: 0,
            errors: Vec::new(),
            duration: Duration::ZERO,
        }
    }

    /// Check if migration was successful
    pub fn success(&self) -> bool {
        self.failed == 0 && self.migrated == self.total_records
    }

    /// Get success percentage
    pub fn success_percentage(&self) -> f64 {
        if self.total_records == 0 {
            100.0
        } else {
            (self.migrated as f64 / self.total_records as f64) * 100.0
        }
    }

    /// Add a record error
    pub fn add_error(&mut self, record_id: String, error: String) {
        self.failed += 1;
        self.errors.push(MigrationError { record_id, error });
    }

    /// Mark a record as migrated
    pub fn mark_migrated(&mut self) {
        self.migrated += 1;
    }

    /// Set total records
    pub fn set_total(&mut self, total: usize) {
        self.total_records = total;
    }

    /// Set duration
    pub fn set_duration(&mut self, duration: Duration) {
        self.duration = duration;
    }
}

/// Verification report for migration integrity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationReport {
    pub entity_type: String,
    pub sql_count: usize,
    pub kv_count: usize,
    pub mismatches: Vec<String>,
    pub success: bool,
}

impl VerificationReport {
    /// Create a new verification report
    pub fn new(entity_type: &str) -> Self {
        Self {
            entity_type: entity_type.to_string(),
            sql_count: 0,
            kv_count: 0,
            mismatches: Vec::new(),
            success: false,
        }
    }

    /// Check if verification passed
    pub fn passed(&self) -> bool {
        self.success && self.sql_count == self.kv_count && self.mismatches.is_empty()
    }

    /// Add a mismatch
    pub fn add_mismatch(&mut self, description: String) {
        self.mismatches.push(description);
        self.success = false;
    }

    /// Mark as successful
    pub fn mark_success(&mut self) {
        if self.mismatches.is_empty() && self.sql_count == self.kv_count {
            self.success = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_report_success() {
        let mut report = MigrationReport::new("tenants");
        report.set_total(100);
        for _ in 0..100 {
            report.mark_migrated();
        }
        report.set_duration(Duration::from_secs(1));

        assert!(report.success());
        assert_eq!(report.success_percentage(), 100.0);
    }

    #[test]
    fn test_migration_report_partial() {
        let mut report = MigrationReport::new("adapters");
        report.set_total(100);
        for _ in 0..95 {
            report.mark_migrated();
        }
        for i in 0..5 {
            report.add_error(format!("adapter-{}", i), "test error".to_string());
        }
        report.set_duration(Duration::from_secs(2));

        assert!(!report.success());
        assert_eq!(report.success_percentage(), 95.0);
    }

    #[test]
    fn test_verification_report_passed() {
        let mut report = VerificationReport::new("tenants");
        report.sql_count = 100;
        report.kv_count = 100;
        report.mark_success();

        assert!(report.passed());
    }

    #[test]
    fn test_verification_report_failed() {
        let mut report = VerificationReport::new("adapters");
        report.sql_count = 100;
        report.kv_count = 95;
        report.add_mismatch("adapter:test missing".to_string());

        assert!(!report.passed());
    }
}
