#![cfg(all(test, feature = "extended-tests"))]

//! Tests for diag command

#[test]
fn test_diag_system_runs() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "diag", "--system"])
        .output()
        .expect("Failed to run aosctl diag --system");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify diagnostic output contains expected checks
    assert!(stdout.contains("adapterOS Diagnostics"));
    assert!(stdout.contains("System Checks"));

    // Should check these components
    // Note: Some might fail/warn in dev environment, that's okay
    let checks_present = stdout.contains("Metal Device")
        || stdout.contains("Memory")
        || stdout.contains("Disk Space")
        || stdout.contains("Permissions")
        || stdout.contains("Database")
        || stdout.contains("Kernel");

    assert!(
        checks_present,
        "Expected at least some diagnostic checks in output"
    );
}

#[test]
fn test_diag_json_output() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "diag", "--system", "--json"])
        .output()
        .expect("Failed to run aosctl diag --system --json");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify JSON structure
    assert!(stdout.contains(r#""profile":"#));
    assert!(stdout.contains(r#""has_warnings":"#));
    assert!(stdout.contains(r#""has_failures":"#));
    assert!(stdout.contains(r#""exit_code":"#));
    assert!(stdout.contains(r#""checks":"#));

    // Should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "JSON output should be valid");

    if let Ok(json) = parsed {
        // Verify structure
        assert!(json.get("checks").is_some());
        assert!(json.get("exit_code").is_some());

        if let Some(checks) = json.get("checks").and_then(|c| c.as_array()) {
            assert!(!checks.is_empty(), "Should have at least one check");

            // Verify check structure
            if let Some(first_check) = checks.first() {
                assert!(first_check.get("check_name").is_some());
                assert!(first_check.get("status").is_some());
                assert!(first_check.get("message").is_some());
            }
        }
    }
}

#[test]
fn test_diag_exit_codes() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "diag", "--system"])
        .output()
        .expect("Failed to run aosctl diag --system");

    // Exit code should be one of: 0 (pass), 10 (warnings), 20 (failures)
    let exit_code = output.status.code().unwrap_or(-1);
    assert!(
        exit_code == 0 || exit_code == 10 || exit_code == 20,
        "Exit code should be 0, 10, or 20, got {}",
        exit_code
    );
}

#[test]
fn test_diag_profile_enum() {
    // Test that DiagProfile enum values work correctly
    use adapteros_cli::commands::diag::DiagProfile;

    let system = DiagProfile::System;
    let tenant = DiagProfile::Tenant;
    let full = DiagProfile::Full;

    // Just verify they're different
    assert!(system != tenant);
    assert!(tenant != full);
    assert!(system != full);
}
