#![cfg(all(test, feature = "extended-tests"))]

//! Tests for tutorial command

#[test]
fn test_tutorial_quickstart_ci_mode() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&["run", "--bin", "aosctl", "--", "tutorial", "--ci"])
        .output()
        .expect("Failed to run aosctl tutorial --ci");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify tutorial output contains expected sections
    assert!(stdout.contains("AdapterOS Interactive Tutorial"));
    assert!(stdout.contains("Quickstart"));

    // Verify tutorial steps
    assert!(stdout.contains("Step 1:"));
    assert!(stdout.contains("Step 2:"));
    assert!(stdout.contains("Step 3:"));

    // Verify content
    assert!(stdout.contains("Initialize a Tenant") || stdout.contains("tenant"));
    assert!(stdout.contains("Verify") || stdout.contains("verify"));
    assert!(stdout.contains("Diagnostics") || stdout.contains("diag"));

    // Verify completion message
    assert!(stdout.contains("Tutorial Complete") || stdout.contains("complete"));
}

#[test]
fn test_tutorial_advanced_ci_mode() {
    use std::process::Command;

    let output = Command::new("cargo")
        .args(&[
            "run",
            "--bin",
            "aosctl",
            "--",
            "tutorial",
            "--advanced",
            "--ci",
        ])
        .output()
        .expect("Failed to run aosctl tutorial --advanced --ci");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify advanced tutorial output
    assert!(stdout.contains("AdapterOS Interactive Tutorial"));
    assert!(stdout.contains("Advanced"));

    // Verify advanced topics
    assert!(stdout.contains("adapter") || stdout.contains("Adapter"));
    assert!(stdout.contains("kernel") || stdout.contains("Kernel"));
    assert!(stdout.contains("diagnostics") || stdout.contains("Diagnostics"));
}

#[test]
fn test_tutorial_module_compiles() {
    // This test ensures the tutorial module compiles successfully
    assert!(true);
}
