use anyhow::Result;
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct InvariantSpec {
    pub id: &'static str,
    pub description: &'static str,
    pub required_runtime_fields: &'static [&'static str],
    pub required_test_fields: &'static [&'static str],
    pub runtime_files: &'static [&'static str],
    pub test_files: &'static [&'static str],
}

#[derive(Debug, Serialize)]
pub struct InvariantReport {
    pub id: String,
    pub runtime_hits: Vec<String>,
    pub test_hits: Vec<String>,
    pub missing_runtime_fields: Vec<String>,
    pub missing_test_fields: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifierReport {
    pub invariants: Vec<InvariantReport>,
    pub failed: bool,
}

/// Default invariants V-INV-1..5.
pub fn default_invariants() -> Vec<InvariantSpec> {
    vec![
        InvariantSpec {
            id: "V-INV-1",
            description: "Worker health events include worker_id, tenant_id, previous_status, new_status, reason, timestamp",
            required_runtime_fields: &[
                "worker_id",
                "tenant_id",
                "previous_status",
                "new_status",
                "reason",
                "timestamp_us",
            ],
            required_test_fields: &["worker_id", "tenant_id", "reason"],
            runtime_files: &[
                "crates/adapteros-telemetry/src/observability.rs",
                "crates/adapteros-lora-worker/src/health.rs",
            ],
            test_files: &[
                "crates/adapteros-telemetry/src/observability.rs",
                "tests/integration_tests/telemetry_observability.rs",
            ],
        },
        InvariantSpec {
            id: "V-INV-2",
            description: "Routing decision events include request_id, tenant_id, worker_id, adapter_ids, determinism_mode, seed hash/summary",
            required_runtime_fields: &[
                "tenant_id",
                "request_id",
                "worker_id",
                "adapter_ids",
                "determinism_mode",
                "seed_hash",
            ],
            required_test_fields: &["adapter_ids", "worker_id", "determinism_mode"],
            runtime_files: &[
                "crates/adapteros-telemetry/src/observability.rs",
                "crates/adapteros-server-api/src/inference_core.rs",
            ],
            test_files: &[
                "crates/adapteros-telemetry/src/observability.rs",
                "tests/integration_tests/telemetry_observability.rs",
            ],
        },
        InvariantSpec {
            id: "V-INV-3",
            description: "Auth events include principal_id, tenant_id, flow_type, success/failure, error code",
            required_runtime_fields: &[
                "principal_id",
                "tenant_id",
                "flow_type",
                "success",
                "error_code",
            ],
            required_test_fields: &["flow_type", "success"],
            runtime_files: &[
                "crates/adapteros-telemetry/src/observability.rs",
                "crates/adapteros-server-api/src/handlers/auth_enhanced.rs",
                "crates/adapteros-db/src/auth_sessions.rs",
                "crates/adapteros-db/src/api_keys.rs",
            ],
            test_files: &[
                "crates/adapteros-telemetry/src/observability.rs",
            ],
        },
        InvariantSpec {
            id: "V-INV-4",
            description: "Single invariant list links runtime enforcement and tests",
            required_runtime_fields: &["default_invariants"],
            required_test_fields: &["run_with_specs"],
            runtime_files: &["crates/adapteros-telemetry-verifier/src/lib.rs"],
            test_files: &["crates/adapteros-telemetry-verifier/tests/verifier.rs"],
        },
        InvariantSpec {
            id: "V-INV-5",
            description: "Verifier fails when enforcement or tests are missing",
            required_runtime_fields: &["failed: bool"],
            required_test_fields: &["missing_runtime_fields", "missing_test_fields"],
            runtime_files: &["crates/adapteros-telemetry-verifier/src/lib.rs"],
            test_files: &["crates/adapteros-telemetry-verifier/tests/verifier.rs"],
        },
    ]
}

pub fn run_verifier(root: &Path) -> Result<VerifierReport> {
    run_with_specs(root, &default_invariants())
}

pub fn run_with_specs(root: &Path, specs: &[InvariantSpec]) -> Result<VerifierReport> {
    let mut reports = Vec::new();
    let mut failed = false;

    // Check for duplicate IDs
    let mut seen = HashSet::new();
    for spec in specs {
        if !seen.insert(spec.id) {
            failed = true;
        }
    }

    for spec in specs {
        let (runtime_hits, missing_runtime) =
            scan_fields(root, spec.runtime_files, spec.required_runtime_fields)?;
        let (test_hits, missing_tests) =
            scan_fields(root, spec.test_files, spec.required_test_fields)?;

        if !missing_runtime.is_empty() || !missing_tests.is_empty() {
            failed = true;
        }

        reports.push(InvariantReport {
            id: spec.id.to_string(),
            runtime_hits,
            test_hits,
            missing_runtime_fields: missing_runtime,
            missing_test_fields: missing_tests,
        });
    }

    Ok(VerifierReport {
        invariants: reports,
        failed,
    })
}

fn scan_fields(
    root: &Path,
    files: &[&str],
    required_fields: &[&str],
) -> Result<(Vec<String>, Vec<String>)> {
    let mut hits = Vec::new();
    let mut missing = Vec::new();

    for &field in required_fields {
        let field_re = Regex::new(&regex::escape(field))?;
        let mut found = false;
        for file in files {
            let path = root.join(file);
            if !path.exists() {
                continue;
            }
            let content = fs::read_to_string(&path)?;
            if field_re.is_match(&content) {
                hits.push(file.to_string());
                found = true;
                break;
            }
        }
        if !found {
            missing.push(field.to_string());
        }
    }

    Ok((hits, missing))
}

/// Derive repository root assuming crate is in `crates/adapteros-telemetry-verifier`.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}
