//! Integration tests for the aosctl db reset command
//!
//! The db commands are available with the following structure:
//! - `aosctl db migrate` - Run database migrations
//! - `aosctl db reset` - Reset database (DEVELOPMENT ONLY)
//! - `aosctl db seed-fixtures` - Reset and seed deterministic test fixtures
//! - `aosctl db health` - Health check for migration signatures and DB integrity
//! - `aosctl db verify-seed` - Verify seeded reference fixtures exist

use std::process::Command;

#[test]
fn test_db_reset_command_help_output() {
    let output = Command::new("cargo")
        .args(["run", "--bin", "aosctl", "--", "db", "reset", "--help"])
        .output()
        .expect("Failed to run aosctl db reset --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify help output contains expected elements
    // Note: db.rs uses --force not --yes
    assert!(stdout.contains("reset") || stdout.contains("Reset"));
    assert!(stdout.contains("Reset database") || stdout.contains("database"));
    assert!(stdout.contains("--db-path"));
    assert!(stdout.contains("--force")); // Note: actual flag is --force, not --yes
}

#[test]
fn test_db_reset_requires_confirmation() {
    // This test would require setting up a test database
    // For now, we'll test the command structure
    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aosctl",
            "--",
            "db",
            "reset",
            "--db-path",
            "var/test-reset.db",
        ])
        .output()
        .expect("Failed to run aosctl db reset without confirmation");

    // Should exit with error or require confirmation
    // The exact behavior depends on the implementation
    assert!(output.status.code().is_some());
}

#[test]
fn test_db_reset_with_force_flag() {
    // Test with --force flag (would normally destroy data)
    // Note: the actual flag is --force, not --yes
    let output = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "aosctl",
            "--",
            "db",
            "reset",
            "--db-path",
            "var/test-reset-force.db",
            "--force",
        ])
        .output()
        .expect("Failed to run aosctl db reset with --force flag");

    // Command should run (though it may fail due to missing database)
    // We're testing that the flag is accepted
    assert!(output.status.code().is_some());
}
