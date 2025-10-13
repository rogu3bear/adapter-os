//! Golden tests for CLI help text
//!
//! These tests ensure help text remains stable and contains expected examples.

use std::process::Command;

#[test]
fn test_main_help() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "--help"])
        .output()
        .expect("Failed to run aosctl --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify main sections are present
    assert!(help_text.contains("AdapterOS command-line interface"));
    assert!(help_text.contains("Usage:"));
    assert!(help_text.contains("Commands:"));
    assert!(help_text.contains("Options:"));

    // Verify new commands are listed
    assert!(help_text.contains("diag"));
    assert!(help_text.contains("explain"));
    assert!(help_text.contains("tutorial"));
    assert!(help_text.contains("manual"));
}

#[test]
fn test_diag_help() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "diag", "--help"])
        .output()
        .expect("Failed to run aosctl diag --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify diag command help
    assert!(help_text.contains("Run system diagnostics"));
    assert!(help_text.contains("--system"));
    assert!(help_text.contains("--tenant"));
    assert!(help_text.contains("--full"));
    assert!(help_text.contains("--bundle"));
    assert!(help_text.contains("--json"));
}

#[test]
fn test_explain_help() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "explain", "--help"])
        .output()
        .expect("Failed to run aosctl explain --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify explain command help
    assert!(help_text.contains("Explain an error code"));
    assert!(help_text.contains("Error code (E3001) or AosError name"));
}

#[test]
fn test_tutorial_help() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "tutorial", "--help"])
        .output()
        .expect("Failed to run aosctl tutorial --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify tutorial command help
    assert!(help_text.contains("Interactive tutorial"));
    assert!(help_text.contains("--advanced"));
    assert!(help_text.contains("--ci"));
}

#[test]
fn test_init_tenant_examples() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "init-tenant", "--help"])
        .output()
        .expect("Failed to run aosctl init-tenant --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify examples are present
    assert!(help_text.contains("Examples:"));
    assert!(help_text.contains("tenant_dev"));
    assert!(help_text.contains("--uid"));
    assert!(help_text.contains("--gid"));
}

#[test]
fn test_serve_examples() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "serve", "--help"])
        .output()
        .expect("Failed to run aosctl serve --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify examples are present with real values
    assert!(help_text.contains("Examples:"));
    assert!(help_text.contains("--tenant"));
    assert!(help_text.contains("--plan"));
    assert!(help_text.contains("--dry-run"));
}

#[test]
fn test_import_model_examples() {
    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "import-model", "--help"])
        .output()
        .expect("Failed to run aosctl import-model --help");

    let help_text = String::from_utf8_lossy(&output.stdout);

    // Verify examples include real model (qwen2.5-7b)
    assert!(help_text.contains("Examples:"));
    assert!(help_text.contains("qwen2.5-7b"));
    assert!(help_text.contains("models/"));
    assert!(help_text.contains("weights.safetensors"));
}
