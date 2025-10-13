//! Agent E: Testing/Deployment/Compliance checks

use super::{Check, Section, VerifyAgentsArgs};
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::process::Command;

pub async fn run(_args: &VerifyAgentsArgs) -> Result<Section> {
    let mut section = Section::new("Agent E - Testing/Deployment/Compliance");

    // 1. Workflows
    section.add_check(check_workflows());

    // 2. Coverage gate
    section.add_check(check_coverage());

    // 3. Fuzzing
    section.add_check(check_fuzzing());

    // 4. Air-gap tests
    section.add_check(check_airgap());

    // 5. SBOM generation
    section.add_check(check_sbom());

    // 6. Backup/GC scripts
    section.add_check(check_backup_gc());

    // 7. Orchestrator
    section.add_check(check_orchestrator());

    // 8. Release checklist
    section.add_check(check_release_checklist());

    Ok(section)
}

fn check_workflows() -> Check {
    let workflows_dir = Path::new(".github/workflows");
    if !workflows_dir.exists() {
        return Check::fail("Workflows", vec![], ".github/workflows not found");
    }

    let required = ["coverage.yml", "airgap.yml", "release.yml"];
    let mut found = Vec::new();
    let mut missing = Vec::new();

    for workflow in required {
        let path = workflows_dir.join(workflow);
        if path.exists() {
            found.push(workflow.to_string());
        } else {
            missing.push(workflow.to_string());
        }
    }

    if missing.is_empty() {
        Check::pass(
            "Workflows",
            vec![
                format!("All required workflows found: {}", found.join(", ")),
            ],
        )
    } else {
        Check::fail(
            "Workflows",
            vec![
                format!("Found: {}", found.join(", ")),
                format!("Missing: {}", missing.join(", ")),
            ],
            "Not all required workflows present",
        )
    }
}

fn check_coverage() -> Check {
    // Check if llvm-cov is available
    let llvm_cov_check = Command::new("cargo")
        .args(["llvm-cov", "--version"])
        .output();

    if llvm_cov_check.is_err() {
        return Check::skip(
            "Coverage gate",
            "cargo-llvm-cov not installed (install with: cargo install cargo-llvm-cov)",
        );
    }

    // Check if coverage workflow exists
    if !Path::new(".github/workflows/coverage.yml").exists() {
        return Check::fail(
            "Coverage gate",
            vec![],
            "coverage.yml workflow not found",
        );
    }

    Check::pass(
        "Coverage gate",
        vec![
            "cargo-llvm-cov is available".to_string(),
            "coverage.yml workflow exists".to_string(),
        ],
    )
}

fn check_fuzzing() -> Check {
    let fuzz_dir = Path::new("fuzz");
    if !fuzz_dir.exists() {
        return Check::fail("Fuzzing", vec![], "fuzz/ directory not found");
    }

    let fuzz_targets_dir = fuzz_dir.join("fuzz_targets");
    if !fuzz_targets_dir.exists() {
        return Check::fail("Fuzzing", vec![], "fuzz/fuzz_targets not found");
    }

    // List fuzz targets
    let entries = match fs::read_dir(&fuzz_targets_dir) {
        Ok(e) => e,
        Err(e) => {
            return Check::fail(
                "Fuzzing",
                vec![],
                format!("Failed to read fuzz_targets: {}", e),
            )
        }
    };

    let targets: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    let required = ["manifest_parse.rs", "sbom_parse.rs", "policy_parse.rs"];
    let mut found = Vec::new();
    let mut missing = Vec::new();

    for target in required {
        if targets.contains(&target.to_string()) {
            found.push(target.to_string());
        } else {
            missing.push(target.to_string());
        }
    }

    if missing.is_empty() {
        Check::pass(
            "Fuzzing",
            vec![
                format!("All required fuzz targets found: {}", found.join(", ")),
                format!("Total fuzz targets: {}", targets.len()),
            ],
        )
    } else {
        Check::fail(
            "Fuzzing",
            vec![
                format!("Found: {}", found.join(", ")),
                format!("Missing: {}", missing.join(", ")),
            ],
            "Not all required fuzz targets present",
        )
    }
}

fn check_airgap() -> Check {
    // Check for airgap workflow
    if !Path::new(".github/workflows/airgap.yml").exists() {
        return Check::fail("Air-gap tests", vec![], "airgap.yml workflow not found");
    }

    // Check for egress tests
    let tests_dir = Path::new("tests");
    if tests_dir.exists() {
        let mut has_egress_test = false;
        if let Ok(entries) = fs::read_dir(tests_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if content.contains("egress") || content.contains("network") {
                        has_egress_test = true;
                        break;
                    }
                }
            }
        }

        if has_egress_test {
            Check::pass(
                "Air-gap tests",
                vec![
                    "airgap.yml workflow exists".to_string(),
                    "Egress test code found".to_string(),
                ],
            )
        } else {
            Check::skip(
                "Air-gap tests",
                "airgap.yml exists but no egress tests found in tests/",
            )
        }
    } else {
        Check::skip("Air-gap tests", "tests/ directory not found")
    }
}

fn check_sbom() -> Check {
    // Check if SBOM generation is implemented
    let sbom_module = Path::new("xtask/src/sbom.rs");
    if !sbom_module.exists() {
        return Check::fail("SBOM generation", vec![], "xtask/src/sbom.rs not found");
    }

    // Check if target/sbom.spdx.json exists
    let sbom_file = Path::new("target/sbom.spdx.json");
    if sbom_file.exists() {
        // Validate it's valid JSON
        match fs::read_to_string(sbom_file) {
            Ok(content) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let has_packages = json.get("packages").is_some();
                    if has_packages {
                        Check::pass(
                            "SBOM generation",
                            vec![
                                "xtask/src/sbom.rs exists".to_string(),
                                "target/sbom.spdx.json exists and is valid".to_string(),
                                format!("Packages count: {}", json["packages"].as_array().map_or(0, |a| a.len())),
                            ],
                        )
                    } else {
                        Check::fail(
                            "SBOM generation",
                            vec!["SBOM file exists but missing packages array".to_string()],
                            "Invalid SBOM structure",
                        )
                    }
                } else {
                    Check::fail(
                        "SBOM generation",
                        vec!["SBOM file exists but is not valid JSON".to_string()],
                        "Invalid JSON",
                    )
                }
            }
            Err(e) => Check::fail(
                "SBOM generation",
                vec![],
                format!("Failed to read SBOM: {}", e),
            ),
        }
    } else {
        Check::skip(
            "SBOM generation",
            "sbom.rs module exists but SBOM not yet generated (run: cargo xtask sbom)",
        )
    }
}

fn check_backup_gc() -> Check {
    let backup_script = Path::new("scripts/backup.sh");
    let gc_script = Path::new("scripts/gc_bundles.sh");

    let has_backup = backup_script.exists();
    let has_gc = gc_script.exists();

    let mut evidence = Vec::new();
    if has_backup {
        evidence.push("scripts/backup.sh exists".to_string());
    }
    if has_gc {
        evidence.push("scripts/gc_bundles.sh exists".to_string());
    }

    if has_backup && has_gc {
        Check::pass("Backup & GC scripts", evidence)
    } else {
        let missing: Vec<_> = [
            (!has_backup).then(|| "backup.sh"),
            (!has_gc).then(|| "gc_bundles.sh"),
        ]
        .into_iter()
        .flatten()
        .collect();

        Check::fail(
            "Backup & GC scripts",
            evidence,
            format!("Missing: {}", missing.join(", ")),
        )
    }
}

fn check_orchestrator() -> Check {
    let orchestrator_crate = Path::new("crates/mplora-orchestrator");
    if !orchestrator_crate.exists() {
        return Check::fail(
            "Orchestrator",
            vec![],
            "crates/mplora-orchestrator not found",
        );
    }

    // Check for verify command or gate enforcement
    let lib_rs = orchestrator_crate.join("src/lib.rs");
    if let Ok(content) = fs::read_to_string(&lib_rs) {
        let has_verify = content.contains("verify") || content.contains("gate");
        if has_verify {
            Check::pass(
                "Orchestrator",
                vec![
                    "crates/mplora-orchestrator exists".to_string(),
                    "Gate enforcement logic found".to_string(),
                ],
            )
        } else {
            Check::skip(
                "Orchestrator",
                "Orchestrator crate exists but gate logic not found",
            )
        }
    } else {
        Check::skip("Orchestrator", "Could not read orchestrator source")
    }
}

fn check_release_checklist() -> Check {
    let checklist = Path::new("docs/RELEASE_CHECKLIST.md");
    if !checklist.exists() {
        return Check::fail(
            "Release checklist",
            vec![],
            "docs/RELEASE_CHECKLIST.md not found",
        );
    }

    let content = match fs::read_to_string(checklist) {
        Ok(c) => c,
        Err(e) => {
            return Check::fail(
                "Release checklist",
                vec![],
                format!("Failed to read checklist: {}", e),
            )
        }
    };

    // Check if it references orchestrator
    let has_orchestrator_ref = content.contains("orchestrator") || content.contains("mplora-orchestrator");

    if has_orchestrator_ref {
        Check::pass(
            "Release checklist",
            vec![
                "docs/RELEASE_CHECKLIST.md exists".to_string(),
                "References orchestrator commands".to_string(),
            ],
        )
    } else {
        Check::skip(
            "Release checklist",
            "Checklist exists but doesn't reference orchestrator",
        )
    }
}
