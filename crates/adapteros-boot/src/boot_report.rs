//! Boot report generation for logging and file output.
//!
//! The boot report captures key information about the boot process:
//! - Configuration hash (BLAKE3)
//! - Phase timings
//! - Enabled features
//! - Server binding info
//! - Key IDs (no key material)
//! - Build information
//!
//! ## Output Formats
//!
//! 1. **Single-line JSON log** at INFO level with `BOOT_REPORT` tag
//! 2. **File** at `var/run/boot_report.json` with 0600 permissions
//!
//! ## Security
//!
//! The boot report is designed to be safe for logs:
//! - No secrets or key material
//! - No raw environment variables
//! - No full paths that leak tenant structure
//! - Only key IDs (derived hashes), not actual keys

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::BootError;
use crate::phase::PhaseTiming;

/// Boot report for logging and file output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootReport {
    /// BLAKE3 hash of effective configuration (first 16 hex chars).
    pub config_hash: String,

    /// Configuration schema version.
    pub config_schema_version: String,

    /// Duration of each boot phase in milliseconds.
    pub boot_phase_durations_ms: HashMap<String, u64>,

    /// Total boot time in milliseconds.
    pub total_boot_time_ms: u64,

    /// Enabled feature flags.
    pub enabled_features: Vec<String>,

    /// Server bind address.
    pub bind_addr: String,

    /// Server port.
    pub port: u16,

    /// Auth key identifiers (no key material!).
    pub auth_key_kids: Vec<String>,

    /// Worker key identifiers (no key material!).
    pub worker_key_kids: Vec<String>,

    /// Build information.
    pub build: BuildInfo,

    /// Run ID for this server instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,

    /// Build identifier ({hash}-{timestamp}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,

    /// Timestamp of report generation (ISO 8601).
    pub generated_at: String,
}

/// Build information embedded in boot report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo {
    /// Git commit SHA (if available at build time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    /// Build timestamp (if available at build time).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_time: Option<String>,

    /// Crate version from Cargo.toml.
    pub version: String,

    /// Rust version used for compilation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc_version: Option<String>,

    /// Target triple (e.g., "aarch64-macos").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    /// Combined build identifier ({hash}-{timestamp}).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_id: Option<String>,
}

impl Default for BuildInfo {
    fn default() -> Self {
        use adapteros_core::version;

        Self {
            git_sha: Some(version::GIT_COMMIT_HASH.to_string()).filter(|s| s != "unknown"),
            build_time: Some(version::BUILD_TIMESTAMP.to_string()).filter(|s| s != "unknown"),
            version: version::VERSION.to_string(),
            rustc_version: Some(version::RUSTC_VERSION.to_string()).filter(|s| s != "unknown"),
            target: Some(format!("{}-{}", version::TARGET_ARCH, version::TARGET_OS)),
            build_id: Some(version::BUILD_ID.to_string()).filter(|s| s != "unknown"),
        }
    }
}

/// Builder for creating boot reports.
pub struct BootReportBuilder {
    config_hash: Option<String>,
    config_schema_version: String,
    phase_timings: Vec<PhaseTiming>,
    enabled_features: Vec<String>,
    bind_addr: String,
    port: u16,
    auth_key_kids: Vec<String>,
    worker_key_kids: Vec<String>,
    build: BuildInfo,
    run_id: Option<String>,
    build_id: Option<String>,
}

impl Default for BootReportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BootReportBuilder {
    /// Create a new boot report builder.
    pub fn new() -> Self {
        Self {
            config_hash: None,
            config_schema_version: "1.0".to_string(),
            phase_timings: Vec::new(),
            enabled_features: detect_enabled_features(),
            bind_addr: "0.0.0.0".to_string(),
            port: 8080,
            auth_key_kids: Vec::new(),
            worker_key_kids: Vec::new(),
            build: BuildInfo::default(),
            run_id: None,
            build_id: None,
        }
    }

    /// Set the configuration hash.
    pub fn config_hash(mut self, hash: impl Into<String>) -> Self {
        self.config_hash = Some(hash.into());
        self
    }

    /// Set the config hash from raw config bytes using BLAKE3.
    pub fn config_hash_from_bytes(mut self, config_bytes: &[u8]) -> Self {
        let hash = blake3::hash(config_bytes);
        self.config_hash = Some(hash.to_hex()[..16].to_string());
        self
    }

    /// Set the configuration schema version.
    pub fn config_schema_version(mut self, version: impl Into<String>) -> Self {
        self.config_schema_version = version.into();
        self
    }

    /// Set the phase timings.
    pub fn phase_timings(mut self, timings: Vec<PhaseTiming>) -> Self {
        self.phase_timings = timings;
        self
    }

    /// Set the server bind address.
    pub fn bind_addr(mut self, addr: impl Into<String>) -> Self {
        self.bind_addr = addr.into();
        self
    }

    /// Set the server port.
    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Set the enabled features list.
    ///
    /// This overrides the auto-detected features with a custom list.
    /// Useful for setting workspace-level feature flags.
    pub fn enabled_features(mut self, features: Vec<String>) -> Self {
        self.enabled_features = features;
        self
    }

    /// Add an enabled feature to the list.
    pub fn add_enabled_feature(mut self, feature: impl Into<String>) -> Self {
        self.enabled_features.push(feature.into());
        self
    }

    /// Add an auth key ID.
    pub fn add_auth_key_kid(mut self, kid: impl Into<String>) -> Self {
        self.auth_key_kids.push(kid.into());
        self
    }

    /// Set all auth key IDs.
    pub fn auth_key_kids(mut self, kids: Vec<String>) -> Self {
        self.auth_key_kids = kids;
        self
    }

    /// Add a worker key ID.
    pub fn add_worker_key_kid(mut self, kid: impl Into<String>) -> Self {
        self.worker_key_kids.push(kid.into());
        self
    }

    /// Set all worker key IDs.
    pub fn worker_key_kids(mut self, kids: Vec<String>) -> Self {
        self.worker_key_kids = kids;
        self
    }

    /// Set custom build info.
    pub fn build_info(mut self, build: BuildInfo) -> Self {
        self.build = build;
        self
    }

    /// Set the run ID.
    pub fn run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Set the build ID.
    pub fn build_id_field(mut self, build_id: impl Into<String>) -> Self {
        self.build_id = Some(build_id.into());
        self
    }

    /// Build the boot report.
    pub fn build(self) -> BootReport {
        let mut phase_durations = HashMap::new();
        let mut total_ms = 0u64;

        for timing in &self.phase_timings {
            if let Some(duration_ms) = timing.duration_ms {
                phase_durations.insert(timing.phase.as_str().to_string(), duration_ms);
                total_ms += duration_ms;
            }
        }

        BootReport {
            config_hash: self.config_hash.unwrap_or_else(|| "unknown".to_string()),
            config_schema_version: self.config_schema_version,
            boot_phase_durations_ms: phase_durations,
            total_boot_time_ms: total_ms,
            enabled_features: self.enabled_features,
            bind_addr: self.bind_addr,
            port: self.port,
            auth_key_kids: self.auth_key_kids,
            worker_key_kids: self.worker_key_kids,
            build: self.build,
            run_id: self.run_id,
            build_id: self.build_id,
            generated_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

impl BootReport {
    /// Create a new boot report builder.
    pub fn builder() -> BootReportBuilder {
        BootReportBuilder::new()
    }

    /// Write boot report to file with 0600 permissions.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to write the report (e.g., "var/run/boot_report.json")
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written or permissions cannot be set.
    pub fn write_to_file(&self, path: &str) -> Result<(), BootError> {
        use std::fs;
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        use std::time::{SystemTime, UNIX_EPOCH};

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| BootError::ReportWrite(e.to_string()))?;

        let path = std::path::Path::new(path);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| BootError::ReportWrite(e.to_string()))?;
        }

        // Generate unique temp file name
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_path = path.with_extension(format!("json.tmp.{}", nanos));

        // Write to temp file with restricted permissions (0600)
        #[cfg(unix)]
        {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temp_path)
                .map_err(|e| BootError::ReportWrite(e.to_string()))?;
            file.write_all(json.as_bytes())
                .map_err(|e| BootError::ReportWrite(e.to_string()))?;
            file.sync_all()
                .map_err(|e| BootError::ReportWrite(e.to_string()))?;
        }

        #[cfg(not(unix))]
        {
            fs::write(&temp_path, &json).map_err(|e| BootError::ReportWrite(e.to_string()))?;
        }

        // Atomic rename
        fs::rename(&temp_path, path).map_err(|e| {
            // Clean up temp file on failure
            let _ = fs::remove_file(&temp_path);
            BootError::ReportWrite(e.to_string())
        })?;

        Ok(())
    }

    /// Emit boot report as a single-line JSON log event.
    ///
    /// Uses tracing at INFO level with a `BOOT_REPORT` message.
    pub fn emit_log(&self) {
        match serde_json::to_string(self) {
            Ok(json) => {
                tracing::info!(
                    boot_report = %json,
                    config_hash = %self.config_hash,
                    total_boot_time_ms = self.total_boot_time_ms,
                    "BOOT_REPORT"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to serialize boot report");
            }
        }
    }

    /// Write to file and emit log in one call.
    pub fn write_and_emit(&self, path: &str) -> Result<(), BootError> {
        self.write_to_file(path)?;
        self.emit_log();
        Ok(())
    }
}

/// Detect enabled features at compile time.
///
/// Note: This returns a minimal set of detectable features.
/// Callers can use `BootReportBuilder::enabled_features()` to set
/// the complete feature list based on their own compile-time flags.
#[allow(clippy::vec_init_then_push)]
fn detect_enabled_features() -> Vec<String> {
    let mut features = Vec::new();

    // Debug build indicator
    #[cfg(debug_assertions)]
    features.push("debug".to_string());

    // Release build indicator
    #[cfg(not(debug_assertions))]
    features.push("release".to_string());

    // Target OS
    #[cfg(target_os = "macos")]
    features.push("macos".to_string());

    #[cfg(target_os = "linux")]
    features.push("linux".to_string());

    // Target architecture
    #[cfg(target_arch = "aarch64")]
    features.push("aarch64".to_string());

    #[cfg(target_arch = "x86_64")]
    features.push("x86_64".to_string());

    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phase::BootPhase;

    #[test]
    fn test_boot_report_builder() {
        let report = BootReport::builder()
            .config_hash("abc123def456")
            .bind_addr("127.0.0.1")
            .port(8080)
            .add_auth_key_kid("jwt-abc123")
            .add_worker_key_kid("worker-def456")
            .build();

        assert_eq!(report.config_hash, "abc123def456");
        assert_eq!(report.bind_addr, "127.0.0.1");
        assert_eq!(report.port, 8080);
        assert_eq!(report.auth_key_kids, vec!["jwt-abc123"]);
        assert_eq!(report.worker_key_kids, vec!["worker-def456"]);
    }

    #[test]
    fn test_config_hash_from_bytes() {
        let config = r#"{"server": {"port": 8080}}"#;
        let report = BootReport::builder()
            .config_hash_from_bytes(config.as_bytes())
            .build();

        // Hash should be 16 hex characters
        assert_eq!(report.config_hash.len(), 16);
        assert!(report.config_hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_phase_timings_aggregation() {
        let mut timing1 = crate::phase::PhaseTiming::start(BootPhase::Starting);
        timing1.duration_ms = Some(100);

        let mut timing2 = crate::phase::PhaseTiming::start(BootPhase::DbConnecting);
        timing2.duration_ms = Some(200);

        let report = BootReport::builder()
            .phase_timings(vec![timing1, timing2])
            .build();

        assert_eq!(report.total_boot_time_ms, 300);
        assert_eq!(report.boot_phase_durations_ms.get("starting"), Some(&100));
        assert_eq!(
            report.boot_phase_durations_ms.get("db-connecting"),
            Some(&200)
        );
    }

    #[test]
    fn test_serialization() {
        let report = BootReport::builder()
            .config_hash("test")
            .bind_addr("0.0.0.0")
            .port(8080)
            .build();

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("config_hash"));
        assert!(json.contains("generated_at"));

        // Ensure it can be deserialized back
        let parsed: BootReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.config_hash, "test");
    }
}
