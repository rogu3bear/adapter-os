use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("create temp dir")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("test_data")
        .join("replay_fixtures")
}

fn run_aosctl(args: &[&str]) -> std::process::Output {
    Command::new("cargo")
        .args(["run", "--bin", "aosctl", "--"])
        .args(args)
        .output()
        .expect("aosctl command should run")
}

#[test]
fn export_then_replay_passes() {
    let temp = new_test_tempdir();
    let out_dir = temp.path().join("basic");
    let fixture = fixture_root();

    let export_status = run_aosctl(&[
        "trace",
        "export",
        "--request",
        "basic",
        "--out",
        out_dir.to_str().unwrap(),
        "--fixtures",
        fixture.to_str().unwrap(),
    ]);
    assert!(
        export_status.status.success(),
        "trace export should succeed"
    );

    let replay_status = run_aosctl(&["replay", "--dir", out_dir.to_str().unwrap(), "--verify"]);
    assert!(replay_status.status.success(), "replay should pass");
}

#[test]
fn replay_fails_when_gate_is_modified() {
    let temp = new_test_tempdir();
    let out_dir = temp.path().join("basic");
    let fixture = fixture_root();

    let export_status = run_aosctl(&[
        "trace",
        "export",
        "--request",
        "basic",
        "--out",
        out_dir.to_str().unwrap(),
        "--fixtures",
        fixture.to_str().unwrap(),
    ]);
    assert!(
        export_status.status.success(),
        "trace export should succeed"
    );

    let trace_path = out_dir.join("token_trace.json");
    let mut trace_json: Value =
        serde_json::from_str(&std::fs::read_to_string(&trace_path).unwrap()).unwrap();
    if let Some(first_gate) = trace_json
        .get_mut("steps")
        .and_then(|s| s.as_array_mut())
        .and_then(|arr| arr.get_mut(0))
    {
        if let Some(gate) = first_gate.get_mut("gate_q15") {
            *gate = Value::from(16001);
        }
    }
    std::fs::write(
        &trace_path,
        serde_json::to_string_pretty(&trace_json).unwrap(),
    )
    .unwrap();

    let replay_status = run_aosctl(&["replay", "--dir", out_dir.to_str().unwrap(), "--verify"]);
    assert!(
        !replay_status.status.success(),
        "replay should fail after gate edit"
    );
}

#[test]
fn replay_fails_when_adapter_hash_changes() {
    let temp = new_test_tempdir();
    let out_dir = temp.path().join("basic");
    let fixture = fixture_root();

    let export_status = run_aosctl(&[
        "trace",
        "export",
        "--request",
        "basic",
        "--out",
        out_dir.to_str().unwrap(),
        "--fixtures",
        fixture.to_str().unwrap(),
    ]);
    assert!(
        export_status.status.success(),
        "trace export should succeed"
    );

    let manifest_path = out_dir.join("context_manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    if let Some(adapters) = manifest
        .get_mut("adapters")
        .and_then(|a| a.as_array_mut())
        .and_then(|arr| arr.get_mut(0))
    {
        if let Some(hash) = adapters.get_mut("hash") {
            *hash = Value::from("4444444444444444444444444444444444444444444444444444444444444444");
        }
    }
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let replay_status = run_aosctl(&["replay", "--dir", out_dir.to_str().unwrap(), "--verify"]);
    assert!(
        !replay_status.status.success(),
        "replay should fail after adapter hash change"
    );
}

#[test]
fn cross_worker_fixture_passes_with_allow_flag() {
    let temp = new_test_tempdir();
    let out_dir = temp.path().join("cross");
    let fixture = fixture_root();

    let export_status = run_aosctl(&[
        "trace",
        "export",
        "--request",
        "cross_worker",
        "--out",
        out_dir.to_str().unwrap(),
        "--fixtures",
        fixture.to_str().unwrap(),
    ]);
    assert!(
        export_status.status.success(),
        "trace export should succeed"
    );

    let replay_status = run_aosctl(&["replay", "--dir", out_dir.to_str().unwrap(), "--verify"]);
    assert!(replay_status.status.success(), "cross-worker should pass");
}
