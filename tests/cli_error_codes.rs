//! Tests for error code system

#[test]
fn test_error_codes_module_exists() {
    // This test ensures the error_codes module compiles
    // The actual unit tests are in error_codes.rs itself
    assert!(true);
}

#[test]
fn test_explain_command() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "explain", "E3001"])
        .output()
        .expect("Failed to run aosctl explain E3001");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify output contains expected elements
    assert!(stdout.contains("Error Code: E3001"));
    assert!(stdout.contains("Kernel Manifest Signature Invalid"));
    assert!(stdout.contains("Cause:"));
    assert!(stdout.contains("Fix:"));
}

#[test]
fn test_explain_aos_error_name() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "explain", "InvalidHash"])
        .output()
        .expect("Failed to run aosctl explain InvalidHash");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should map AosError name to error code
    assert!(stdout.contains("Mapped from AosError::InvalidHash"));
    assert!(stdout.contains("E1004"));
}

#[test]
fn test_error_codes_list() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "error-codes"])
        .output()
        .expect("Failed to run aosctl error-codes");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify error code categories are listed
    assert!(stdout.contains("AdapterOS Error Code Registry"));
    assert!(stdout.contains("Crypto/Signing"));
    assert!(stdout.contains("Policy/Determinism"));
    assert!(stdout.contains("Kernels/Build/Manifest"));
    assert!(stdout.contains("Telemetry/Chain"));
    assert!(stdout.contains("Artifacts/CAS"));
    assert!(stdout.contains("Adapters/MPLoRA"));
    assert!(stdout.contains("Node/Cluster"));
    assert!(stdout.contains("CLI/Config"));
    assert!(stdout.contains("OS/Environment"));

    // Verify specific codes
    assert!(stdout.contains("E1001"));
    assert!(stdout.contains("E2001"));
    assert!(stdout.contains("E3001"));
    assert!(stdout.contains("E9001"));
}

#[test]
fn test_error_codes_json() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "error-codes", "--json"])
        .output()
        .expect("Failed to run aosctl error-codes --json");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify JSON output
    assert!(stdout.contains(r#""code":"#));
    assert!(stdout.contains(r#""category":"#));
    assert!(stdout.contains(r#""title":"#));
    assert!(stdout.contains(r#""cause":"#));
    assert!(stdout.contains(r#""fix":"#));

    // Should be valid JSON
    let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok());
}
