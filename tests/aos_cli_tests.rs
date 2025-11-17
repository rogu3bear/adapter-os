use std::process::Command;

/// Resolve the `aos` binary, preferring a built binary and
/// falling back to `cargo run --bin aos` when necessary.
fn build_aos_command(args: &[&str]) -> Command {
    // For now we always drive via `cargo run --bin aos` to keep
    // behavior consistent in development and CI.
    let mut cmd = Command::new("cargo");
    cmd.args(&["run", "--quiet", "--bin", "aos", "--"])
        .args(args);
    cmd
}

#[test]
fn aos_help_exits_successfully() {
    let output = build_aos_command(&["--help"])
        .output()
        .expect("failed to execute aos --help");

    assert!(
        output.status.success(),
        "aos --help should exit 0, got {:?}",
        output.status
    );
}

#[test]
fn aos_start_backend_dry_run() {
    let output = build_aos_command(&["start", "backend", "--dry-run"])
        .output()
        .expect("failed to execute aos start backend --dry-run");

    assert!(
        output.status.success(),
        "aos start backend --dry-run should exit 0, got {:?}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Dry-run") || stdout.contains("would-start"),
        "expected dry-run output, got: {stdout}"
    );
}

#[test]
fn aos_status_json_is_structured() {
    let output = build_aos_command(&["status", "--json"])
        .output()
        .expect("failed to execute aos status --json");

    assert!(
        output.status.success(),
        "aos status --json should exit 0, got {:?}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("status --json did not return valid JSON");

    // Basic shape check: top-level fields and services array.
    assert!(
        parsed.get("component").is_some(),
        "status JSON should include component field"
    );
    let services = parsed
        .get("services")
        .and_then(|v| v.as_array())
        .expect("status JSON should include services array");

    // Ensure we have entries for the known services.
    let mut have_backend = false;
    let mut have_ui = false;
    let mut have_menubar = false;
    for svc in services {
        if let Some(name) = svc.get("service").and_then(|v| v.as_str()) {
            match name {
                "backend" => have_backend = true,
                "ui" => have_ui = true,
                "menu-bar" => have_menubar = true,
                _ => {}
            }
        }
    }

    assert!(have_backend, "status JSON should include backend service");
    assert!(have_ui, "status JSON should include ui service");
    assert!(have_menubar, "status JSON should include menu-bar service");
}
