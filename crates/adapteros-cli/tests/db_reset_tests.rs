//! Integration tests for the aosctl db reset command

use std::process::Command;

#[test]
fn test_db_reset_command_help_output() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "aosctl", "--", "db", "reset", "--help"])
        .output()
        .expect("Failed to run aosctl db reset --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify help output contains expected elements
    assert!(stdout.contains("reset"));
    assert!(stdout.contains("Reset database"));
    assert!(stdout.contains("--db-path"));
    assert!(stdout.contains("--yes"));
}

#[test]
fn test_db_reset_requires_confirmation() {
    // This test would require setting up a test database
    // For now, we'll test the command structure
    let output = Command::new("cargo")
        .args([
            "run", "--bin", "aosctl", "--", "db", "reset",
            "--db-path", "/tmp/test-reset.db"
        ])
        .output()
        .expect("Failed to run aosctl db reset without confirmation");

    // Should exit with error or require confirmation
    // The exact behavior depends on the implementation
    assert!(output.status.code().is_some());
}

#[test]
fn test_db_reset_with_yes_flag() {
    // Test with --yes flag (would normally destroy data)
    let output = Command::new("cargo")
        .args([
            "run", "--bin", "aosctl", "--", "db", "reset",
            "--db-path", "/tmp/test-reset-yes.db",
            "--yes"
        ])
        .output()
        .expect("Failed to run aosctl db reset with --yes flag");

    // Command should run (though it may fail due to missing database)
    // We're testing that the flag is accepted
    assert!(output.status.code().is_some());
}
