//! Drift detection and comparison

use crate::metadata::DeviceFingerprint;
use adapteros_core::{B3Hash, Result};
use serde::{Deserialize, Serialize};

/// Drift severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftSeverity {
    /// No drift detected
    None,
    /// Informational drift (logged but not enforced)
    Info,
    /// Warning drift (may affect determinism)
    Warning,
    /// Critical drift (blocks inference)
    Critical,
}

impl DriftSeverity {
    /// Check if this severity should block execution
    pub fn is_blocking(&self) -> bool {
        matches!(self, DriftSeverity::Critical)
    }

    /// Get the maximum severity between two levels
    pub fn max(self, other: Self) -> Self {
        match (self, other) {
            (DriftSeverity::Critical, _) | (_, DriftSeverity::Critical) => DriftSeverity::Critical,
            (DriftSeverity::Warning, _) | (_, DriftSeverity::Warning) => DriftSeverity::Warning,
            (DriftSeverity::Info, _) | (_, DriftSeverity::Info) => DriftSeverity::Info,
            _ => DriftSeverity::None,
        }
    }
}

/// Drift report for a single field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDrift {
    pub field_name: String,
    pub baseline_value: String,
    pub current_value: String,
    pub severity: DriftSeverity,
    pub description: String,
}

/// Complete drift report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub baseline_hash: B3Hash,
    pub current_hash: B3Hash,
    pub drift_detected: bool,
    pub severity: DriftSeverity,
    pub field_drifts: Vec<FieldDrift>,
    pub timestamp: u64,
}

impl DriftReport {
    /// Create a new drift report with no drifts
    pub fn no_drift(baseline_hash: B3Hash, current_hash: B3Hash) -> Self {
        Self {
            baseline_hash,
            current_hash,
            drift_detected: false,
            severity: DriftSeverity::None,
            field_drifts: Vec::new(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Add a field drift
    pub fn add_drift(&mut self, drift: FieldDrift) {
        // Update overall severity if this drift is more severe
        if drift.severity as u8 > self.severity as u8 {
            self.severity = drift.severity;
        }
        self.drift_detected = true;
        self.field_drifts.push(drift);
    }

    /// Check if drift should block inference
    pub fn should_block(&self) -> bool {
        self.severity == DriftSeverity::Critical
    }

    /// Generate summary string
    pub fn summary(&self) -> String {
        if !self.drift_detected {
            return "No drift detected".to_string();
        }

        format!(
            "Drift detected: {:?} severity, {} field(s) changed",
            self.severity,
            self.field_drifts.len()
        )
    }

    /// Get detailed field-by-field comparison
    pub fn detailed_report(&self) -> String {
        let mut lines = vec![self.summary()];
        lines.push(format!("Baseline: {}", self.baseline_hash));
        lines.push(format!("Current:  {}", self.current_hash));
        lines.push(String::new());

        if self.drift_detected {
            lines.push("Field drifts:".to_string());
            for drift in &self.field_drifts {
                lines.push(format!(
                    "  - {} [{:?}]: {} -> {}",
                    drift.field_name, drift.severity, drift.baseline_value, drift.current_value
                ));
                if !drift.description.is_empty() {
                    lines.push(format!("    {}", drift.description));
                }
            }
        }

        lines.join("\n")
    }
}

use adapteros_core::DriftPolicy;

/// Drift evaluator
pub struct DriftEvaluator {
    policy: DriftPolicy,
}

impl DriftEvaluator {
    /// Create a new drift evaluator with default policy
    pub fn new() -> Self {
        Self {
            policy: DriftPolicy::default(),
        }
    }

    /// Create a new drift evaluator from policy
    pub fn from_policy(policy: &DriftPolicy) -> Self {
        Self {
            policy: policy.clone(),
        }
    }

    /// Compare two fingerprints and generate drift report
    pub fn compare(
        &self,
        baseline: &DeviceFingerprint,
        current: &DeviceFingerprint,
    ) -> Result<DriftReport> {
        let baseline_hash = baseline.compute_hash()?;
        let current_hash = current.compute_hash()?;

        // If hashes match, no drift
        if baseline_hash == current_hash {
            return Ok(DriftReport::no_drift(baseline_hash, current_hash));
        }

        let mut report = DriftReport::no_drift(baseline_hash, current_hash);

        // Check each field
        self.check_field(
            &mut report,
            "schema_version",
            &baseline.schema_version.to_string(),
            &current.schema_version.to_string(),
            DriftSeverity::Critical,
        );

        self.check_field(
            &mut report,
            "device_model",
            &baseline.device_model,
            &current.device_model,
            DriftSeverity::Critical,
        );

        self.check_field(
            &mut report,
            "soc_id",
            &baseline.soc_id,
            &current.soc_id,
            DriftSeverity::Critical,
        );

        self.check_field(
            &mut report,
            "gpu_pci_id",
            &baseline.gpu_pci_id,
            &current.gpu_pci_id,
            DriftSeverity::Critical,
        );

        self.check_field(
            &mut report,
            "os_version",
            &baseline.os_version,
            &current.os_version,
            DriftSeverity::Warning,
        );

        self.check_field(
            &mut report,
            "os_build",
            &baseline.os_build,
            &current.os_build,
            if self.policy.os_build_tolerance > 0 {
                DriftSeverity::Info
            } else {
                DriftSeverity::Warning
            },
        );

        self.check_field(
            &mut report,
            "metal_family",
            &baseline.metal_family,
            &current.metal_family,
            DriftSeverity::Critical,
        );

        self.check_field(
            &mut report,
            "gpu_driver_version",
            &baseline.gpu_driver_version,
            &current.gpu_driver_version,
            if self.policy.gpu_driver_tolerance > 0 {
                DriftSeverity::Info
            } else {
                DriftSeverity::Warning
            },
        );

        self.check_field(
            &mut report,
            "path_hash",
            &baseline.path_hash.to_string(),
            &current.path_hash.to_string(),
            DriftSeverity::Info,
        );

        self.check_field(
            &mut report,
            "env_hash",
            &baseline.env_hash.to_string(),
            &current.env_hash.to_string(),
            if self.policy.env_hash_tolerance > 0 {
                DriftSeverity::Info
            } else {
                DriftSeverity::Warning
            },
        );

        // Check CPU features
        if baseline.cpu_features != current.cpu_features {
            report.add_drift(FieldDrift {
                field_name: "cpu_features".to_string(),
                baseline_value: baseline.cpu_features.join(", "),
                current_value: current.cpu_features.join(", "),
                severity: DriftSeverity::Critical,
                description: "CPU features changed".to_string(),
            });
        }

        // Check firmware hash (if both available)
        if baseline.firmware_hash.is_some()
            && current.firmware_hash.is_some()
            && baseline.firmware_hash != current.firmware_hash
        {
            report.add_drift(FieldDrift {
                field_name: "firmware_hash".to_string(),
                baseline_value: baseline
                    .firmware_hash
                    .as_ref()
                    .map(|h| h.to_string())
                    .unwrap_or_default(),
                current_value: current
                    .firmware_hash
                    .as_ref()
                    .map(|h| h.to_string())
                    .unwrap_or_default(),
                severity: DriftSeverity::Warning,
                description: "Firmware version changed".to_string(),
            });
        }

        // Check boot version hash (if both available)
        if baseline.boot_version_hash.is_some()
            && current.boot_version_hash.is_some()
            && baseline.boot_version_hash != current.boot_version_hash
        {
            report.add_drift(FieldDrift {
                field_name: "boot_version_hash".to_string(),
                baseline_value: baseline
                    .boot_version_hash
                    .as_ref()
                    .map(|h| h.to_string())
                    .unwrap_or_default(),
                current_value: current
                    .boot_version_hash
                    .as_ref()
                    .map(|h| h.to_string())
                    .unwrap_or_default(),
                severity: DriftSeverity::Info,
                description: "Boot configuration changed".to_string(),
            });
        }

        // Apply policy overrides
        if !self.policy.allow_warnings && report.severity == DriftSeverity::Warning {
            report.severity = DriftSeverity::Critical;
        }

        Ok(report)
    }

    fn check_field(
        &self,
        report: &mut DriftReport,
        field_name: &str,
        baseline_value: &str,
        current_value: &str,
        severity: DriftSeverity,
    ) {
        if baseline_value != current_value {
            report.add_drift(FieldDrift {
                field_name: field_name.to_string(),
                baseline_value: baseline_value.to_string(),
                current_value: current_value.to_string(),
                severity,
                description: format!("Field '{}' changed", field_name),
            });
        }
    }
}

impl Default for DriftEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;

    #[test]
    fn test_drift_detection() {
        let baseline = DeviceFingerprint {
            schema_version: 1,
            device_model: "MacBookPro18,3".to_string(),
            soc_id: "Apple M1 Pro".to_string(),
            gpu_pci_id: "Apple7::12345".to_string(),
            os_version: "14.0".to_string(),
            os_build: "23A344".to_string(),
            metal_family: "Apple9".to_string(),
            gpu_driver_version: "14.0".to_string(),
            path_hash: B3Hash::hash(b"path"),
            env_hash: B3Hash::hash(b"env"),
            cpu_features: vec!["aarch64".to_string()],
            firmware_hash: None,
            boot_version_hash: None,
        };

        let current = baseline.clone();

        let evaluator = DriftEvaluator::new();

        let report = evaluator.compare(&baseline, &current).unwrap();
        assert!(!report.drift_detected);
        assert_eq!(report.severity, DriftSeverity::None);
    }

    #[test]
    fn test_drift_detection_os_build_change() {
        let baseline = DeviceFingerprint {
            schema_version: 1,
            device_model: "MacBookPro18,3".to_string(),
            soc_id: "Apple M1 Pro".to_string(),
            gpu_pci_id: "Apple7::12345".to_string(),
            os_version: "14.0".to_string(),
            os_build: "23A344".to_string(),
            metal_family: "Apple9".to_string(),
            gpu_driver_version: "14.0".to_string(),
            path_hash: B3Hash::hash(b"path"),
            env_hash: B3Hash::hash(b"env"),
            cpu_features: vec!["aarch64".to_string()],
            firmware_hash: None,
            boot_version_hash: None,
        };

        let mut current = baseline.clone();
        current.os_build = "23A345".to_string();

        let evaluator = DriftEvaluator::new();
        let report = evaluator.compare(&baseline, &current).unwrap();

        assert!(report.drift_detected);
        // os_build changes are Warning severity by default (allow_warnings=true)
        assert_eq!(report.severity, DriftSeverity::Warning);
        // Warning does not block (only Critical blocks)
        assert!(!report.should_block());
    }
}
