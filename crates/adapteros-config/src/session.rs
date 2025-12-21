// Copyright JKCA | 2025 James KC Auchterlonie
//
// Configuration session management: snapshots and drift detection

use crate::effective::EffectiveConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sensitive configuration fields that should be redacted in snapshots
const REDACTED_FIELDS: &[&str] = &[
    "AOS_SECURITY_JWT_SECRET",
    "AOS_SIGNING_KEY",
    "HF_TOKEN",
    "AOS_DATABASE_PASSWORD",
    "AOS_KMS_ACCESS_KEY",
    "AOS_KEYCHAIN_FALLBACK",
];

/// Severity level for configuration drift
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfigDriftSeverity {
    /// Informational change, no action required
    Info,
    /// Warning, may affect behavior
    Warning,
    /// Critical change, requires immediate attention
    Critical,
}

/// A single field that has drifted between snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFieldDrift {
    /// Configuration key
    pub key: String,
    /// Previous value (redacted if sensitive)
    pub old_value: String,
    /// New value (redacted if sensitive)
    pub new_value: String,
    /// Source of previous value
    pub old_source: Option<String>,
    /// Source of new value
    pub new_source: String,
    /// Severity of this drift
    pub severity: ConfigDriftSeverity,
}

/// A single configuration entry in a snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshotEntry {
    /// Value (masked as "***REDACTED***" if sensitive)
    pub value: String,
    /// Source of this value (cli, env, toml, default)
    pub source: String,
    /// Whether this field was redacted
    pub redacted: bool,
}

/// Point-in-time snapshot of configuration state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    /// Map of configuration key to entry
    pub entries: HashMap<String, ConfigSnapshotEntry>,
    /// Hash of the snapshot for quick comparison
    pub hash: String,
    /// ISO 8601 timestamp of snapshot creation
    pub created_at: String,
}

/// Report of configuration drift between two snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigDriftReport {
    /// Hash of previous snapshot
    pub previous_hash: String,
    /// Hash of current snapshot
    pub current_hash: String,
    /// Whether any drift was detected
    pub drift_detected: bool,
    /// Number of fields that drifted
    pub field_count: usize,
    /// Details of each drifted field
    pub fields: Vec<ConfigFieldDrift>,
    /// ISO 8601 timestamp of drift detection
    pub timestamp: String,
}

impl ConfigSnapshot {
    /// Create a snapshot from the current effective configuration
    pub fn from_effective_config(cfg: &EffectiveConfig) -> Self {
        let mut entries = HashMap::new();

        // Iterate all configuration values
        for (key, value) in cfg.all_values() {
            let is_redacted = REDACTED_FIELDS.contains(&key.as_str());
            let source = cfg
                .get_source(key)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());

            let entry = ConfigSnapshotEntry {
                value: if is_redacted {
                    "***REDACTED***".to_string()
                } else {
                    value.clone()
                },
                source,
                redacted: is_redacted,
            };

            entries.insert(key.clone(), entry);
        }

        // Generate hash of the snapshot
        let hash = Self::compute_hash(&entries);

        // Create ISO 8601 timestamp
        let created_at = chrono::Utc::now().to_rfc3339();

        Self {
            entries,
            hash,
            created_at,
        }
    }

    /// Compare this snapshot with a previous one to detect drift
    pub fn diff(&self, previous: &ConfigSnapshot) -> ConfigDriftReport {
        let mut fields = Vec::new();

        // Check for changed and new fields
        for (key, current_entry) in &self.entries {
            if let Some(prev_entry) = previous.entries.get(key) {
                // Field exists in both - check if value or source changed
                if current_entry.value != prev_entry.value
                    || current_entry.source != prev_entry.source
                {
                    fields.push(ConfigFieldDrift {
                        key: key.clone(),
                        old_value: prev_entry.value.clone(),
                        new_value: current_entry.value.clone(),
                        old_source: Some(prev_entry.source.clone()),
                        new_source: current_entry.source.clone(),
                        severity: Self::classify_severity(key),
                    });
                }
            } else {
                // New field
                fields.push(ConfigFieldDrift {
                    key: key.clone(),
                    old_value: String::new(),
                    new_value: current_entry.value.clone(),
                    old_source: None,
                    new_source: current_entry.source.clone(),
                    severity: Self::classify_severity(key),
                });
            }
        }

        // Check for removed fields
        for (key, prev_entry) in &previous.entries {
            if !self.entries.contains_key(key) {
                fields.push(ConfigFieldDrift {
                    key: key.clone(),
                    old_value: prev_entry.value.clone(),
                    new_value: String::new(),
                    old_source: Some(prev_entry.source.clone()),
                    new_source: "removed".to_string(),
                    severity: Self::classify_severity(key),
                });
            }
        }

        let drift_detected = !fields.is_empty();
        let field_count = fields.len();
        let timestamp = chrono::Utc::now().to_rfc3339();

        ConfigDriftReport {
            previous_hash: previous.hash.clone(),
            current_hash: self.hash.clone(),
            drift_detected,
            field_count,
            fields,
            timestamp,
        }
    }

    /// Compute a hash of the snapshot entries for quick comparison
    fn compute_hash(entries: &HashMap<String, ConfigSnapshotEntry>) -> String {
        use std::collections::BTreeMap;

        // Sort entries for deterministic hashing
        let sorted: BTreeMap<_, _> = entries.iter().collect();

        // Create a canonical string representation
        let mut canonical = String::new();
        for (key, entry) in sorted {
            canonical.push_str(key);
            canonical.push('=');
            canonical.push_str(&entry.value);
            canonical.push('@');
            canonical.push_str(&entry.source);
            canonical.push('\n');
        }

        // Use BLAKE3 for fast, secure hashing
        let hash = blake3::hash(canonical.as_bytes());
        hash.to_hex().to_string()
    }

    /// Classify the severity of a configuration change based on key name
    fn classify_severity(key: &str) -> ConfigDriftSeverity {
        // Critical: Security-related and production mode flags
        if key.starts_with("AOS_SECURITY_")
            || key.starts_with("AOS_SIGNING_")
            || key.contains("_PRODUCTION_MODE")
        {
            return ConfigDriftSeverity::Critical;
        }

        // Warning: Model, backend, and database configuration
        if key.starts_with("AOS_MODEL_")
            || key.starts_with("AOS_BACKEND_")
            || key.starts_with("AOS_DATABASE_")
        {
            return ConfigDriftSeverity::Warning;
        }

        // Info: Everything else
        ConfigDriftSeverity::Info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redacted_fields() {
        // Verify REDACTED_FIELDS constant contains expected sensitive keys
        assert!(REDACTED_FIELDS.contains(&"AOS_SECURITY_JWT_SECRET"));
        assert!(REDACTED_FIELDS.contains(&"HF_TOKEN"));
    }

    #[test]
    fn test_severity_classification() {
        assert_eq!(
            ConfigSnapshot::classify_severity("AOS_SECURITY_JWT_SECRET"),
            ConfigDriftSeverity::Critical
        );
        assert_eq!(
            ConfigSnapshot::classify_severity("AOS_MODEL_PATH"),
            ConfigDriftSeverity::Warning
        );
        assert_eq!(
            ConfigSnapshot::classify_severity("AOS_LOG_LEVEL"),
            ConfigDriftSeverity::Info
        );
    }

    #[test]
    fn test_compute_hash_deterministic() {
        let mut entries1 = HashMap::new();
        entries1.insert(
            "KEY_A".to_string(),
            ConfigSnapshotEntry {
                value: "value1".to_string(),
                source: "env".to_string(),
                redacted: false,
            },
        );
        entries1.insert(
            "KEY_B".to_string(),
            ConfigSnapshotEntry {
                value: "value2".to_string(),
                source: "cli".to_string(),
                redacted: false,
            },
        );

        let mut entries2 = HashMap::new();
        entries2.insert(
            "KEY_B".to_string(),
            ConfigSnapshotEntry {
                value: "value2".to_string(),
                source: "cli".to_string(),
                redacted: false,
            },
        );
        entries2.insert(
            "KEY_A".to_string(),
            ConfigSnapshotEntry {
                value: "value1".to_string(),
                source: "env".to_string(),
                redacted: false,
            },
        );

        // Same entries in different order should produce same hash
        let hash1 = ConfigSnapshot::compute_hash(&entries1);
        let hash2 = ConfigSnapshot::compute_hash(&entries2);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_drift_detection() {
        let mut entries_old = HashMap::new();
        entries_old.insert(
            "KEY_A".to_string(),
            ConfigSnapshotEntry {
                value: "old_value".to_string(),
                source: "env".to_string(),
                redacted: false,
            },
        );

        let mut entries_new = HashMap::new();
        entries_new.insert(
            "KEY_A".to_string(),
            ConfigSnapshotEntry {
                value: "new_value".to_string(),
                source: "cli".to_string(),
                redacted: false,
            },
        );

        let snapshot_old = ConfigSnapshot {
            entries: entries_old,
            hash: "old_hash".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let snapshot_new = ConfigSnapshot {
            entries: entries_new,
            hash: "new_hash".to_string(),
            created_at: "2025-01-02T00:00:00Z".to_string(),
        };

        let report = snapshot_new.diff(&snapshot_old);

        assert!(report.drift_detected);
        assert_eq!(report.field_count, 1);
        assert_eq!(report.fields[0].key, "KEY_A");
        assert_eq!(report.fields[0].old_value, "old_value");
        assert_eq!(report.fields[0].new_value, "new_value");
    }
}
