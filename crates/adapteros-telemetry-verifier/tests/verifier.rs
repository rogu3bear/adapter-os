use adapteros_telemetry_verifier::{run_with_specs, InvariantSpec};
use std::fs;
use tempfile::TempDir;

fn write(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn fixture_dir() -> TempDir {
    TempDir::new().expect("tempdir")
}

#[test]
fn happy_path_passes() {
    let dir = fixture_dir();
    let root = dir.path();
    write(&root.join("runtime_ok.txt"), "foo bar baz");
    write(&root.join("test_ok.txt"), "foo tested");

    let specs = [InvariantSpec {
        id: "V-TEST-1",
        description: "runtime + test present",
        required_runtime_fields: &["foo"],
        required_test_fields: &["tested"],
        runtime_files: &["runtime_ok.txt"],
        test_files: &["test_ok.txt"],
    }];

    let report = run_with_specs(root, &specs).expect("verifier runs");
    assert!(!report.failed, "expected verifier to pass");
}

#[test]
fn missing_runtime_fails() {
    let dir = fixture_dir();
    let root = dir.path();
    write(&root.join("test_ok.txt"), "tested");

    let specs = [InvariantSpec {
        id: "V-TEST-2",
        description: "missing runtime field should fail",
        required_runtime_fields: &["foo"],
        required_test_fields: &["tested"],
        runtime_files: &["runtime_missing.txt"],
        test_files: &["test_ok.txt"],
    }];

    let report = run_with_specs(root, &specs).expect("verifier runs");
    assert!(report.failed, "expected verifier failure");
    let entry = report
        .invariants
        .iter()
        .find(|inv| inv.id == "V-TEST-2")
        .unwrap();
    assert_eq!(entry.missing_runtime_fields, vec!["foo"]);
}

#[test]
fn missing_tests_fail() {
    let dir = fixture_dir();
    let root = dir.path();
    write(&root.join("runtime_ok.txt"), "foo bar");

    let specs = [InvariantSpec {
        id: "V-TEST-3",
        description: "missing test field should fail",
        required_runtime_fields: &["foo"],
        required_test_fields: &["tested"],
        runtime_files: &["runtime_ok.txt"],
        test_files: &["test_missing.txt"],
    }];

    let report = run_with_specs(root, &specs).expect("verifier runs");
    assert!(report.failed, "expected verifier failure");
    let entry = report
        .invariants
        .iter()
        .find(|inv| inv.id == "V-TEST-3")
        .unwrap();
    assert_eq!(entry.missing_test_fields, vec!["tested"]);
}
