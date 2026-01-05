#![allow(clippy::bool_comparison)]
#![allow(clippy::needless_borrows_for_generic_args)]

use std::env;
use std::process::Command;

fn should_skip() -> bool {
    env::var("AOS_CLI_TESTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        == false
}

/// Resolve the `aos` binary, driven via cargo run on the correct package.
fn build_aos_command(args: &[&str]) -> Command {
    // Use the adapteros-aos package which defines the `aos` bin.
    let mut cmd = Command::new("cargo");
    cmd.args(&[
        "run",
        "--quiet",
        "-p",
        "adapteros-aos",
        "--bin",
        "aos",
        "--",
    ])
    .args(args);
    // Provide a valid database URL to satisfy config validation (defaults are stricter).
    cmd.env("AOS_DATABASE_URL", "sqlite::memory:");
    cmd.env("AOS_LOG_LEVEL", "info");
    cmd
}

const CONFIG_ARGS: [&str; 2] = ["--config", "configs/aos.toml"];

#[test]
fn aos_help_exits_successfully() {
    if should_skip() {
        eprintln!("skipping aos_help_exits_successfully (set AOS_CLI_TESTS=1 to run)");
        return;
    }
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
    if should_skip() {
        eprintln!("skipping aos_start_backend_dry_run (set AOS_CLI_TESTS=1 to run)");
        return;
    }
    let output = build_aos_command(&[
        CONFIG_ARGS[0],
        CONFIG_ARGS[1],
        "start",
        "backend",
        "--dry-run",
    ])
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
    if should_skip() {
        eprintln!("skipping aos_status_json_is_structured (set AOS_CLI_TESTS=1 to run)");
        return;
    }
    let output = build_aos_command(&[CONFIG_ARGS[0], CONFIG_ARGS[1], "status", "--json"])
        .output()
        .expect("failed to execute aos status --json");

    assert!(
        output.status.success(),
        "aos status --json should exit 0, got {:?}",
        output.status
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parsed_block: Option<serde_json::Value> = None;
    for value in serde_json::Deserializer::from_str(&stdout).into_iter::<serde_json::Value>() {
        parsed_block = Some(value.expect("status output JSON fragment invalid"));
    }
    let parsed = parsed_block.expect("status output should contain JSON payload");

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
