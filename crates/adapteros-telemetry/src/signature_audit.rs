//! Signature audit trail logging
//!
//! Implements comprehensive audit logging for all signature operations
//! to ensure compliance and traceability.

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Audit log entry for signature operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureAuditEntry {
    pub sequence_no: u64,
    pub operation: SignatureOperation,
    pub hash: B3Hash,
    pub key_id: String,
    pub result: SignatureResult,
    pub timestamp: u64,
    pub context: BTreeMap<String, serde_json::Value>,
}

/// Signature operation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureOperation {
    Sign,
    Verify,
    KeyRotation,
    KeyGeneration,
}

/// Signature operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignatureResult {
    Success,
    Failure { reason: String },
}

/// Signature audit logger
pub struct SignatureAuditLogger {
    entries: Vec<SignatureAuditEntry>,
    sequence_counter: Arc<AtomicU64>,
}

impl SignatureAuditLogger {
    /// Create a new signature audit logger
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            sequence_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a signature verification operation
    pub fn record_sig_verification(
        &mut self,
        hash: B3Hash,
        key_id: &str,
        result: SignatureResult,
        sequence_no: u64,
    ) -> Result<()> {
        let entry = SignatureAuditEntry {
            sequence_no,
            operation: SignatureOperation::Verify,
            hash,
            key_id: key_id.to_string(),
            result,
            // Use unwrap_or_default to avoid panic if system clock is misconfigured
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            context: BTreeMap::new(),
        };

        self.entries.push(entry);
        Ok(())
    }

    /// Record a signature operation
    pub fn record_signature_operation(
        &mut self,
        operation: SignatureOperation,
        hash: B3Hash,
        key_id: &str,
        result: SignatureResult,
        context: BTreeMap<String, serde_json::Value>,
    ) -> Result<()> {
        let sequence_no = self.sequence_counter.fetch_add(1, Ordering::SeqCst);

        let entry = SignatureAuditEntry {
            sequence_no,
            operation,
            hash,
            key_id: key_id.to_string(),
            result,
            // Use unwrap_or_default to avoid panic if system clock is misconfigured
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            context,
        };

        self.entries.push(entry);
        Ok(())
    }

    /// Get all audit entries
    pub fn get_entries(&self) -> &[SignatureAuditEntry] {
        &self.entries
    }

    /// Get entries by operation type
    pub fn get_entries_by_operation(
        &self,
        operation: &SignatureOperation,
    ) -> Vec<&SignatureAuditEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                std::mem::discriminant(&entry.operation) == std::mem::discriminant(operation)
            })
            .collect()
    }

    /// Get entries by key ID
    pub fn get_entries_by_key(&self, key_id: &str) -> Vec<&SignatureAuditEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.key_id == key_id)
            .collect()
    }

    /// Export audit log to JSON
    pub fn export_json(&self) -> Result<String> {
        serde_json::to_string_pretty(&self.entries).map_err(AosError::Serialization)
    }

    /// Clear audit log
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl Default for SignatureAuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_audit_logging() {
        let mut logger = SignatureAuditLogger::new();
        let hash = B3Hash::hash(b"test_data");

        logger
            .record_sig_verification(hash, "test_key_001", SignatureResult::Success, 1)
            .unwrap();

        let entries = logger.get_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key_id, "test_key_001");
        assert!(matches!(entries[0].result, SignatureResult::Success));
    }

    #[test]
    fn test_audit_log_export() {
        let mut logger = SignatureAuditLogger::new();
        let hash = B3Hash::hash(b"test_data");

        logger
            .record_sig_verification(hash, "test_key_001", SignatureResult::Success, 1)
            .unwrap();

        let json = logger.export_json().unwrap();
        assert!(json.contains("test_key_001"));
        assert!(json.contains("Success"));
    }
}
