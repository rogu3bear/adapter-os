//! Integration tests for the aosctl doctor command

use std::process::Command;

#[test]
fn test_doctor_command_help_output() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "aosctl", "--", "doctor", "--help"])
        .output()
        .expect("Failed to run aosctl doctor --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify help output contains expected elements
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("Check system health"));
    assert!(stdout.contains("--server-url"));
    assert!(stdout.contains("--timeout"));
}

#[test]
fn test_doctor_command_with_invalid_server() {
    let output = Command::new("cargo")
        .args([
            "run", "--bin", "aosctl", "--", "doctor",
            "--server-url", "http://127.0.0.1:99999",
            "--timeout", "1"
        ])
        .output()
        .expect("Failed to run aosctl doctor with invalid server");

    // The command should exit with non-zero status for connection failure
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Should contain some error information
    assert!(stderr.len() > 0);
}

#[test]
fn test_doctor_command_argument_validation() {
    // Test invalid timeout value
    let output = Command::new("cargo")
        .args([
            "run", "--bin", "aosctl", "--", "doctor",
            "--timeout", "invalid"
        ])
        .output()
        .expect("Failed to run aosctl doctor with invalid timeout");

    // Should fail with argument parsing error
    assert!(!output.status.success());
}
