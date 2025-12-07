use std::path::Path;
use std::process::Command;

const PROJECT_ROOT: &str = env!("CARGO_MANIFEST_DIR");
const START_SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/start");

fn run_start(args: &[&str]) -> std::process::Output {
    Command::new(START_SCRIPT)
        .args(args)
        .current_dir(PROJECT_ROOT)
        .env("AOS_DEV_SKIP_DRIFT_CHECK", "1")
        .env("AOS_DEV_NO_AUTH", "1")
        .output()
        .expect("failed to invoke ./start")
}

#[test]
fn start_help_is_available() {
    assert!(
        Path::new(START_SCRIPT).exists(),
        "./start script should exist at repository root"
    );

    let output = run_start(&["help"]);
    assert!(
        output.status.success(),
        "expected ./start help to succeed, got {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Start all services") || stdout.contains("Commands:"),
        "help output should mention commands; stdout:\n{}",
        stdout
    );
}

#[test]
fn start_status_reports() {
    let output = run_start(&["status"]);
    assert!(
        output.status.success(),
        "expected ./start status to succeed, got {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
