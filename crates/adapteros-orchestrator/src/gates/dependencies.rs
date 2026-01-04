//! Runtime dependency checks and fallback path management for gates
//!
//! Provides centralized dependency resolution with graceful degradation when
//! required paths or tools are unavailable.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, warn};

/// Gate-specific dependency requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateDependencies {
    /// Gate identifier
    pub gate_id: String,
    /// Required paths that must exist
    pub required_paths: Vec<String>,
    /// Optional paths with fallbacks
    pub optional_paths: Vec<(String, Vec<String>)>,
    /// Required CLI tools (e.g., "cargo", "cargo-audit")
    pub required_tools: Vec<String>,
    /// Gate severity: "critical" (blocks promotion), "warning" (logs but continues)
    pub severity: GateSeverity,
}

/// Gate severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateSeverity {
    Critical,
    Warning,
}

/// Dependency check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyCheckResult {
    /// Gate ID
    pub gate_id: String,
    /// Whether all required dependencies are available
    pub all_available: bool,
    /// Status of each required path
    pub required_paths: HashMap<String, PathStatus>,
    /// Status of each optional path (resolved to a working path if possible)
    pub optional_paths: HashMap<String, PathResolution>,
    /// Status of each required tool
    pub required_tools: HashMap<String, ToolStatus>,
    /// Overall degradation level (0 = no issues, 1 = partial degradation, 2 = severe)
    pub degradation_level: u8,
    /// Detailed messages for operators
    pub messages: Vec<String>,
}

/// Status of a required path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathStatus {
    pub path: String,
    pub exists: bool,
    pub readable: bool,
}

/// Resolution status of an optional path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathResolution {
    pub primary_path: String,
    pub resolved_path: Option<String>,
    pub is_fallback: bool,
}

/// Status of a required tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool: String,
    pub available: bool,
    pub version: Option<String>,
}

impl DependencyCheckResult {
    /// Check if all critical dependencies are met
    pub fn critical_met(&self) -> bool {
        self.all_available
            && self.degradation_level == 0
            && self.required_paths.values().all(|p| p.exists && p.readable)
            && self.required_tools.values().all(|t| t.available)
    }

    /// Get resolved path for optional dependency, or None
    pub fn get_resolved_path(&self, key: &str) -> Option<String> {
        self.optional_paths
            .get(key)
            .and_then(|r| r.resolved_path.clone())
    }
}

/// Dependency checker for gates
pub struct DependencyChecker {
    definitions: HashMap<String, GateDependencies>,
}

impl DependencyChecker {
    /// Create a new dependency checker with standard gate definitions
    pub fn new() -> Self {
        let mut definitions = HashMap::new();

        // Determinism Gate: needs replay bundles
        definitions.insert(
            "determinism".to_string(),
            GateDependencies {
                gate_id: "determinism".to_string(),
                required_paths: vec!["/srv/aos/bundles".to_string()],
                optional_paths: vec![(
                    "replay_bundle".to_string(),
                    vec![
                        "var/bundles".to_string(),
                        "bundles".to_string(),
                        "target/bundles".to_string(),
                    ],
                )],
                required_tools: vec![],
                severity: GateSeverity::Critical,
            },
        );

        // Security Gate: needs cargo tools
        definitions.insert(
            "security".to_string(),
            GateDependencies {
                gate_id: "security".to_string(),
                required_paths: vec!["deny.toml".to_string()],
                optional_paths: vec![],
                required_tools: vec!["cargo".to_string()],
                severity: GateSeverity::Critical,
            },
        );

        // Metallib Gate: needs manifest and metallib files
        definitions.insert(
            "metallib".to_string(),
            GateDependencies {
                gate_id: "metallib".to_string(),
                required_paths: vec![
                    "crates/adapteros-lora-kernel-mtl/shaders/aos_kernels.metallib".to_string(),
                ],
                optional_paths: vec![(
                    "manifests_dir".to_string(),
                    vec!["manifests".to_string(), "target/manifests".to_string()],
                )],
                required_tools: vec![],
                severity: GateSeverity::Critical,
            },
        );

        // Telemetry Gate: needs database and bundle directories
        definitions.insert(
            "telemetry".to_string(),
            GateDependencies {
                gate_id: "telemetry".to_string(),
                required_paths: vec![],
                optional_paths: vec![
                    (
                        "telemetry_dir".to_string(),
                        vec![
                            "var/telemetry".to_string(),
                            ".telemetry".to_string(),
                            "/var/aos/telemetry".to_string(),
                        ],
                    ),
                    (
                        "bundles_dir".to_string(),
                        vec![
                            "/srv/aos/bundles".to_string(),
                            "var/bundles".to_string(),
                            "bundles".to_string(),
                        ],
                    ),
                ],
                required_tools: vec![],
                severity: GateSeverity::Warning,
            },
        );

        // Metrics Gate: needs database
        definitions.insert(
            "metrics".to_string(),
            GateDependencies {
                gate_id: "metrics".to_string(),
                required_paths: vec![],
                optional_paths: vec![],
                required_tools: vec![],
                severity: GateSeverity::Warning,
            },
        );

        // Performance Gate: needs database
        definitions.insert(
            "performance".to_string(),
            GateDependencies {
                gate_id: "performance".to_string(),
                required_paths: vec![],
                optional_paths: vec![],
                required_tools: vec![],
                severity: GateSeverity::Warning,
            },
        );

        // SBOM Gate: needs SBOM file and optional signature
        definitions.insert(
            "sbom".to_string(),
            GateDependencies {
                gate_id: "sbom".to_string(),
                required_paths: vec!["target/sbom.spdx.json".to_string()],
                optional_paths: vec![(
                    "sbom_signature".to_string(),
                    vec!["target/sbom.spdx.json.sig".to_string()],
                )],
                required_tools: vec![],
                severity: GateSeverity::Warning,
            },
        );

        Self { definitions }
    }

    /// Check dependencies for a specific gate
    pub fn check_gate(&self, gate_id: &str) -> Result<DependencyCheckResult> {
        let deps = self
            .definitions
            .get(gate_id)
            .context(format!("Unknown gate: {}", gate_id))?;

        let mut result = DependencyCheckResult {
            gate_id: gate_id.to_string(),
            all_available: true,
            required_paths: HashMap::new(),
            optional_paths: HashMap::new(),
            required_tools: HashMap::new(),
            degradation_level: 0,
            messages: Vec::new(),
        };

        // Check required paths
        for path_str in &deps.required_paths {
            let path = Path::new(path_str);
            let exists = path.exists();
            let readable = exists && (path.is_file() || path.is_dir());

            if !exists || !readable {
                result.all_available = false;
                if deps.severity == GateSeverity::Critical {
                    result.degradation_level = 2;
                } else if result.degradation_level < 1 {
                    result.degradation_level = 1;
                }
                result
                    .messages
                    .push(format!("Required path not accessible: {}", path_str));
            }

            result.required_paths.insert(
                path_str.clone(),
                PathStatus {
                    path: path_str.clone(),
                    exists,
                    readable,
                },
            );
        }

        // Check optional paths with fallbacks
        for (key, fallbacks) in &deps.optional_paths {
            let mut resolved = None;
            let mut is_fallback = false;

            for fallback in fallbacks {
                let path = Path::new(fallback);
                if path.exists() && (path.is_file() || path.is_dir()) {
                    resolved = Some(fallback.clone());
                    is_fallback = fallback != &fallbacks[0];
                    if is_fallback {
                        result
                            .messages
                            .push(format!("Using fallback path for '{}': {}", key, fallback));
                        if result.degradation_level < 1 {
                            result.degradation_level = 1;
                        }
                    }
                    break;
                }
            }

            if resolved.is_none() {
                result.messages.push(format!(
                    "Optional dependency '{}' not found in any fallback path: {:?}",
                    key, fallbacks
                ));
                if result.degradation_level < 1 {
                    result.degradation_level = 1;
                }
            }

            result.optional_paths.insert(
                key.clone(),
                PathResolution {
                    primary_path: fallbacks.first().cloned().unwrap_or_default(),
                    resolved_path: resolved,
                    is_fallback,
                },
            );
        }

        // Check required tools
        for tool in &deps.required_tools {
            let available = check_tool_availability(tool);
            let version = if available {
                get_tool_version(tool)
            } else {
                None
            };

            if !available {
                result.all_available = false;
                if deps.severity == GateSeverity::Critical {
                    result.degradation_level = 2;
                } else if result.degradation_level < 1 {
                    result.degradation_level = 1;
                }
                result
                    .messages
                    .push(format!("Required tool not available: {}", tool));
            }

            result.required_tools.insert(
                tool.clone(),
                ToolStatus {
                    tool: tool.clone(),
                    available,
                    version,
                },
            );
        }

        // Log results
        if result.all_available {
            debug!(gate = gate_id, "All dependencies available");
        } else {
            match result.degradation_level {
                2 => warn!(gate = gate_id, "Critical dependencies missing"),
                1 => warn!(gate = gate_id, "Some optional dependencies missing"),
                _ => {}
            }
        }

        Ok(result)
    }

    /// Check multiple gates at once
    pub fn check_gates(&self, gate_ids: &[&str]) -> Result<Vec<DependencyCheckResult>> {
        gate_ids.iter().map(|id| self.check_gate(id)).collect()
    }

    /// Get all registered gate IDs
    pub fn list_gates(&self) -> Vec<String> {
        self.definitions.keys().cloned().collect()
    }

    /// Get gate dependencies by ID
    pub fn get_definition(&self, gate_id: &str) -> Option<&GateDependencies> {
        self.definitions.get(gate_id)
    }
}

impl Default for DependencyChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a tool is available on the system
fn check_tool_availability(tool: &str) -> bool {
    // For cargo, check if it's in PATH
    let status = std::process::Command::new("which").arg(tool).output();

    matches!(status, Ok(output) if output.status.success())
}

/// Get the version string for a tool
fn get_tool_version(tool: &str) -> Option<String> {
    std::process::Command::new(tool)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_checker_creation() {
        let checker = DependencyChecker::new();
        assert!(!checker.list_gates().is_empty());
        assert!(checker.list_gates().contains(&"determinism".to_string()));
    }

    #[test]
    fn test_check_nonexistent_gate() {
        let checker = DependencyChecker::new();
        let result = checker.check_gate("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_path_status_creation() {
        let status = PathStatus {
            path: "/test/path".to_string(),
            exists: false,
            readable: false,
        };
        assert!(!status.exists);
    }

    #[test]
    fn test_degradation_level() {
        let mut result = DependencyCheckResult {
            gate_id: "test".to_string(),
            all_available: false,
            required_paths: HashMap::new(),
            optional_paths: HashMap::new(),
            required_tools: HashMap::new(),
            degradation_level: 0,
            messages: vec![],
        };

        assert_eq!(result.degradation_level, 0);
        assert!(!result.critical_met());

        result.degradation_level = 2;
        assert!(!result.critical_met());
    }

    #[test]
    fn test_get_tool_version_for_existing_tool() {
        // cargo should be available in the test environment
        let version = get_tool_version("cargo");
        assert!(version.is_some(), "cargo --version should return output");
        let v = version.unwrap();
        assert!(v.contains("cargo"), "version should mention cargo");
    }

    #[test]
    fn test_get_tool_version_for_missing_tool() {
        let version = get_tool_version("definitely-not-a-real-tool-12345");
        assert!(version.is_none(), "missing tool should return None");
    }

    #[test]
    fn test_security_gate_includes_tool_version() {
        let checker = DependencyChecker::new();
        let result = checker.check_gate("security").unwrap();
        // Security gate requires cargo
        if let Some(cargo_status) = result.required_tools.get("cargo") {
            if cargo_status.available {
                assert!(
                    cargo_status.version.is_some(),
                    "available cargo should have version"
                );
            }
        }
    }
}
