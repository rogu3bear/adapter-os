//! Integration tests for the aosctl doctor command

#![allow(clippy::len_zero)]

use std::process::Command;

#[test]
fn test_doctor_command_help_output() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "aosctl", "--", "doctor", "--help"])
        .output()
        .expect("Failed to run aosctl doctor --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify help output contains expected elements
    // The command description is "Run system health diagnostics"
    assert!(
        stdout.contains("doctor") || stdout.contains("health"),
        "Help should reference doctor or health"
    );
    assert!(
        stdout.contains("health") || stdout.contains("diagnostics"),
        "Help should describe health checking functionality"
    );
    assert!(stdout.contains("--server-url"));
    assert!(stdout.contains("--timeout"));
}

#[test]
fn test_doctor_command_with_invalid_server() {
    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aosctl",
            "--",
            "doctor",
            "--server-url",
            "http://127.0.0.1:99999",
            "--timeout",
            "1",
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
            "run",
            "--bin",
            "aosctl",
            "--",
            "doctor",
            "--timeout",
            "invalid",
        ])
        .output()
        .expect("Failed to run aosctl doctor with invalid timeout");

    // Should fail with argument parsing error
    assert!(!output.status.success());
}
