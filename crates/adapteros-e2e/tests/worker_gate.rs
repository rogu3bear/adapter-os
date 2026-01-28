#![cfg(feature = "worker-gate")]

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const DEFAULT_VOCAB_SIZE: usize = 32;
const DEFAULT_HIDDEN_DIM: usize = 8;
const DEFAULT_NUM_LAYERS: usize = 1;
const DEFAULT_NUM_HEADS: usize = 1;
const DEFAULT_NUM_KV_HEADS: usize = 1;
const DEFAULT_INTERMEDIATE_SIZE: usize = 16;
const DEFAULT_MAX_SEQ_LEN: usize = 128;

const INFER_COUNT: usize = 10;
const INFER_MAX_ATTEMPTS: usize = 20;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Serialize)]
struct GateCheck {
    name: String,
    status: String,
    duration_ms: u128,
    details: Value,
    error: Option<String>,
}

#[derive(Serialize)]
struct GateSummary {
    total: usize,
    passed: usize,
    failed: usize,
}

#[derive(Serialize)]
struct GateReport {
    gate: String,
    status: String,
    timestamp_unix: u64,
    checks: Vec<GateCheck>,
    summary: GateSummary,
}

struct WorkerGateRun {
    success_count: usize,
    attempts: usize,
    telemetry_bytes: u64,
    exit_code: Option<i32>,
    exit_success: bool,
    panic_detected: bool,
    socket_path: PathBuf,
    telemetry_dir: PathBuf,
    worker_log_path: PathBuf,
    manifest_path: PathBuf,
    model_path: PathBuf,
    tokenizer_path: PathBuf,
    startup_error: Option<String>,
    infer_error: Option<String>,
    shutdown_error: Option<String>,
}

static WORKER_GATE_RUN: OnceLock<Result<WorkerGateRun, String>> = OnceLock::new();

#[test]
fn worker_gate() {
    let checks = vec![
        run_check_with_worker_gate("aosctl_infer", check_aosctl_infer),
        run_check_with_worker_gate("telemetry_non_empty", check_telemetry_non_empty),
        run_check_with_worker_gate("worker_shutdown_clean", check_worker_shutdown_clean),
        run_check_with_worker_gate("shutdown_panic_free", check_shutdown_panic_free),
        run_check_with_worker_gate("pinned_residency", check_pinned_residency),
    ];

    let passed = checks.iter().filter(|c| c.status == "pass").count();
    let failed = checks.len() - passed;
    let status = if failed == 0 { "pass" } else { "fail" };

    let report = GateReport {
        gate: "worker-gate".to_string(),
        status: status.to_string(),
        timestamp_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_secs(0))
            .as_secs(),
        checks,
        summary: GateSummary {
            total: passed + failed,
            passed,
            failed,
        },
    };

    let report_path = write_report(&report).unwrap_or_else(|err| {
        panic!("worker gate: failed to write report: {}", err);
    });

    if report.status != "pass" {
        let failed_checks: Vec<&GateCheck> = report
            .checks
            .iter()
            .filter(|c| c.status != "pass")
            .collect();
        panic!(
            "worker gate failed: {} failed checks (report: {})\n{:?}",
            failed_checks.len(),
            report_path.display(),
            failed_checks
                .iter()
                .map(|c| format!("{}: {}", c.name, c.error.clone().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
}

fn run_check<F>(name: &str, f: F) -> GateCheck
where
    F: FnOnce() -> Result<Value, String>,
{
    let start = Instant::now();
    match f() {
        Ok(details) => GateCheck {
            name: name.to_string(),
            status: "pass".to_string(),
            duration_ms: start.elapsed().as_millis(),
            details,
            error: None,
        },
        Err(error) => GateCheck {
            name: name.to_string(),
            status: "fail".to_string(),
            duration_ms: start.elapsed().as_millis(),
            details: json!({}),
            error: Some(error),
        },
    }
}

fn run_check_with_worker_gate<F>(name: &str, f: F) -> GateCheck
where
    F: FnOnce(&WorkerGateRun) -> Result<Value, String>,
{
    run_check(name, || {
        let run = worker_gate_run()?;
        f(run)
    })
}

fn worker_gate_run() -> Result<&'static WorkerGateRun, String> {
    let result = WORKER_GATE_RUN.get_or_init(run_worker_gate);
    match result {
        Ok(run) => Ok(run),
        Err(err) => Err(err.clone()),
    }
}

fn check_aosctl_infer(run: &WorkerGateRun) -> Result<Value, String> {
    if let Some(err) = run.startup_error.as_ref() {
        return Err(format!("worker startup failed: {}", err));
    }
    if let Some(err) = run.infer_error.as_ref() {
        return Err(err.clone());
    }
    if run.success_count != INFER_COUNT {
        return Err(format!(
            "aosctl infer success count {} (attempts {}) does not match {}",
            run.success_count, run.attempts, INFER_COUNT
        ));
    }

    Ok(json!({
        "success_count": run.success_count,
        "attempts": run.attempts,
        "socket_path": run.socket_path.display().to_string(),
        "manifest_path": run.manifest_path.display().to_string(),
        "model_path": run.model_path.display().to_string(),
        "tokenizer_path": run.tokenizer_path.display().to_string(),
    }))
}

fn check_telemetry_non_empty(run: &WorkerGateRun) -> Result<Value, String> {
    if run.telemetry_bytes == 0 {
        return Err("telemetry bundle is empty after inference".to_string());
    }

    Ok(json!({
        "telemetry_bytes": run.telemetry_bytes,
        "telemetry_dir": run.telemetry_dir.display().to_string(),
    }))
}

fn check_worker_shutdown_clean(run: &WorkerGateRun) -> Result<Value, String> {
    if let Some(err) = run.shutdown_error.as_ref() {
        return Err(err.clone());
    }
    if !run.exit_success {
        return Err(format!("worker exited with status {:?}", run.exit_code));
    }

    Ok(json!({
        "exit_code": run.exit_code,
    }))
}

fn check_shutdown_panic_free(run: &WorkerGateRun) -> Result<Value, String> {
    if run.panic_detected {
        return Err("worker panic detected during shutdown".to_string());
    }

    Ok(json!({
        "panic_detected": run.panic_detected,
        "worker_log": run.worker_log_path.display().to_string(),
    }))
}

fn check_pinned_residency(run: &WorkerGateRun) -> Result<Value, String> {
    if run.telemetry_bytes == 0 {
        return Err("telemetry bundle is empty; cannot validate pinned residency".to_string());
    }

    let event = find_telemetry_event(&run.telemetry_dir, "model.residency")?
        .ok_or_else(|| "model.residency telemetry event missing".to_string())?;
    let metadata = event.get("metadata").cloned().unwrap_or_else(|| json!({}));
    let pinned = metadata
        .get("pinned")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let load_count = metadata.get("load_count").and_then(|value| value.as_u64());
    let evict_count = metadata.get("evict_count").and_then(|value| value.as_u64());

    if !pinned {
        return Err("base model not pinned during run".to_string());
    }

    Ok(json!({
        "pinned": pinned,
        "load_count": load_count,
        "evict_count": evict_count,
        "telemetry_dir": run.telemetry_dir.display().to_string(),
    }))
}

fn run_worker_gate() -> Result<WorkerGateRun, String> {
    let root = repo_root();
    let report_dir = root.join("target/worker-gate");
    let telemetry_dir = report_dir.join("telemetry");
    let worker_log_path = report_dir.join("worker.log");
    let cp_log_path = report_dir.join("cp_stub.log");
    let socket_path = root.join("var/run/worker.sock");

    fs::create_dir_all(&report_dir).map_err(|e| e.to_string())?;
    if telemetry_dir.exists() {
        fs::remove_dir_all(&telemetry_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&telemetry_dir).map_err(|e| e.to_string())?;
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let _ = fs::remove_file(&socket_path);

    let manifest_path = resolve_manifest_path(&report_dir)?;
    let (model_path, tokenizer_path) = resolve_model_paths(&report_dir)?;

    if !manifest_path.exists() {
        return Err(format!(
            "manifest missing at {} (set AOS_WORKER_GATE_MANIFEST to override)",
            manifest_path.display()
        ));
    }
    if !model_path.exists() {
        return Err(format!(
            "model path missing at {} (set AOS_WORKER_GATE_MODEL_PATH to override)",
            model_path.display()
        ));
    }
    if !tokenizer_path.exists() {
        return Err(format!(
            "tokenizer missing at {} (set AOS_WORKER_GATE_TOKENIZER or AOS_WORKER_GATE_MODEL_PATH to override)",
            tokenizer_path.display()
        ));
    }

    build_binaries(&root)?;

    let mut cp_stub = spawn_cp_stub(&root, &cp_log_path)?;
    let mut worker = match spawn_worker(
        &root,
        &worker_log_path,
        &manifest_path,
        &model_path,
        &tokenizer_path,
        &socket_path,
        &telemetry_dir,
    ) {
        Ok(worker) => worker,
        Err(err) => {
            let _ = stop_process(&mut cp_stub);
            return Err(err);
        }
    };

    let mut startup_error: Option<String> = None;
    let mut infer_error: Option<String> = None;
    let mut shutdown_error: Option<String> = None;
    let mut success_count = 0usize;
    let mut attempts = 0usize;
    let mut worker_exit: Option<ExitStatus> = None;

    match wait_for_socket(&socket_path, &mut worker, STARTUP_TIMEOUT) {
        Ok(()) => {
            if let Err(err) =
                run_inference_loop(&root, &socket_path, &mut success_count, &mut attempts)
            {
                infer_error = Some(err);
            }
        }
        Err(err) => {
            startup_error = Some(err);
        }
    }

    match shutdown_worker(&mut worker) {
        Ok(status) => worker_exit = Some(status),
        Err(err) => {
            shutdown_error = Some(err);
        }
    }

    let _ = stop_process(&mut cp_stub);

    let telemetry_bytes = dir_size(&telemetry_dir)?;
    let panic_detected = scan_for_panic(&worker_log_path)?;
    let (exit_code, exit_success) = match worker_exit {
        Some(status) => (status.code(), status.success()),
        None => (None, false),
    };

    Ok(WorkerGateRun {
        success_count,
        attempts,
        telemetry_bytes,
        exit_code,
        exit_success,
        panic_detected,
        socket_path,
        telemetry_dir,
        worker_log_path,
        manifest_path,
        model_path,
        tokenizer_path,
        startup_error,
        infer_error,
        shutdown_error,
    })
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn report_path() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("AOS_WORKER_GATE_REPORT") {
        return Ok(PathBuf::from(value));
    }
    Ok(repo_root().join("target/worker-gate/report.json"))
}

fn write_report(report: &GateReport) -> Result<PathBuf, String> {
    let path = report_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let payload = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    fs::write(&path, payload).map_err(|e| e.to_string())?;
    Ok(path)
}

fn build_binaries(root: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .current_dir(root)
        .arg("build")
        .arg("-p")
        .arg("adapteros-cli")
        .arg("-p")
        .arg("adapteros-lora-worker")
        .status()
        .map_err(|e| format!("failed to run cargo build: {}", e))?;
    if !status.success() {
        return Err(format!("cargo build failed with status {}", status));
    }
    Ok(())
}

fn spawn_cp_stub(root: &Path, log_path: &Path) -> Result<Child, String> {
    let log = File::create(log_path).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    Command::new("python3")
        .current_dir(root)
        .arg(root.join("var/cp_stub.py"))
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .map_err(|e| format!("failed to start CP stub: {}", e))
}

fn spawn_worker(
    root: &Path,
    log_path: &Path,
    manifest_path: &Path,
    model_path: &Path,
    tokenizer_path: &Path,
    socket_path: &Path,
    telemetry_dir: &Path,
) -> Result<Child, String> {
    let log = File::create(log_path).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;
    let worker_bin = bin_path(root, "aos-worker");

    Command::new(worker_bin)
        .current_dir(root)
        .arg("--uds-path")
        .arg(socket_path)
        .arg("--manifest")
        .arg(manifest_path)
        .arg("--model-path")
        .arg(model_path)
        .arg("--tokenizer")
        .arg(tokenizer_path)
        .arg("--backend")
        .arg("mock")
        .env("AOS_CP_URL", "http://127.0.0.1:9090")
        .env("AOS_MODEL_PATH", model_path)
        .env("AOS_TOKENIZER_PATH", tokenizer_path)
        .env("AOS_MODEL_CACHE_MAX_MB", "512")
        .env("AOS_PIN_BASE_MODEL", "1")
        .env("AOS_PIN_BUDGET_BYTES", "268435456")
        .env("AOS_TELEMETRY_DIR", telemetry_dir)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .spawn()
        .map_err(|e| format!("failed to start worker: {}", e))
}

fn wait_for_socket(path: &Path, worker: &mut Child, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if path.exists() {
            return Ok(());
        }
        if let Some(status) = worker
            .try_wait()
            .map_err(|e| format!("worker wait failed: {}", e))?
        {
            return Err(format!("worker exited before socket ready: {}", status));
        }
        if Instant::now() >= deadline {
            return Err(format!("timeout waiting for socket {}", path.display()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn run_inference_loop(
    root: &Path,
    socket_path: &Path,
    success_count: &mut usize,
    attempts: &mut usize,
) -> Result<(), String> {
    let aosctl_bin = bin_path(root, "aosctl");
    while *success_count < INFER_COUNT && *attempts < INFER_MAX_ATTEMPTS {
        *attempts += 1;
        let status = Command::new(&aosctl_bin)
            .current_dir(root)
            .arg("infer")
            .arg("--prompt")
            .arg(format!("worker-gate-{}", *attempts))
            .arg("--socket")
            .arg(socket_path)
            .arg("--max-tokens")
            .arg("8")
            .arg("--timeout")
            .arg("20000")
            .status()
            .map_err(|e| format!("failed to run aosctl infer: {}", e))?;

        if status.success() {
            *success_count += 1;
        } else {
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    Ok(())
}

fn shutdown_worker(worker: &mut Child) -> Result<ExitStatus, String> {
    if let Some(status) = worker
        .try_wait()
        .map_err(|e| format!("worker wait failed: {}", e))?
    {
        return Ok(status);
    }

    send_sigint(worker.id())?;
    wait_for_exit(worker, SHUTDOWN_TIMEOUT)
}

fn stop_process(child: &mut Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|e| format!("process wait failed: {}", e))?
        .is_some()
    {
        return Ok(());
    }

    child
        .kill()
        .map_err(|e| format!("failed to kill process: {}", e))?;
    Ok(())
}

fn send_sigint(pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .arg("-INT")
        .arg(pid.to_string())
        .status()
        .map_err(|e| format!("failed to send SIGINT: {}", e))?;
    if !status.success() {
        return Err(format!("kill -INT failed with status {}", status));
    }
    Ok(())
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("worker wait failed: {}", e))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            child.wait().ok();
            return Err(format!("worker did not exit within {:?}", timeout));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn bin_path(root: &Path, name: &str) -> PathBuf {
    let mut path = root.join("target").join("debug").join(name);
    if cfg!(windows) {
        path.set_extension("exe");
    }
    if path.exists() {
        return path;
    }

    let alt_name = if name.contains('-') {
        name.replace('-', "_")
    } else {
        name.replace('_', "-")
    };
    let mut alt_path = root.join("target").join("debug").join(alt_name);
    if cfg!(windows) {
        alt_path.set_extension("exe");
    }
    if alt_path.exists() {
        return alt_path;
    }
    path
}

fn dir_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    let entries = fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if meta.is_dir() {
            total += dir_size(&entry.path())?;
        } else {
            total += meta.len();
        }
    }
    Ok(total)
}

fn scan_for_panic(log_path: &Path) -> Result<bool, String> {
    let mut buf = String::new();
    if let Ok(mut file) = File::open(log_path) {
        let _ = file.read_to_string(&mut buf);
    }
    Ok(buf.contains("[PANIC HOOK]") || buf.contains("panicked at"))
}

fn find_telemetry_event(telemetry_dir: &Path, event_type: &str) -> Result<Option<Value>, String> {
    if !telemetry_dir.exists() {
        return Ok(None);
    }

    let entries = fs::read_dir(telemetry_dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("ndjson") {
            continue;
        }

        let file = File::open(&path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let event: Value = serde_json::from_str(trimmed).map_err(|e| {
                format!(
                    "failed to parse telemetry event in {}: {}",
                    path.display(),
                    e
                )
            })?;
            if event.get("event_type").and_then(|value| value.as_str()) == Some(event_type) {
                return Ok(Some(event));
            }
        }
    }

    Ok(None)
}

fn resolve_manifest_path(report_dir: &Path) -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("AOS_WORKER_GATE_MANIFEST") {
        return Ok(PathBuf::from(value));
    }

    let path = report_dir.join("manifest.json");
    write_json(&path, &gate_manifest_json())?;
    Ok(path)
}

fn resolve_model_paths(report_dir: &Path) -> Result<(PathBuf, PathBuf), String> {
    if let Ok(value) = std::env::var("AOS_WORKER_GATE_MODEL_PATH") {
        let model_path = PathBuf::from(value);
        let tokenizer_path = std::env::var("AOS_WORKER_GATE_TOKENIZER")
            .map(PathBuf::from)
            .unwrap_or_else(|_| model_path.join("tokenizer.json"));
        return Ok((model_path, tokenizer_path));
    }

    let model_dir = report_dir.join("model");
    if model_dir.exists() {
        fs::remove_dir_all(&model_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;

    let config_path = model_dir.join("config.json");
    write_json(&config_path, &gate_model_config_json())?;

    let tokenizer_path = model_dir.join("tokenizer.json");
    write_json(&tokenizer_path, &gate_tokenizer_json(DEFAULT_VOCAB_SIZE)?)?;

    Ok((model_dir, tokenizer_path))
}

fn gate_manifest_json() -> Value {
    json!({
        "schema": "adapteros.manifest.v3",
        "base": {
            "model_id": "worker-gate-mock",
            "model_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "arch": "qwen2",
            "vocab_size": DEFAULT_VOCAB_SIZE,
            "hidden_dim": DEFAULT_HIDDEN_DIM,
            "n_layers": DEFAULT_NUM_LAYERS,
            "n_heads": DEFAULT_NUM_HEADS,
            "config_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "tokenizer_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "tokenizer_cfg_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "license_hash": null,
            "rope_scaling_override": null
        },
        "adapters": [],
        "router": {
            "k_sparse": 1,
            "gate_quant": "q15",
            "entropy_floor": 0.02,
            "tau": 1.0,
            "sample_tokens_full": 32,
            "warmup": false,
            "algorithm": "weighted",
            "orthogonal_penalty": 0.0,
            "shared_downsample": false,
            "compression_ratio": 1.0,
            "multi_path_enabled": false,
            "diversity_threshold": 0.0,
            "orthogonal_constraints": false
        },
        "telemetry": {
            "schema_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "sampling": {
                "token": 1.0,
                "router": 1.0,
                "inference": 1.0
            },
            "router_full_tokens": 32,
            "bundle": {
                "max_events": 1000,
                "max_bytes": 1048576
            }
        },
        "policies": {
            "egress": {
                "mode": "allow_all",
                "serve_requires_pf": false,
                "allow_tcp": true,
                "allow_udp": true,
                "uds_paths": ["*"]
            },
            "determinism": {
                "require_metallib_embed": false,
                "require_kernel_hash_match": false,
                "rng": "hkdf_seeded",
                "retrieval_tie_break": ["score_desc"]
            },
            "evidence": {
                "require_open_book": false,
                "min_spans": 0,
                "prefer_latest_revision": false,
                "warn_on_superseded": false
            },
            "refusal": {
                "abstain_threshold": 1.0,
                "missing_fields_templates": {}
            },
            "numeric": {
                "canonical_units": {},
                "max_rounding_error": 100.0,
                "require_units_in_trace": false
            },
            "rag": {
                "index_scope": "per_tenant",
                "doc_tags_required": [],
                "embedding_model_hash": "0000000000000000000000000000000000000000000000000000000000000000",
                "topk": 3,
                "order": ["score_desc"]
            },
            "isolation": {
                "process_model": "shared",
                "uds_root": "var/run/aos",
                "forbid_shm": false
            },
            "performance": {
                "latency_p95_ms": 10000,
                "router_overhead_pct_max": 100,
                "throughput_tokens_per_s_min": 1,
                "max_tokens": 4096,
                "cpu_threshold_pct": 100.0,
                "memory_threshold_pct": 100.0,
                "circuit_breaker_threshold": 100
            },
            "memory": {
                "min_headroom_pct": 5,
                "evict_order": ["ephemeral_ttl"],
                "k_reduce_before_evict": false
            },
            "artifacts": {
                "require_signature": false,
                "require_sbom": false,
                "cas_only": false
            },
            "drift": {
                "os_build_tolerance": 999,
                "gpu_driver_tolerance": 999,
                "env_hash_tolerance": 999,
                "allow_warnings": true,
                "block_on_critical": false
            }
        },
        "seeds": {
            "global": "de00000000000000000000000000000000000000000000000000000000000000",
            "manifest_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "parent_cpid": null
        }
    })
}

fn gate_model_config_json() -> Value {
    json!({
        "model_type": "mock",
        "vocab_size": DEFAULT_VOCAB_SIZE,
        "hidden_size": DEFAULT_HIDDEN_DIM,
        "num_hidden_layers": DEFAULT_NUM_LAYERS,
        "num_attention_heads": DEFAULT_NUM_HEADS,
        "num_key_value_heads": DEFAULT_NUM_KV_HEADS,
        "intermediate_size": DEFAULT_INTERMEDIATE_SIZE,
        "max_position_embeddings": DEFAULT_MAX_SEQ_LEN,
        "rope_theta": 10000.0
    })
}

fn gate_tokenizer_json(vocab_size: usize) -> Result<Value, String> {
    if vocab_size < 2 {
        return Err("tokenizer vocab size must be at least 2".to_string());
    }

    let mut vocab = Map::new();
    vocab.insert("<unk>".to_string(), json!(0));
    for idx in 1..vocab_size {
        vocab.insert(format!("tok{}", idx), json!(idx));
    }

    Ok(json!({
        "version": "1.0",
        "truncation": null,
        "padding": null,
        "added_tokens": [],
        "normalizer": null,
        "pre_tokenizer": null,
        "post_processor": null,
        "decoder": null,
        "model": {
            "type": "BPE",
            "dropout": null,
            "unk_token": "<unk>",
            "continuing_subword_prefix": null,
            "end_of_word_suffix": null,
            "fuse_unk": false,
            "byte_fallback": false,
            "ignore_merges": false,
            "vocab": vocab,
            "merges": []
        }
    }))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, payload).map_err(|e| e.to_string())
}
