//! Policy Hash Watcher
//!
//! Detects runtime policy pack mutations and triggers quarantine.
//! Implements hybrid persistence model:
//! - Baseline hashes in database (persistent, audit trail)
//! - Runtime cache for O(1) validation
//! - In-memory delta buffer for violation tracking
//!
//! Per Determinism Ruleset #2: "refuse to serve if policy hashes don't match"

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_db::{Db, PolicyHashRecord};
use adapteros_telemetry::{PolicyHashValidationEvent, TelemetryWriter, ValidationStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Hash violation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashViolation {
    pub policy_pack_id: String,
    pub expected_hash: B3Hash,
    pub actual_hash: B3Hash,
    pub detected_at: u64,
    pub cpid: Option<String>,
}

/// Validation result for a policy pack
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub status: ValidationStatus,
    pub baseline_hash: Option<B3Hash>,
    pub current_hash: B3Hash,
}

/// Policy hash watcher with hybrid persistence
pub struct PolicyHashWatcher {
    /// Database handle for persistence
    db: Arc<Db>,
    
    /// Telemetry writer for logging
    telemetry: Arc<TelemetryWriter>,
    
    /// In-memory cache: policy_pack_id -> baseline_hash
    /// Uses RwLock for concurrent reads during hot path
    cache: Arc<RwLock<HashMap<String, B3Hash>>>,
    
    /// Detected violations buffer
    violations: Arc<RwLock<Vec<HashViolation>>>,
    
    /// Control Plane ID (optional)
    cpid: Option<String>,
}

impl PolicyHashWatcher {
    /// Create a new policy hash watcher
    pub fn new(db: Arc<Db>, telemetry: Arc<TelemetryWriter>, cpid: Option<String>) -> Self {
        Self {
            db,
            telemetry,
            cache: Arc::new(RwLock::new(HashMap::new())),
            violations: Arc::new(RwLock::new(Vec::new())),
            cpid,
        }
    }

    /// Register a baseline hash for a policy pack
    pub async fn register_baseline(
        &self,
        policy_pack_id: &str,
        baseline_hash: &B3Hash,
        signer_pubkey: Option<&str>,
    ) -> Result<()> {
        info!(
            policy_pack_id = %policy_pack_id,
            hash = %baseline_hash.to_hex(),
            cpid = ?self.cpid,
            "Registering policy pack baseline hash"
        );

        // Store in database
        self.db
            .insert_policy_hash(
                policy_pack_id,
                baseline_hash,
                self.cpid.as_deref(),
                signer_pubkey,
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to register policy hash: {}", e)))?;

        // Update cache
        {
            let mut cache = self.cache.write().unwrap();
            cache.insert(policy_pack_id.to_string(), *baseline_hash);
        }

        debug!(
            policy_pack_id = %policy_pack_id,
            "Baseline hash registered successfully"
        );

        Ok(())
    }

    /// Validate a policy pack hash against baseline
    pub async fn validate_policy_pack(
        &self,
        policy_pack_id: &str,
        current_hash: &B3Hash,
    ) -> Result<ValidationResult> {
        // Try cache first (O(1) lookup)
        let baseline_hash = {
            let cache = self.cache.read().unwrap();
            cache.get(policy_pack_id).copied()
        };

        // If not in cache, load from database
        let baseline_hash = if let Some(hash) = baseline_hash {
            Some(hash)
        } else {
            match self.db.get_policy_hash(policy_pack_id, self.cpid.as_deref()).await {
                Ok(Some(record)) => {
                    // Populate cache
                    let mut cache = self.cache.write().unwrap();
                    cache.insert(policy_pack_id.to_string(), record.baseline_hash);
                    Some(record.baseline_hash)
                }
                Ok(None) => None,
                Err(e) => {
                    error!(
                        policy_pack_id = %policy_pack_id,
                        error = %e,
                        "Failed to load policy hash from database"
                    );
                    return Err(AosError::Database(format!("Failed to load policy hash: {}", e)));
                }
            }
        };

        // Validate hash
        let (valid, status) = if let Some(baseline) = baseline_hash {
            if baseline == *current_hash {
                (true, ValidationStatus::Valid)
            } else {
                (false, ValidationStatus::Mismatch)
            }
        } else {
            (false, ValidationStatus::Missing)
        };

        // Log telemetry event (100% sampling)
        let event = match status {
            ValidationStatus::Valid => {
                PolicyHashValidationEvent::valid(
                    policy_pack_id.to_string(),
                    current_hash.to_hex(),
                    self.cpid.clone(),
                )
            }
            ValidationStatus::Mismatch => {
                let prev_hash = baseline_hash.unwrap().to_hex();
                warn!(
                    policy_pack_id = %policy_pack_id,
                    expected = %prev_hash,
                    actual = %current_hash.to_hex(),
                    "Policy pack hash mismatch detected"
                );

                // Record violation
                self.record_violation(
                    policy_pack_id,
                    baseline_hash.unwrap(),
                    *current_hash,
                );

                PolicyHashValidationEvent::mismatch(
                    policy_pack_id.to_string(),
                    prev_hash,
                    current_hash.to_hex(),
                    self.cpid.clone(),
                )
            }
            ValidationStatus::Missing => {
                debug!(
                    policy_pack_id = %policy_pack_id,
                    "No baseline hash found for policy pack"
                );

                PolicyHashValidationEvent::missing(
                    policy_pack_id.to_string(),
                    current_hash.to_hex(),
                    self.cpid.clone(),
                )
            }
        };

        if let Err(e) = self.telemetry.log_policy_hash_validation(event) {
            error!(error = %e, "Failed to log policy hash validation event");
        }

        Ok(ValidationResult {
            valid,
            status,
            baseline_hash,
            current_hash: *current_hash,
        })
    }

    /// Record a hash violation
    fn record_violation(&self, policy_pack_id: &str, expected: B3Hash, actual: B3Hash) {
        let violation = HashViolation {
            policy_pack_id: policy_pack_id.to_string(),
            expected_hash: expected,
            actual_hash: actual,
            detected_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            cpid: self.cpid.clone(),
        };

        let mut violations = self.violations.write().unwrap();
        violations.push(violation);
    }

    /// Get all detected violations
    pub fn get_violations(&self) -> Vec<HashViolation> {
        let violations = self.violations.read().unwrap();
        violations.clone()
    }

    /// Clear violations for a specific policy pack
    pub fn clear_violations(&self, policy_pack_id: &str) -> Result<()> {
        info!(
            policy_pack_id = %policy_pack_id,
            "Clearing policy hash violations"
        );

        let mut violations = self.violations.write().unwrap();
        violations.retain(|v| v.policy_pack_id != policy_pack_id);

        Ok(())
    }

    /// Clear all violations
    pub fn clear_all_violations(&self) -> Result<()> {
        info!("Clearing all policy hash violations");

        let mut violations = self.violations.write().unwrap();
        violations.clear();

        Ok(())
    }

    /// Check if system is quarantined (any violations present)
    pub fn is_quarantined(&self) -> bool {
        let violations = self.violations.read().unwrap();
        !violations.is_empty()
    }

    /// Get count of violations
    pub fn violation_count(&self) -> usize {
        let violations = self.violations.read().unwrap();
        violations.len()
    }

    /// Validate all registered policy packs
    pub async fn validate_all_policies(&self, policy_hashes: &HashMap<String, B3Hash>) -> Result<()> {
        debug!("Validating all policy pack hashes");

        let mut any_violations = false;

        for (policy_pack_id, current_hash) in policy_hashes {
            match self.validate_policy_pack(policy_pack_id, current_hash).await {
                Ok(result) => {
                    if !result.valid {
                        any_violations = true;
                    }
                }
                Err(e) => {
                    error!(
                        policy_pack_id = %policy_pack_id,
                        error = %e,
                        "Failed to validate policy pack"
                    );
                }
            }
        }

        if any_violations {
            warn!("Policy hash violations detected during validation sweep");
        } else {
            debug!("All policy packs validated successfully");
        }

        Ok(())
    }

    /// Load all baseline hashes from database into cache
    pub async fn load_cache(&self) -> Result<()> {
        info!(cpid = ?self.cpid, "Loading policy hashes into cache");

        let records = self.db
            .list_policy_hashes(self.cpid.as_deref())
            .await
            .map_err(|e| AosError::Database(format!("Failed to list policy hashes: {}", e)))?;

        let mut cache = self.cache.write().unwrap();
        for record in records {
            cache.insert(record.policy_pack_id.clone(), record.baseline_hash);
        }

        info!(count = cache.len(), "Policy hash cache loaded");

        Ok(())
    }

    /// Start background watcher task
    /// 
    /// Runs periodic validation sweeps at the specified interval.
    /// This is non-deterministic but provides continuous monitoring.
    pub fn start_background_watcher(
        self: Arc<Self>,
        interval: Duration,
        policy_hashes: Arc<RwLock<HashMap<String, B3Hash>>>,
    ) -> JoinHandle<()> {
        info!(interval_secs = ?interval.as_secs(), "Starting background policy hash watcher");

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                ticker.tick().await;

                debug!("Background policy hash validation sweep");

                let hashes = {
                    let lock = policy_hashes.read().unwrap();
                    lock.clone()
                };

                if let Err(e) = self.validate_all_policies(&hashes).await {
                    error!(error = %e, "Background hash validation failed");
                }

                if self.is_quarantined() {
                    warn!(
                        violation_count = self.violation_count(),
                        "System quarantined due to policy hash violations"
                    );
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup_test_watcher() -> (PolicyHashWatcher, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());
        
        let db = Db::connect(&db_url).await.unwrap();
        db.migrate().await.unwrap();

        let telemetry_dir = temp_dir.path().join("telemetry");
        std::fs::create_dir_all(&telemetry_dir).unwrap();
        let telemetry = TelemetryWriter::new(&telemetry_dir, 1000, 1024 * 1024).unwrap();

        let watcher = PolicyHashWatcher::new(
            Arc::new(db),
            Arc::new(telemetry),
            Some("test-cp".to_string()),
        );

        (watcher, temp_dir)
    }

    #[tokio::test]
    async fn test_register_baseline() {
        let (watcher, _temp) = setup_test_watcher().await;

        let hash = B3Hash::hash(b"test policy config");
        watcher
            .register_baseline("test_policy", &hash, None)
            .await
            .unwrap();

        // Verify it's in cache
        let cache = watcher.cache.read().unwrap();
        assert_eq!(cache.get("test_policy"), Some(&hash));
    }

    #[tokio::test]
    async fn test_validate_matching_hash() {
        let (watcher, _temp) = setup_test_watcher().await;

        let hash = B3Hash::hash(b"test policy config");
        watcher
            .register_baseline("test_policy", &hash, None)
            .await
            .unwrap();

        let result = watcher.validate_policy_pack("test_policy", &hash).await.unwrap();
        assert!(result.valid);
        assert_eq!(result.status, ValidationStatus::Valid);
    }

    #[tokio::test]
    async fn test_validate_mismatched_hash() {
        let (watcher, _temp) = setup_test_watcher().await;

        let baseline_hash = B3Hash::hash(b"original config");
        watcher
            .register_baseline("test_policy", &baseline_hash, None)
            .await
            .unwrap();

        let mutated_hash = B3Hash::hash(b"mutated config");
        let result = watcher
            .validate_policy_pack("test_policy", &mutated_hash)
            .await
            .unwrap();

        assert!(!result.valid);
        assert_eq!(result.status, ValidationStatus::Mismatch);
        assert!(watcher.is_quarantined());
        assert_eq!(watcher.violation_count(), 1);
    }

    #[tokio::test]
    async fn test_validate_missing_baseline() {
        let (watcher, _temp) = setup_test_watcher().await;

        let hash = B3Hash::hash(b"new policy config");
        let result = watcher
            .validate_policy_pack("unknown_policy", &hash)
            .await
            .unwrap();

        assert!(!result.valid);
        assert_eq!(result.status, ValidationStatus::Missing);
    }

    #[tokio::test]
    async fn test_clear_violations() {
        let (watcher, _temp) = setup_test_watcher().await;

        let baseline_hash = B3Hash::hash(b"original");
        watcher
            .register_baseline("policy1", &baseline_hash, None)
            .await
            .unwrap();

        let mutated_hash = B3Hash::hash(b"mutated");
        watcher
            .validate_policy_pack("policy1", &mutated_hash)
            .await
            .unwrap();

        assert!(watcher.is_quarantined());

        watcher.clear_violations("policy1").unwrap();
        assert!(!watcher.is_quarantined());
    }

    #[tokio::test]
    async fn test_load_cache() {
        let (watcher, _temp) = setup_test_watcher().await;

        let hash1 = B3Hash::hash(b"policy1 config");
        let hash2 = B3Hash::hash(b"policy2 config");

        watcher
            .register_baseline("policy1", &hash1, None)
            .await
            .unwrap();
        watcher
            .register_baseline("policy2", &hash2, None)
            .await
            .unwrap();

        // Clear cache
        {
            let mut cache = watcher.cache.write().unwrap();
            cache.clear();
        }

        // Reload from database
        watcher.load_cache().await.unwrap();

        let cache = watcher.cache.read().unwrap();
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("policy1"), Some(&hash1));
        assert_eq!(cache.get("policy2"), Some(&hash2));
    }
}

