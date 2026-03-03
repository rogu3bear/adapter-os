use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

const DEFAULT_DETERMINISM_SEED: &str =
    "2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a";

const PROMPT_KEYWORDS: &[(&str, &[&str])] = &[
    ("fast", &["fast", "quick", "smoke"]),
    ("full", &["full"]),
    ("nightly", &["nightly"]),
    ("determinism", &["determinism", "seed", "replay"]),
    ("e2e", &["e2e", "gold"]),
    ("integration", &["integration"]),
    ("unit", &["unit"]),
    ("compile", &["compile"]),
    ("stress", &["stress"]),
    ("streaming", &["streaming"]),
    ("dual_write", &["dual-write", "dual_write"]),
    ("replay", &["replay"]),
];

const CAPABILITY_ENV_VARS: &[&str] = &[
    "AOS_DATABASE_URL",
    "AOS_SERVER_PORT",
    "AOS_TOKEN",
    "AOS_AUTH_TOKEN",
    "AOS_ADMIN_TOKEN",
    "AOS_JWT_TOKEN",
    "AOS_TOKENIZER_PATH",
    "AOS_E2E_MODEL_PATH",
    "AOS_WORKER_MODEL",
    "TEST_MLX_MODEL_PATH",
    "MLX_INCLUDE_DIR",
    "MLX_LIB_DIR",
];

const THREADS: &[(&str, u64)] = &[
    ("unit", 8),
    ("integration", 4),
    ("e2e", 2),
    ("replay", 1),
    ("streaming", 2),
];

const TIMEOUTS_S: &[(&str, u64)] = &[
    ("compile", 900),
    ("unit", 600),
    ("integration", 1200),
    ("e2e", 1800),
    ("determinism", 900),
    ("replay", 1200),
    ("streaming", 600),
    ("dual_write", 900),
    ("stress", 1800),
];

#[derive(Parser, Debug)]
pub struct TestOrchestratorArgs {
    #[arg(long, default_value = "", help = "Prompt to guide test selection")]
    prompt: String,
    #[arg(
        long,
        default_value = "",
        help = "Comma-separated cargo features to allow"
    )]
    features: String,
    #[arg(long, help = "Execute planned tests")]
    run: bool,
    #[arg(
        long,
        default_value = "var/reports/test-orchestrator",
        help = "Report output directory"
    )]
    report_dir: String,
    #[arg(long, help = "Skip doc drift scan")]
    skip_doc_scan: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TestEntry {
    id: String,
    name: String,
    file: String,
    kind: String,
    features: Vec<String>,
    ignore_reason: Option<String>,
    ignore_tracking_id: Option<String>,
    prereqs: Vec<String>,
    timeout_s: u64,
    trust: String,
    owner: String,
}

#[derive(Debug, Clone, Serialize)]
struct Manifest {
    generated_at: String,
    tests: Vec<TestEntry>,
    capabilities: Capabilities,
}

#[derive(Debug, Clone, Serialize)]
struct Capabilities {
    os: String,
    arch: String,
    dotenv_present: bool,
    env_vars: Vec<String>,
    server_running: bool,
    metal: bool,
    mlx: bool,
    db: DbCapability,
    tokens: Vec<String>,
    model_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DbCapability {
    url_set: bool,
    reachable: Option<bool>,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChangeScope {
    files: Vec<String>,
    areas: Vec<String>,
}

#[derive(Debug, Clone)]
struct TestSelection {
    test: TestEntry,
    runnable: bool,
    selected: bool,
    selection_reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct Summary {
    total: usize,
    runnable: usize,
    selected: usize,
    ignored: usize,
    stubs: usize,
}

#[derive(Debug, Clone, Serialize)]
struct SelectionRecord {
    id: String,
    file: String,
    kind: String,
    selected: bool,
    runnable: bool,
    selection_reason: String,
    trust: String,
    ignore_reason: Option<String>,
    ignore_tracking_id: Option<String>,
    prereqs: Vec<String>,
    features: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PlanStep {
    tier: String,
    name: String,
    command: Vec<String>,
    timeout_s: u64,
    env: BTreeMap<String, String>,
    meta: serde_json::Value,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct RunResult {
    tier: String,
    name: String,
    command: String,
    status: String,
    exit_code: i32,
    timed_out: bool,
    duration_s: f64,
    failed_tests: Vec<String>,
    reruns: Vec<RerunCommand>,
}

#[derive(Debug, Clone, Serialize)]
struct RerunCommand {
    reason: String,
    env: BTreeMap<String, String>,
    command: String,
}

#[derive(Debug, Clone, Serialize)]
struct TestDebt {
    ignored_tests_count: usize,
    ignored_missing_tracking_count: usize,
    stub_tests_count: usize,
    blocked_tests_count: usize,
    ignored_missing_tracking: Vec<String>,
    stub_tests: Vec<String>,
    blocked_tests: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DocDrift {
    missing_files: Vec<DocDriftItem>,
    missing_targets: Vec<DocDriftTarget>,
    empty_files: Vec<DocDriftItem>,
}

#[derive(Debug, Clone, Serialize)]
struct DocDriftItem {
    doc: String,
    reference: String,
}

#[derive(Debug, Clone, Serialize)]
struct DocDriftTarget {
    doc: String,
    reference: String,
    expected_path: String,
}

#[derive(Debug, Clone)]
struct CommandResult {
    exit_code: i32,
    timed_out: bool,
    duration_s: f64,
    output: String,
}

pub fn run(args: TestOrchestratorArgs) -> Result<()> {
    let root = repo_root();
    let prompt_tags = parse_prompt(&args.prompt);
    let allowed_features = parse_features(&args.features);

    let inventory = scan_tests(&root)?;
    let capabilities = detect_capabilities(&root)?;
    let change_scope = detect_change_scope(&root)?;
    let selection = build_selection(
        &inventory,
        &capabilities,
        &allowed_features,
        &change_scope,
        &prompt_tags,
    );
    let plan = build_plan(&selection, &prompt_tags, &change_scope);

    let doc_drift = if args.skip_doc_scan {
        DocDrift {
            missing_files: Vec::new(),
            missing_targets: Vec::new(),
            empty_files: Vec::new(),
        }
    } else {
        scan_docs(&root, &inventory)?
    };

    let base_env = build_base_env();
    let (results, reruns) = if args.run {
        run_plan(&root, &plan, &base_env)?
    } else {
        (Vec::new(), Vec::new())
    };

    let generated_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let manifest = Manifest {
        generated_at: generated_at.clone(),
        tests: inventory.clone(),
        capabilities: capabilities.clone(),
    };

    let summary = summarize_selection(&selection);
    let debt = build_debt(&selection);
    let plan_len = plan.len();
    let mut prompt_tags_list: Vec<String> = prompt_tags.iter().cloned().collect();
    prompt_tags_list.sort();
    let mut features_list: Vec<String> = allowed_features.iter().cloned().collect();
    features_list.sort();
    let selection_records: Vec<SelectionRecord> = selection
        .iter()
        .map(|item| SelectionRecord {
            id: item.test.id.clone(),
            file: item.test.file.clone(),
            kind: item.test.kind.clone(),
            selected: item.selected,
            runnable: item.runnable,
            selection_reason: item.selection_reason.clone(),
            trust: item.test.trust.clone(),
            ignore_reason: item.test.ignore_reason.clone(),
            ignore_tracking_id: item.test.ignore_tracking_id.clone(),
            prereqs: item.test.prereqs.clone(),
            features: item.test.features.clone(),
        })
        .collect();

    let report = json!({
        "generated_at": generated_at,
        "repo_root": root.to_string_lossy(),
        "prompt": args.prompt,
        "prompt_tags": prompt_tags_list,
        "features": features_list,
        "change_scope": change_scope,
        "summary": summary,
        "capabilities": capabilities,
        "plan": plan,
        "selection": selection_records,
        "results": results,
        "rerun_commands": reruns,
        "test_debt": &debt,
        "doc_drift": &doc_drift,
    });

    let report_dir = root.join(args.report_dir);
    write_json(&report_dir.join("manifest.json"), &manifest)?;
    write_json(&report_dir.join("report.json"), &report)?;

    print_summary(
        &summary,
        &manifest.capabilities,
        report_dir.join("report.json"),
        &debt,
        &doc_drift,
        plan_len,
        args.run,
        &results,
    );

    if args.run {
        let failed = results.iter().any(|res| res.status != "passed");
        if failed {
            std::process::exit(1);
        }
    }

    Ok(())
}

fn repo_root() -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output();
    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                let trimmed = path.trim();
                if !trimmed.is_empty() {
                    return PathBuf::from(trimmed);
                }
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn parse_prompt(prompt: &str) -> HashSet<String> {
    let token_re = Regex::new(r"[A-Za-z0-9_\-]+").unwrap();
    let tokens: HashSet<String> = token_re
        .find_iter(&prompt.to_lowercase())
        .map(|m| m.as_str().to_string())
        .collect();
    let mut found = HashSet::new();
    for (key, keywords) in PROMPT_KEYWORDS {
        if keywords.iter().any(|kw| tokens.contains(*kw)) {
            found.insert((*key).to_string());
        }
    }
    found
}

fn parse_features(features: &str) -> HashSet<String> {
    features
        .split(',')
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn build_base_env() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    env.insert("AOS_DEV_NO_AUTH".to_string(), "1".to_string());
    env.insert("AOS_BACKEND".to_string(), "mock".to_string());
    env.insert("RUST_BACKTRACE".to_string(), "1".to_string());
    env.insert(
        "RUST_LOG".to_string(),
        "adapteros=debug,tower_http=warn".to_string(),
    );
    env.insert("AOS_ALLOW_LEGACY_AOS".to_string(), "0".to_string());
    env
}

fn collect_test_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let tests_dir = root.join("tests");
    if tests_dir.exists() {
        for entry in WalkDir::new(&tests_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if entry.file_type().is_file() && is_rust_file(entry.path()) {
                files.push(entry.path().to_path_buf());
            }
        }
    }

    let crates_dir = root.join("crates");
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let tests_path = path.join("tests");
                if tests_path.exists() {
                    for entry in WalkDir::new(&tests_path)
                        .into_iter()
                        .filter_map(|entry| entry.ok())
                    {
                        if entry.file_type().is_file() && is_rust_file(entry.path()) {
                            files.push(entry.path().to_path_buf());
                        }
                    }
                }
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

fn collect_doc_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let docs_dir = root.join("docs");
    let tests_dir = root.join("tests");
    if docs_dir.exists() {
        files.extend(collect_md_files(&docs_dir));
    }
    if tests_dir.exists() {
        files.extend(collect_md_files(&tests_dir));
    }

    let crates_dir = root.join("crates");
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let path = entry.path().join("tests");
            if path.exists() {
                files.extend(collect_md_files(&path));
            }
        }
    }

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(path);
            }
        }
    }

    files.sort();
    files.dedup();
    files
}

fn collect_md_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.path().to_path_buf())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect()
}

fn is_rust_file(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("rs")
}

fn is_stub_file(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() >= 15 {
        return false;
    }
    if !text.contains("#[ignore") {
        return false;
    }
    let stub_re = Regex::new(r"fn\s+test\w*\s*\([^)]*\)\s*\{\s*\}").unwrap();
    stub_re.is_match(text)
}

fn extract_features(attrs: &[String], feature_re: &Regex) -> BTreeSet<String> {
    let mut features = BTreeSet::new();
    for line in attrs {
        for cap in feature_re.captures_iter(line) {
            if let Some(value) = cap.get(1) {
                features.insert(value.as_str().to_string());
            }
        }
    }
    features
}

fn has_test_attr(attrs: &[String], patterns: &[Regex]) -> bool {
    attrs
        .iter()
        .any(|line| patterns.iter().any(|pattern| pattern.is_match(line)))
}

fn extract_ignore_reason(attrs: &[String], ignore_re: &Regex) -> Option<String> {
    for line in attrs {
        if let Some(cap) = ignore_re.captures(line) {
            return cap
                .get(1)
                .map(|value| value.as_str().to_string())
                .or(Some("ignored".to_string()));
        }
    }
    None
}

fn classify_kind(rel_path: &str, ignore_reason: &str, features: &BTreeSet<String>) -> String {
    let lowered = rel_path.to_lowercase();
    if lowered.contains("determinism") || lowered.contains("seed") {
        return "determinism".to_string();
    }
    if lowered.contains("replay") {
        return "replay".to_string();
    }
    if lowered.contains("streaming") {
        return "streaming".to_string();
    }
    if lowered.contains("stress") {
        return "stress".to_string();
    }
    if lowered.contains("e2e") || lowered.contains("gold") {
        return "e2e".to_string();
    }
    if features.iter().any(|feat| feat.contains("determinism")) {
        return "determinism".to_string();
    }
    if ignore_reason.to_lowercase().contains("determinism") {
        return "determinism".to_string();
    }
    "integration".to_string()
}

fn infer_prereqs(rel_path: &str, ignore_reason: &str, features: &BTreeSet<String>) -> Vec<String> {
    let mut prereqs = BTreeSet::new();
    let lowered = rel_path.to_lowercase();
    let reason = ignore_reason.to_lowercase();

    if reason.contains("database") || reason.contains("sqlite") || reason.contains("db") {
        prereqs.insert("AOS_DATABASE_URL".to_string());
    }
    if reason.contains("server") || reason.contains("control plane") || reason.contains("readyz") {
        prereqs.insert("RUNNING_SERVER".to_string());
    }
    if reason.contains("token") || reason.contains("auth") {
        prereqs.insert("TOKEN".to_string());
    }
    if reason.contains("tokenizer")
        || reason.contains("model files")
        || reason.contains("model file")
    {
        prereqs.insert("TOKENIZER_MODEL".to_string());
    }
    if reason.contains("metal")
        || reason.contains("coreml")
        || reason.contains("hardware-residency")
    {
        prereqs.insert("METAL".to_string());
    }
    if reason.contains("mlx") {
        prereqs.insert("MLX".to_string());
    }
    if reason.contains("worker") || reason.contains("uds") {
        prereqs.insert("WORKER".to_string());
    }

    if lowered.contains("tests/e2e") || lowered.contains("/e2e") {
        prereqs.insert("RUNNING_SERVER".to_string());
    }
    if lowered.contains("metal") || lowered.contains("coreml") {
        prereqs.insert("METAL".to_string());
    }
    if lowered.contains("mlx") {
        prereqs.insert("MLX".to_string());
    }

    for feature in features {
        if feature.contains("mlx") {
            prereqs.insert("MLX".to_string());
        }
        if feature.contains("coreml")
            || feature.contains("metal")
            || feature.contains("hardware-residency")
        {
            prereqs.insert("METAL".to_string());
        }
    }

    prereqs.into_iter().collect()
}

fn compute_owner(rel_path: &str) -> String {
    let parts: Vec<&str> = rel_path.split('/').collect();
    if parts.first() == Some(&"tests") {
        if parts.len() > 1 {
            return format!("tests/{}", parts[1]);
        }
        return "tests".to_string();
    }
    if parts.first() == Some(&"crates") && parts.len() > 1 {
        return format!("crates/{}", parts[1]);
    }
    parts.first().unwrap_or(&"unknown").to_string()
}

fn scan_tests(root: &Path) -> Result<Vec<TestEntry>> {
    let feature_re = Regex::new(r#"feature\s*=\s*"([^"]+)""#).unwrap();
    let ignore_re = Regex::new(r#"#\s*\[\s*ignore(?:\s*=\s*"([^"]*)")?\s*\]"#).unwrap();
    let tracking_re = Regex::new(r"(?i)\btracking:\s*([A-Z]+-\w+)\b").unwrap();
    let fn_re = Regex::new(r"\bfn\s+([A-Za-z0-9_]+)\b").unwrap();
    let test_attr_patterns = vec![
        Regex::new(r"#\[\s*test\s*\]").unwrap(),
        Regex::new(r"#\[\s*tokio::test").unwrap(),
        Regex::new(r"#\[\s*async_std::test").unwrap(),
        Regex::new(r"#\[\s*rstest").unwrap(),
        Regex::new(r"#\[\s*traced_test").unwrap(),
    ];

    let mut tests = Vec::new();
    for path in collect_test_files(root) {
        let rel_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(&path).unwrap_or_default();
        let stub = is_stub_file(&text);
        let mut file_attrs = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("#![") || trimmed.starts_with("#[") {
                file_attrs.push(trimmed.to_string());
                continue;
            }
            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                break;
            }
        }

        let file_features = extract_features(&file_attrs, &feature_re);
        let mut pending_attrs: Vec<String> = Vec::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("#[") {
                pending_attrs.push(trimmed.to_string());
                continue;
            }

            if trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub async fn ")
            {
                if has_test_attr(&pending_attrs, &test_attr_patterns) {
                    if let Some(cap) = fn_re.captures(trimmed) {
                        let name = cap.get(1).map(|m| m.as_str()).unwrap_or("unknown");
                        let mut features = file_features.clone();
                        features.extend(extract_features(&pending_attrs, &feature_re));
                        let ignore_reason = extract_ignore_reason(&pending_attrs, &ignore_re);
                        let tracking_id = ignore_reason
                            .as_ref()
                            .and_then(|reason| tracking_re.captures(reason))
                            .and_then(|cap| cap.get(1))
                            .map(|cap| cap.as_str().to_string());
                        let kind = classify_kind(
                            &rel_path,
                            ignore_reason.as_deref().unwrap_or(""),
                            &features,
                        );
                        let prereqs = infer_prereqs(
                            &rel_path,
                            ignore_reason.as_deref().unwrap_or(""),
                            &features,
                        );
                        let trust = if stub {
                            "stub"
                        } else if ignore_reason.is_some() {
                            "ignored"
                        } else {
                            "real"
                        };
                        let timeout_s = timeout_for_kind(&kind);
                        tests.push(TestEntry {
                            id: format!("{rel_path}::{name}"),
                            name: name.to_string(),
                            file: rel_path.clone(),
                            kind,
                            features: features.into_iter().collect(),
                            ignore_reason,
                            ignore_tracking_id: tracking_id,
                            prereqs,
                            timeout_s,
                            trust: trust.to_string(),
                            owner: compute_owner(&rel_path),
                        });
                    }
                }
                pending_attrs.clear();
                continue;
            }

            if !trimmed.is_empty() && !trimmed.starts_with("//") {
                pending_attrs.clear();
            }
        }
    }

    tests.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tests)
}

fn scan_docs(root: &Path, inventory: &[TestEntry]) -> Result<DocDrift> {
    let doc_test_cmd_re = Regex::new(r"cargo\s+test[^\n]*--test\s+([A-Za-z0-9_\-]+)").unwrap();
    let doc_package_re = Regex::new(r"(?:-p|--package)\s+([A-Za-z0-9_\-]+)").unwrap();
    let doc_path_re = Regex::new(r"(?P<path>(?:tests|crates/[^/\s]+/tests)/[^\s`]+?\.rs)").unwrap();
    let doc_abs_path_re =
        Regex::new(r"(?P<path>/(?:[^ \t\r\n]+/)*(?:tests|crates/[^/\s]+/tests)/[^\s`]+?\.rs)")
            .unwrap();

    let mut file_index: HashMap<String, Vec<&TestEntry>> = HashMap::new();
    for test in inventory {
        file_index.entry(test.file.clone()).or_default().push(test);
    }

    let mut referenced_paths = Vec::new();
    let mut referenced_targets = Vec::new();

    for doc_path in collect_doc_files(root) {
        let content = fs::read_to_string(&doc_path).unwrap_or_default();
        for cap in doc_path_re.captures_iter(&content) {
            if let Some(path) = cap.name("path") {
                referenced_paths.push((doc_path.clone(), path.as_str().to_string()));
            }
        }
        for cap in doc_abs_path_re.captures_iter(&content) {
            if let Some(path) = cap.name("path") {
                referenced_paths.push((doc_path.clone(), path.as_str().to_string()));
            }
        }

        for line in content.lines() {
            if !line.contains("cargo test") || !line.contains("--test") {
                continue;
            }
            if let Some(cap) = doc_test_cmd_re.captures(line) {
                let test_name = cap
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let pkg = doc_package_re
                    .captures(line)
                    .and_then(|cap| cap.get(1))
                    .map(|m| m.as_str().to_string());
                referenced_targets.push((doc_path.clone(), test_name, pkg));
            }
        }
    }

    let mut missing_files = Vec::new();
    let mut missing_targets = Vec::new();
    let mut empty_files = Vec::new();

    for (doc_path, raw_path) in referenced_paths {
        let normalized = match normalize_doc_path(root, &raw_path) {
            Some(path) => path,
            None => continue,
        };
        if Path::new(&normalized).is_absolute() {
            continue;
        }
        if !root.join(&normalized).exists() {
            missing_files.push(DocDriftItem {
                doc: doc_path
                    .strip_prefix(root)
                    .unwrap_or(&doc_path)
                    .to_string_lossy()
                    .to_string(),
                reference: normalized,
            });
        } else if !file_index.contains_key(&normalized) {
            empty_files.push(DocDriftItem {
                doc: doc_path
                    .strip_prefix(root)
                    .unwrap_or(&doc_path)
                    .to_string_lossy()
                    .to_string(),
                reference: normalized,
            });
        }
    }

    for (doc_path, test_name, pkg) in referenced_targets {
        let expected = if let Some(pkg) = pkg {
            format!("crates/{pkg}/tests/{test_name}.rs")
        } else {
            format!("tests/{test_name}.rs")
        };
        if !root.join(&expected).exists() {
            missing_targets.push(DocDriftTarget {
                doc: doc_path
                    .strip_prefix(root)
                    .unwrap_or(&doc_path)
                    .to_string_lossy()
                    .to_string(),
                reference: format!("cargo test --test {test_name}"),
                expected_path: expected,
            });
        }
    }

    missing_files.sort_by(|a, b| {
        (a.doc.clone(), a.reference.clone()).cmp(&(b.doc.clone(), b.reference.clone()))
    });
    missing_targets.sort_by(|a, b| {
        (a.doc.clone(), a.reference.clone()).cmp(&(b.doc.clone(), b.reference.clone()))
    });
    empty_files.sort_by(|a, b| {
        (a.doc.clone(), a.reference.clone()).cmp(&(b.doc.clone(), b.reference.clone()))
    });

    Ok(DocDrift {
        missing_files,
        missing_targets,
        empty_files,
    })
}

fn normalize_doc_path(root: &Path, path_str: &str) -> Option<String> {
    if path_str.starts_with(&root.to_string_lossy().to_string()) {
        let rel = Path::new(path_str)
            .strip_prefix(root)
            .ok()?
            .to_string_lossy()
            .to_string();
        return Some(rel);
    }
    if let Some(pos) = path_str.find("/crates/") {
        return Some(format!("crates/{}", &path_str[pos + "/crates/".len()..]));
    }
    if let Some(pos) = path_str.find("/tests/") {
        return Some(format!("tests/{}", &path_str[pos + "/tests/".len()..]));
    }
    let cleaned = path_str.trim_start_matches('/');
    if cleaned.starts_with("tests/") || cleaned.starts_with("crates/") {
        return Some(cleaned.to_string());
    }
    if Path::new(path_str).is_absolute() {
        return None;
    }
    Some(path_str.to_string())
}

fn detect_capabilities(root: &Path) -> Result<Capabilities> {
    let env_vars: Vec<String> = CAPABILITY_ENV_VARS
        .iter()
        .filter_map(|key| std::env::var(key).ok().map(|_| key.to_string()))
        .collect();

    let tokens = [
        "AOS_TOKEN",
        "AOS_AUTH_TOKEN",
        "AOS_ADMIN_TOKEN",
        "AOS_JWT_TOKEN",
    ]
    .iter()
    .filter_map(|key| std::env::var(key).ok().map(|_| key.to_string()))
    .collect();

    let model_paths = [
        "AOS_TOKENIZER_PATH",
        "AOS_E2E_MODEL_PATH",
        "AOS_WORKER_MODEL",
        "TEST_MLX_MODEL_PATH",
    ]
    .iter()
    .filter_map(|key| std::env::var(key).ok().map(|_| key.to_string()))
    .collect();

    let db_url = std::env::var("AOS_DATABASE_URL").ok();
    let mut db = DbCapability {
        url_set: false,
        reachable: None,
        detail: String::new(),
    };
    if let Some(url) = db_url {
        db.url_set = true;
        if url.starts_with("sqlite:") {
            let mut path = url.trim_start_matches("sqlite:").to_string();
            if path.starts_with("//") {
                path = path.trim_start_matches('/').to_string();
            }
            let file = PathBuf::from(path);
            if file.exists() {
                let readable = fs::File::open(&file).is_ok();
                db.reachable = Some(readable);
                db.detail = "sqlite".to_string();
            } else {
                db.reachable = Some(false);
                db.detail = "sqlite file missing".to_string();
            }
        } else {
            db.reachable = None;
            db.detail = "non-sqlite".to_string();
        }
    }

    let metal = cfg!(target_os = "macos")
        && Path::new("/System/Library/Frameworks/Metal.framework").exists();

    let mlx = check_mlx_env();

    let server_running = check_readyz();

    Ok(Capabilities {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        dotenv_present: root.join(".env").exists(),
        env_vars,
        server_running,
        metal,
        mlx,
        db,
        tokens,
        model_paths,
    })
}

fn check_mlx_env() -> bool {
    let include_dir = std::env::var("MLX_INCLUDE_DIR").ok();
    let lib_dir = std::env::var("MLX_LIB_DIR").ok();
    if include_dir.is_none() || lib_dir.is_none() {
        return false;
    }
    let include_ok = Path::new(include_dir.as_ref().unwrap())
        .join("mlx")
        .exists();
    let lib_ok = fs::read_dir(lib_dir.unwrap())
        .map(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with("libmlx."))
        })
        .unwrap_or(false);
    include_ok && lib_ok
}

fn check_readyz() -> bool {
    let port = std::env::var("AOS_SERVER_PORT")
        .ok()
        .or_else(|| {
            std::env::var("AOS_PORT_PANE_BASE")
                .ok()
                .and_then(|raw| raw.parse::<u16>().ok())
                .filter(|base| *base > 0 && *base <= 65523)
                .map(|base| base.to_string())
        })
        .unwrap_or_else(|| "18080".to_string());
    let addr = format!("127.0.0.1:{port}");
    let mut stream = match std::net::TcpStream::connect(addr) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let request = b"GET /readyz HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n";
    if stream.write_all(request).is_err() {
        return false;
    }
    let mut buffer = [0u8; 256];
    let count = match stream.read(&mut buffer) {
        Ok(count) => count,
        Err(_) => return false,
    };
    let response = String::from_utf8_lossy(&buffer[..count]);
    response.starts_with("HTTP/1.1 200") || response.starts_with("HTTP/1.0 200")
}

fn detect_change_scope(root: &Path) -> Result<ChangeScope> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output();
    let mut files = Vec::new();
    if let Ok(output) = output {
        if output.status.success() {
            let content = String::from_utf8_lossy(&output.stdout);
            for line in content.lines() {
                if line.len() > 3 {
                    let path = line[3..].trim();
                    if !path.is_empty() {
                        files.push(path.to_string());
                    }
                }
            }
        }
    }
    files.sort();
    files.dedup();

    let area_rules: Vec<(&str, Vec<&str>)> = vec![
        (
            "determinism",
            vec![
                "crates/adapteros-core",
                "crates/adapteros-lora-router",
                "crates/adapteros-lora-kernel",
                "metal/",
                "seeds/",
                "run_replay_determinism_tests.sh",
            ],
        ),
        (
            "db",
            vec!["crates/adapteros-db", "migrations/", "scripts/db/"],
        ),
        (
            "server",
            vec![
                "crates/adapteros-server",
                "crates/adapteros-server-api",
                "config/",
                "configs/",
            ],
        ),
        ("ui", vec!["crates/adapteros-ui", "static/"]),
        (
            "worker",
            vec![
                "crates/adapteros-lora-worker",
                "crates/adapteros-lora-mlx-ffi",
                "crates/adapteros-lora-kernel",
                "metal/",
            ],
        ),
        ("cli", vec!["crates/adapteros-cli", "aosctl"]),
        ("tui", vec!["crates/adapteros-tui"]),
        ("orchestrator", vec!["crates/adapteros-orchestrator"]),
    ];

    let mut areas = BTreeSet::new();
    for file in &files {
        for (area, prefixes) in &area_rules {
            if prefixes.iter().any(|prefix| file.starts_with(prefix)) {
                areas.insert((*area).to_string());
            }
        }
    }

    Ok(ChangeScope {
        files,
        areas: areas.into_iter().collect(),
    })
}

fn build_selection(
    inventory: &[TestEntry],
    caps: &Capabilities,
    allowed_features: &HashSet<String>,
    change_scope: &ChangeScope,
    prompt_tags: &HashSet<String>,
) -> Vec<TestSelection> {
    let changed_files: HashSet<String> = change_scope
        .files
        .iter()
        .filter(|file| file.ends_with(".rs"))
        .cloned()
        .collect();
    let mut changed_crates = HashSet::new();
    for path in &change_scope.files {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 2 && parts[0] == "crates" {
            changed_crates.insert(parts[1].to_string());
        }
    }

    let mut selected_kinds = HashSet::new();
    let force_integration = prompt_tags.contains("integration")
        || prompt_tags.contains("full")
        || prompt_tags.contains("nightly");
    if prompt_tags.contains("determinism") {
        selected_kinds.insert("determinism");
    }
    if prompt_tags.contains("e2e") {
        selected_kinds.insert("e2e");
    }
    if prompt_tags.contains("streaming") {
        selected_kinds.insert("streaming");
    }
    if prompt_tags.contains("replay") {
        selected_kinds.insert("replay");
    }
    if prompt_tags.contains("stress") {
        selected_kinds.insert("stress");
    }

    let mut selections = Vec::new();
    for test in inventory {
        let mut reasons = Vec::new();
        let mut runnable = true;

        if test.trust == "stub" {
            runnable = false;
            reasons.push("stub test file".to_string());
        }
        if test.ignore_reason.is_some() && test.trust == "ignored" {
            runnable = false;
            reasons.push("ignored in code".to_string());
        }

        let missing_features: Vec<String> = test
            .features
            .iter()
            .filter(|feat| !allowed_features.contains(*feat))
            .cloned()
            .collect();
        if !missing_features.is_empty() {
            runnable = false;
            reasons.push(format!("missing features: {}", missing_features.join(", ")));
        }

        let missing_caps: Vec<String> = test
            .prereqs
            .iter()
            .filter(|cap| !capability_ok(cap, caps))
            .cloned()
            .collect();
        if !missing_caps.is_empty() {
            runnable = false;
            reasons.push(format!("missing capabilities: {}", missing_caps.join(", ")));
        }

        let mut selected = false;
        if runnable {
            if changed_files.contains(&test.file) {
                selected = true;
                reasons.push("changed test file".to_string());
            } else if test.file.starts_with("crates/") {
                let crate_name = test.file.split('/').nth(1).unwrap_or("");
                if changed_crates.contains(crate_name) {
                    selected = true;
                    reasons.push("changed crate".to_string());
                }
            }
            if !selected && force_integration && test.kind == "integration" {
                selected = true;
                reasons.push("prompt tag: integration".to_string());
            }
            if !selected && selected_kinds.contains(test.kind.as_str()) {
                selected = true;
                reasons.push(format!("prompt tag: {}", test.kind));
            }
        }

        if !selected && runnable {
            reasons.push("not in change scope".to_string());
        }

        selections.push(TestSelection {
            test: test.clone(),
            runnable,
            selected,
            selection_reason: if reasons.is_empty() {
                "not selected".to_string()
            } else {
                reasons.join("; ")
            },
        });
    }
    selections
}

fn build_plan(
    selection: &[TestSelection],
    prompt_tags: &HashSet<String>,
    change_scope: &ChangeScope,
) -> Vec<PlanStep> {
    let change_areas: HashSet<String> = change_scope.areas.iter().cloned().collect();
    let mut plan = Vec::new();

    let mut include_compile = true;
    let mut include_unit = true;
    let mut include_integration = true;
    if !prompt_tags.is_empty() {
        include_unit = prompt_tags.contains("unit")
            || prompt_tags.contains("fast")
            || prompt_tags.contains("full")
            || prompt_tags.contains("nightly");
        include_integration = prompt_tags.contains("integration")
            || prompt_tags.contains("full")
            || prompt_tags.contains("nightly");
    }

    let mut include_determinism =
        prompt_tags.contains("determinism") || change_areas.contains("determinism");
    let mut include_e2e = prompt_tags.contains("e2e");
    let mut include_replay = prompt_tags.contains("replay");
    let mut include_streaming = prompt_tags.contains("streaming");
    let mut include_stress = prompt_tags.contains("stress");
    let mut include_dual_write = prompt_tags.contains("dual_write");

    if prompt_tags.contains("fast") {
        include_integration = false;
        include_determinism = false;
        include_e2e = false;
        include_replay = false;
        include_streaming = false;
        include_stress = false;
        include_dual_write = false;
    }

    if prompt_tags.contains("full") || prompt_tags.contains("nightly") {
        include_compile = true;
        include_unit = true;
        include_integration = true;
        include_determinism = true;
        include_e2e = true;
    }
    if prompt_tags.contains("nightly") {
        include_replay = true;
        include_streaming = true;
        include_stress = true;
        include_dual_write = true;
    }

    if include_compile {
        plan.push(PlanStep {
            tier: "compile".to_string(),
            name: "Compile gate".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--workspace".to_string(),
                "--no-run".to_string(),
            ],
            timeout_s: timeout_for_kind("compile"),
            env: BTreeMap::new(),
            meta: json!({}),
            reason: "baseline compile validation".to_string(),
        });
    }

    if include_unit {
        plan.push(PlanStep {
            tier: "unit".to_string(),
            name: "Fast unit tests".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--workspace".to_string(),
                "--lib".to_string(),
                "--".to_string(),
                format!("--test-threads={}", threads_for_kind("unit")),
            ],
            timeout_s: timeout_for_kind("unit"),
            env: BTreeMap::new(),
            meta: json!({}),
            reason: "fast unit coverage".to_string(),
        });
    }

    if include_integration {
        plan.extend(build_integration_plan(selection));
    }

    if include_e2e {
        let mut env = BTreeMap::new();
        env.insert(
            "AOS_DETERMINISM_SEED".to_string(),
            DEFAULT_DETERMINISM_SEED.to_string(),
        );
        plan.push(PlanStep {
            tier: "e2e".to_string(),
            name: "Gold standard E2E".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--test".to_string(),
                "gold_standard_e2e".to_string(),
                "--".to_string(),
                format!("--test-threads={}", threads_for_kind("e2e")),
                "--nocapture".to_string(),
            ],
            timeout_s: timeout_for_kind("e2e"),
            env,
            meta: json!({"test_binary": "gold_standard_e2e"}),
            reason: "prompt tag or full/nightly suite".to_string(),
        });
    }

    if include_determinism {
        let mut env = BTreeMap::new();
        env.insert(
            "AOS_DETERMINISM_SEED".to_string(),
            DEFAULT_DETERMINISM_SEED.to_string(),
        );
        plan.push(PlanStep {
            tier: "determinism".to_string(),
            name: "Determinism suite".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--workspace".to_string(),
                "--".to_string(),
                format!("--test-threads={}", threads_for_kind("integration")),
                "determinism".to_string(),
            ],
            timeout_s: timeout_for_kind("determinism"),
            env,
            meta: json!({}),
            reason: "determinism-sensitive change or prompt tag".to_string(),
        });
    }

    if include_replay {
        let mut env = BTreeMap::new();
        env.insert(
            "AOS_DETERMINISM_SEED".to_string(),
            DEFAULT_DETERMINISM_SEED.to_string(),
        );
        plan.push(PlanStep {
            tier: "replay".to_string(),
            name: "Determinism replay harness".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--test".to_string(),
                "determinism_replay_harness".to_string(),
                "--".to_string(),
                format!("--test-threads={}", threads_for_kind("replay")),
                "--nocapture".to_string(),
            ],
            timeout_s: timeout_for_kind("replay"),
            env,
            meta: json!({"test_binary": "determinism_replay_harness"}),
            reason: "prompt tag or nightly suite".to_string(),
        });
    }

    if include_streaming {
        plan.push(PlanStep {
            tier: "streaming".to_string(),
            name: "Streaming reliability".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "adapteros-server-api".to_string(),
                "--test".to_string(),
                "streaming_reliability".to_string(),
                "--".to_string(),
                format!("--test-threads={}", threads_for_kind("streaming")),
                "--nocapture".to_string(),
            ],
            timeout_s: timeout_for_kind("streaming"),
            env: BTreeMap::new(),
            meta: json!({"test_binary": "streaming_reliability", "crate": "adapteros-server-api"}),
            reason: "prompt tag or nightly suite".to_string(),
        });
    }

    if include_dual_write {
        let mut env = BTreeMap::new();
        env.insert(
            "AOS_DETERMINISM_SEED".to_string(),
            DEFAULT_DETERMINISM_SEED.to_string(),
        );
        env.insert("AOS_STORAGE_BACKEND".to_string(), "dual_write".to_string());
        env.insert(
            "AOS_ATOMIC_DUAL_WRITE_STRICT".to_string(),
            "true".to_string(),
        );
        plan.push(PlanStep {
            tier: "dual_write".to_string(),
            name: "Dual-write drift tests".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "-p".to_string(),
                "adapteros-db".to_string(),
                "--test".to_string(),
                "atomic_dual_write_tests".to_string(),
                "--".to_string(),
                "--test-threads=2".to_string(),
                "--nocapture".to_string(),
            ],
            timeout_s: timeout_for_kind("dual_write"),
            env,
            meta: json!({"test_binary": "atomic_dual_write_tests", "crate": "adapteros-db"}),
            reason: "prompt tag or nightly suite".to_string(),
        });
    }

    if include_stress {
        let mut env = BTreeMap::new();
        env.insert(
            "AOS_DETERMINISM_SEED".to_string(),
            DEFAULT_DETERMINISM_SEED.to_string(),
        );
        plan.push(PlanStep {
            tier: "stress".to_string(),
            name: "Stress tests".to_string(),
            command: vec![
                "cargo".to_string(),
                "test".to_string(),
                "--workspace".to_string(),
                "--".to_string(),
                format!("--test-threads={}", threads_for_kind("integration")),
                "stress".to_string(),
            ],
            timeout_s: timeout_for_kind("stress"),
            env,
            meta: json!({}),
            reason: "prompt tag or nightly suite".to_string(),
        });
    }

    plan
}

fn build_integration_plan(selection: &[TestSelection]) -> Vec<PlanStep> {
    let selected_tests: Vec<&TestSelection> = selection
        .iter()
        .filter(|item| item.selected && item.runnable && item.test.kind == "integration")
        .collect();
    if selected_tests.is_empty() {
        return Vec::new();
    }

    let mut runnable_by_binary: HashMap<(String, Vec<String>), Vec<&TestSelection>> =
        HashMap::new();
    for item in selection
        .iter()
        .filter(|item| item.runnable && item.test.kind == "integration")
    {
        runnable_by_binary
            .entry((item.test.file.clone(), item.test.features.clone()))
            .or_default()
            .push(item);
    }

    let mut selected_by_binary: HashMap<(String, Vec<String>), Vec<&TestSelection>> =
        HashMap::new();
    for item in &selected_tests {
        selected_by_binary
            .entry((item.test.file.clone(), item.test.features.clone()))
            .or_default()
            .push(*item);
    }

    let mut plan = Vec::new();
    let mut binaries: Vec<(String, Vec<String>)> = selected_by_binary.keys().cloned().collect();
    binaries.sort_by(|a, b| a.0.cmp(&b.0));

    for (file_path, features) in binaries {
        let selected = selected_by_binary
            .get(&(file_path.clone(), features.clone()))
            .cloned()
            .unwrap_or_default();
        let runnable = runnable_by_binary
            .get(&(file_path.clone(), features.clone()))
            .cloned()
            .unwrap_or_default();

        let crate_name = if file_path.starts_with("crates/") {
            file_path.split('/').nth(1).map(|s| s.to_string())
        } else {
            None
        };
        let binary = Path::new(&file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let feature_args = if features.is_empty() {
            Vec::new()
        } else {
            vec!["--features".to_string(), features.join(",")]
        };

        if selected.len() == runnable.len() {
            let mut command = vec!["cargo".to_string(), "test".to_string()];
            if let Some(crate_name) = &crate_name {
                command.push("-p".to_string());
                command.push(crate_name.to_string());
            }
            command.extend(feature_args.clone());
            command.push("--test".to_string());
            command.push(binary.clone());
            command.push("--".to_string());
            command.push(format!(
                "--test-threads={}",
                threads_for_kind("integration")
            ));
            plan.push(PlanStep {
                tier: "integration".to_string(),
                name: format!("Integration tests: {file_path}"),
                command,
                timeout_s: timeout_for_kind("integration"),
                env: BTreeMap::new(),
                meta: json!({"test_binary": binary, "crate": crate_name, "features": features}),
                reason: "targeted integration: change scope match".to_string(),
            });
        } else {
            for item in selected {
                let mut command = vec!["cargo".to_string(), "test".to_string()];
                if let Some(crate_name) = &crate_name {
                    command.push("-p".to_string());
                    command.push(crate_name.to_string());
                }
                command.extend(feature_args.clone());
                command.push("--test".to_string());
                command.push(binary.clone());
                command.push(item.test.name.clone());
                command.push("--".to_string());
                command.push(format!(
                    "--test-threads={}",
                    threads_for_kind("integration")
                ));
                plan.push(PlanStep {
                    tier: "integration".to_string(),
                    name: format!("Integration test: {}", item.test.id),
                    command,
                    timeout_s: timeout_for_kind("integration"),
                    env: BTreeMap::new(),
                    meta: json!({
                        "test_binary": binary,
                        "crate": crate_name,
                        "features": features,
                        "test_name": item.test.name
                    }),
                    reason: "targeted integration: partial binary selection".to_string(),
                });
            }
        }
    }
    plan
}

fn run_plan(
    root: &Path,
    plan: &[PlanStep],
    base_env: &HashMap<String, String>,
) -> Result<(Vec<RunResult>, Vec<RerunCommand>)> {
    let mut results = Vec::new();
    let mut reruns = Vec::new();
    for step in plan {
        println!("\n==> {}", step.name);
        println!("    {}", step.command.join(" "));
        let mut env = base_env.clone();
        for (key, value) in &step.env {
            env.insert(key.clone(), value.clone());
        }
        let result = run_command(&step.command, root, &env, step.timeout_s)?;
        let status = if result.timed_out || result.exit_code != 0 {
            "failed".to_string()
        } else {
            "passed".to_string()
        };
        let failed_tests = parse_failed_tests(&result.output);
        let step_reruns = build_rerun_commands(step, &failed_tests);
        reruns.extend(step_reruns.clone());
        results.push(RunResult {
            tier: step.tier.clone(),
            name: step.name.clone(),
            command: step.command.join(" "),
            status,
            exit_code: result.exit_code,
            timed_out: result.timed_out,
            duration_s: result.duration_s,
            failed_tests,
            reruns: step_reruns,
        });
    }
    Ok((results, reruns))
}

fn run_command(
    command: &[String],
    cwd: &Path,
    env: &HashMap<String, String>,
    timeout_s: u64,
) -> Result<CommandResult> {
    if command.is_empty() {
        return Ok(CommandResult {
            exit_code: 1,
            timed_out: false,
            duration_s: 0.0,
            output: "empty command".to_string(),
        });
    }

    let mut cmd = Command::new(&command[0]);
    cmd.args(&command[1..])
        .current_dir(cwd)
        .envs(env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn command")?;
    let start = Instant::now();

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let output_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

    let output_buf_stdout = std::sync::Arc::clone(&output_buf);
    let stdout_handle = thread::spawn(move || {
        if let Some(mut stdout) = stdout {
            let mut buffer = String::new();
            let _ = stdout.read_to_string(&mut buffer);
            if !buffer.is_empty() {
                print!("{buffer}");
                let _ = std::io::stdout().flush();
                if let Ok(mut shared) = output_buf_stdout.lock() {
                    shared.push_str(&buffer);
                }
            }
        }
    });

    let output_buf_stderr = std::sync::Arc::clone(&output_buf);
    let stderr_handle = thread::spawn(move || {
        if let Some(mut stderr) = stderr {
            let mut buffer = String::new();
            let _ = stderr.read_to_string(&mut buffer);
            if !buffer.is_empty() {
                eprint!("{buffer}");
                let _ = std::io::stderr().flush();
                if let Ok(mut shared) = output_buf_stderr.lock() {
                    shared.push_str(&buffer);
                }
            }
        }
    });

    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait().context("Failed to poll command")? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(timeout_s) {
            timed_out = true;
            let _ = child.kill();
            break child.wait().context("Failed to wait on killed command")?;
        }
        thread::sleep(Duration::from_millis(100));
    };

    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let output = output_buf
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();

    Ok(CommandResult {
        exit_code: status.code().unwrap_or(1),
        timed_out,
        duration_s: start.elapsed().as_secs_f64(),
        output,
    })
}

fn parse_failed_tests(output: &str) -> Vec<String> {
    let mut failed = BTreeSet::new();
    for line in output.lines() {
        if line.starts_with("test ") && line.contains("FAILED") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 1 {
                failed.insert(parts[1].to_string());
            }
        }
    }
    failed.into_iter().collect()
}

fn build_rerun_commands(step: &PlanStep, failed_tests: &[String]) -> Vec<RerunCommand> {
    let mut reruns = Vec::new();
    let mut env = BTreeMap::new();
    env.insert(
        "AOS_DETERMINISM_SEED".to_string(),
        DEFAULT_DETERMINISM_SEED.to_string(),
    );
    env.insert("AOS_DEBUG_DETERMINISM".to_string(), "1".to_string());
    env.insert("RUST_BACKTRACE".to_string(), "1".to_string());
    env.insert(
        "RUST_LOG".to_string(),
        "adapteros=debug,tower_http=warn".to_string(),
    );

    let base = step.command.clone();

    if failed_tests.is_empty() {
        let cmd = add_debug_flags(base);
        reruns.push(RerunCommand {
            reason: "rerun failed suite".to_string(),
            env,
            command: cmd.join(" "),
        });
        return reruns;
    }

    for test_name in failed_tests {
        let mut cmd = base.clone();
        if let Some(idx) = cmd.iter().position(|value| value == "--") {
            cmd.insert(idx, test_name.clone());
        } else {
            cmd.push(test_name.clone());
        }
        let cmd = add_debug_flags(cmd);
        reruns.push(RerunCommand {
            reason: "failed test".to_string(),
            env: env.clone(),
            command: cmd.join(" "),
        });
    }
    reruns
}

fn add_debug_flags(mut cmd: Vec<String>) -> Vec<String> {
    if let Some(idx) = cmd.iter().position(|value| value == "--") {
        let tail = cmd[idx + 1..].to_vec();
        if !tail.iter().any(|value| value == "--nocapture") {
            cmd.push("--nocapture".to_string());
        }
        if !tail.iter().any(|value| value == "--test-threads=1") {
            cmd.push("--test-threads=1".to_string());
        }
    } else {
        cmd.push("--".to_string());
        cmd.push("--nocapture".to_string());
        cmd.push("--test-threads=1".to_string());
    }
    cmd
}

fn build_debt(selection: &[TestSelection]) -> TestDebt {
    let ignored: Vec<&TestSelection> = selection
        .iter()
        .filter(|item| item.test.ignore_reason.is_some())
        .collect();
    let ignored_missing_tracking: Vec<&TestSelection> = ignored
        .iter()
        .copied()
        .filter(|item| item.test.ignore_tracking_id.is_none())
        .collect();
    let stubs: Vec<&TestSelection> = selection
        .iter()
        .filter(|item| item.test.trust == "stub")
        .collect();
    let blocked: Vec<&TestSelection> = selection
        .iter()
        .filter(|item| !item.runnable && item.test.ignore_reason.is_none())
        .collect();

    TestDebt {
        ignored_tests_count: ignored.len(),
        ignored_missing_tracking_count: ignored_missing_tracking.len(),
        stub_tests_count: stubs.len(),
        blocked_tests_count: blocked.len(),
        ignored_missing_tracking: ignored_missing_tracking
            .iter()
            .map(|item| item.test.id.clone())
            .collect(),
        stub_tests: stubs.iter().map(|item| item.test.id.clone()).collect(),
        blocked_tests: blocked.iter().map(|item| item.test.id.clone()).collect(),
    }
}

fn summarize_selection(selection: &[TestSelection]) -> Summary {
    Summary {
        total: selection.len(),
        runnable: selection.iter().filter(|item| item.runnable).count(),
        selected: selection.iter().filter(|item| item.selected).count(),
        ignored: selection
            .iter()
            .filter(|item| item.test.ignore_reason.is_some())
            .count(),
        stubs: selection
            .iter()
            .filter(|item| item.test.trust == "stub")
            .count(),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create report directory")?;
    }
    let content = serde_json::to_string_pretty(value).context("Failed to serialize report")?;
    fs::write(path, content).context("Failed to write report")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn print_summary(
    summary: &Summary,
    capabilities: &Capabilities,
    report_path: PathBuf,
    debt: &TestDebt,
    doc_drift: &DocDrift,
    plan_len: usize,
    ran: bool,
    results: &[RunResult],
) {
    println!("\n==> Test Orchestrator Summary");
    println!(
        "    Inventory: {} tests ({} runnable)",
        summary.total, summary.runnable
    );
    println!(
        "    Selected: {} | Ignored: {} | Stubs: {}",
        summary.selected, summary.ignored, summary.stubs
    );
    println!(
        "    Capabilities: os={} metal={} mlx={} server={}",
        capabilities.os, capabilities.metal, capabilities.mlx, capabilities.server_running
    );
    println!(
        "    Plan steps: {} | Report: {}",
        plan_len,
        report_path.display()
    );

    if debt.ignored_missing_tracking_count > 0 {
        println!("    Debt: ignored tests missing tracking IDs detected");
    }
    let drift_missing = !doc_drift.missing_files.is_empty();
    let drift_targets = !doc_drift.missing_targets.is_empty();
    let drift_empty = !doc_drift.empty_files.is_empty();
    if drift_missing || drift_targets || drift_empty {
        println!("    Doc drift: missing or empty test references detected");
    }

    if ran {
        let failures = results.iter().any(|res| res.status != "passed");
        if failures {
            println!("    Result: failures detected (see report for rerun commands)");
        } else {
            println!("    Result: all planned steps passed");
        }
    }
}

fn capability_ok(capability: &str, caps: &Capabilities) -> bool {
    match capability {
        "AOS_DATABASE_URL" => caps.db.url_set,
        "RUNNING_SERVER" => caps.server_running,
        "METAL" => caps.metal,
        "MLX" => caps.mlx,
        "TOKEN" => !caps.tokens.is_empty(),
        "TOKENIZER_MODEL" => !caps.model_paths.is_empty(),
        "WORKER" => caps.server_running,
        _ => false,
    }
}

fn timeout_for_kind(kind: &str) -> u64 {
    TIMEOUTS_S
        .iter()
        .find(|(key, _)| *key == kind)
        .map(|(_, value)| *value)
        .unwrap_or(1200)
}

fn threads_for_kind(kind: &str) -> u64 {
    THREADS
        .iter()
        .find(|(key, _)| *key == kind)
        .map(|(_, value)| *value)
        .unwrap_or(4)
}
