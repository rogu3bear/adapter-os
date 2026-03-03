use std::fs;
use std::path::{Path, PathBuf};

use adapteros_core::receipt_digest::canonical_json_string;
use adapteros_core::B3Hash;
use anyhow::{anyhow, bail, Context, Result};
use blake3::Hasher;
use rusqlite::{params, Connection, OpenFlags};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::output::OutputWriter;

const REQUIRED_ARTIFACTS: &[&str] = &[
    "context_manifest.json",
    "token_trace.json",
    "input_tokens.json",
    "expected_report.json",
];
const REPLAY_AVAILABILITY_REPORT_FILE: &str = "replay_availability_report.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayAvailabilityCheckStatus {
    Pass,
    Fail,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayAvailabilityCheck {
    pub name: String,
    pub status: ReplayAvailabilityCheckStatus,
    pub blocking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayAvailabilityReport {
    pub status: String,
    pub generated_at: String,
    pub fixture_dir: String,
    pub required_checks_passed: bool,
    pub blocking_failures: Vec<String>,
    pub db_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_path: Option<String>,
    pub checks: Vec<ReplayAvailabilityCheck>,
}

#[derive(Debug, Clone)]
pub struct ReplayAvailabilitySummary {
    pub status: String,
    pub required_checks_passed: bool,
    pub db_status: String,
    pub report_path: PathBuf,
    pub blocking_failures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHash {
    pub id: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterHash {
    pub id: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextManifest {
    pub request_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub worker_id: Option<String>,
    #[serde(default)]
    pub worker_id_included: bool,
    #[serde(default)]
    pub allow_cross_worker: bool,
    pub base_model: ModelHash,
    pub adapters: Vec<AdapterHash>,
    #[serde(default, alias = "policy_mask_digest")]
    pub policy_mask_digest_b3: Option<String>,
    #[serde(default)]
    pub backend_used: Option<String>,
    #[serde(default)]
    pub kernel_version_id: Option<String>,
    #[serde(default)]
    pub tokenizer_hash_b3: Option<String>,
    #[serde(default)]
    pub tokenizer_version: Option<String>,
    #[serde(default)]
    pub tokenizer_normalization: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStep {
    pub step: usize,
    pub input_id: u32,
    pub output_id: u32,
    pub gate_q15: i32,
    pub adapter_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenTrace {
    pub global_seed: Option<String>,
    pub steps: Vec<TraceStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputTokens {
    pub tokens: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayExpectation {
    pub request_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub worker_id: Option<String>,
    #[serde(default)]
    pub allow_cross_worker: bool,
    pub expected_context_digest: String,
    pub expected_receipt: String,
    pub expected_output_tokens: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub request_id: String,
    pub cpid: String,
    pub plan_id: String,
    pub status: String,
    pub reason: Option<String>,
    pub context_digest_match: bool,
    pub receipt_match: bool,
    pub output_match: bool,
    pub worker_check_passed: bool,
    pub computed_context_digest: String,
    pub expected_context_digest: String,
    pub computed_receipt: String,
    pub expected_receipt: String,
    pub output_tokens: Vec<u32>,
    pub expected_output_tokens: Vec<u32>,
    pub availability_status: String,
    pub availability_required_checks_passed: bool,
    pub availability_db_status: String,
    pub availability_report_path: String,
}

impl ReplayReport {
    pub fn passed(&self) -> bool {
        self.status == "pass"
    }
}

fn ensure_artifacts_exist(dir: &Path) -> Result<()> {
    for name in REQUIRED_ARTIFACTS {
        let path = dir.join(name);
        if !path.exists() {
            bail!("missing required artifact: {}", path.display());
        }
    }
    Ok(())
}

fn push_availability_check(
    checks: &mut Vec<ReplayAvailabilityCheck>,
    name: impl Into<String>,
    status: ReplayAvailabilityCheckStatus,
    blocking: bool,
    detail: Option<String>,
) {
    checks.push(ReplayAvailabilityCheck {
        name: name.into(),
        status,
        blocking,
        detail,
    });
}

fn validate_required_field(
    checks: &mut Vec<ReplayAvailabilityCheck>,
    name: &str,
    value: &str,
    detail: &str,
) {
    let status = if value.trim().is_empty() {
        ReplayAvailabilityCheckStatus::Fail
    } else {
        ReplayAvailabilityCheckStatus::Pass
    };
    push_availability_check(
        checks,
        format!("manifest.{}", name),
        status,
        true,
        if status == ReplayAvailabilityCheckStatus::Fail {
            Some(detail.to_string())
        } else {
            None
        },
    );
}

fn validate_manifest_components(
    manifest: &ContextManifest,
    checks: &mut Vec<ReplayAvailabilityCheck>,
) {
    validate_required_field(
        checks,
        "request_id",
        &manifest.request_id,
        "request_id must be non-empty",
    );
    validate_required_field(checks, "cpid", &manifest.cpid, "cpid must be non-empty");
    validate_required_field(
        checks,
        "plan_id",
        &manifest.plan_id,
        "plan_id must be non-empty",
    );
    validate_required_field(
        checks,
        "base_model.id",
        &manifest.base_model.id,
        "base_model.id must be non-empty",
    );
    validate_required_field(
        checks,
        "base_model.hash",
        &manifest.base_model.hash,
        "base_model.hash must be non-empty",
    );

    if manifest.adapters.is_empty() {
        push_availability_check(
            checks,
            "manifest.adapters".to_string(),
            ReplayAvailabilityCheckStatus::Pass,
            true,
            Some("no adapters declared (base-only replay)".to_string()),
        );
        return;
    }

    let mut invalid_adapters = Vec::new();
    for (idx, adapter) in manifest.adapters.iter().enumerate() {
        if adapter.id.trim().is_empty() || adapter.hash.trim().is_empty() {
            invalid_adapters.push(idx);
        }
    }
    if invalid_adapters.is_empty() {
        push_availability_check(
            checks,
            "manifest.adapters".to_string(),
            ReplayAvailabilityCheckStatus::Pass,
            true,
            Some(format!(
                "{} adapter component(s) validated",
                manifest.adapters.len()
            )),
        );
    } else {
        push_availability_check(
            checks,
            "manifest.adapters".to_string(),
            ReplayAvailabilityCheckStatus::Fail,
            true,
            Some(format!(
                "adapter entries missing id/hash at indexes {:?}",
                invalid_adapters
            )),
        );
    }
}

fn parse_sqlite_db_path(url_or_path: &str) -> Option<PathBuf> {
    let raw = url_or_path.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("sqlite::memory:") {
        return None;
    }
    if let Some(path) = raw.strip_prefix("sqlite://") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = raw.strip_prefix("sqlite:") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = raw.strip_prefix("file://") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = raw.strip_prefix("file:") {
        return Some(PathBuf::from(path));
    }
    if raw.contains("://") {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn resolve_db_path_for_checks() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("AOS_DATABASE_URL") {
        return parse_sqlite_db_path(&value);
    }
    if let Ok(value) = std::env::var("DATABASE_URL") {
        return parse_sqlite_db_path(&value);
    }
    Some(adapteros_core::rebase_var_path("var/aos-cp.sqlite3"))
}

fn sqlite_error_mentions_missing_table(err: &rusqlite::Error) -> bool {
    match err {
        rusqlite::Error::SqliteFailure(_, Some(msg)) => msg.contains("no such table"),
        _ => false,
    }
}

fn add_db_presence_checks(
    manifest: Option<&ContextManifest>,
    checks: &mut Vec<ReplayAvailabilityCheck>,
) -> (String, Option<PathBuf>) {
    let Some(manifest) = manifest else {
        push_availability_check(
            checks,
            "db.presence",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some("manifest unavailable; skipped DB checks".to_string()),
        );
        return ("unknown".to_string(), None);
    };

    let Some(db_path) = resolve_db_path_for_checks() else {
        push_availability_check(
            checks,
            "db.connection",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some("non-sqlite database URL or invalid DB path".to_string()),
        );
        return ("unknown".to_string(), None);
    };

    if !db_path.exists() {
        push_availability_check(
            checks,
            "db.connection",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some(format!("database not found at {}", db_path.display())),
        );
        return ("unknown".to_string(), Some(db_path));
    }

    let conn = match Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(conn) => conn,
        Err(err) => {
            push_availability_check(
                checks,
                "db.connection",
                ReplayAvailabilityCheckStatus::Unknown,
                false,
                Some(format!(
                    "unable to open database {}: {}",
                    db_path.display(),
                    err
                )),
            );
            return ("unknown".to_string(), Some(db_path));
        }
    };

    push_availability_check(
        checks,
        "db.connection",
        ReplayAvailabilityCheckStatus::Pass,
        false,
        Some(format!("connected: {}", db_path.display())),
    );

    let trace_count = conn.query_row(
        "SELECT COUNT(1) FROM inference_traces WHERE request_id = ?1",
        params![manifest.request_id],
        |row| row.get::<_, i64>(0),
    );
    match trace_count {
        Ok(count) if count > 0 => push_availability_check(
            checks,
            "db.inference_traces.request_id",
            ReplayAvailabilityCheckStatus::Pass,
            true,
            Some(format!("matched {} row(s)", count)),
        ),
        Ok(_) => push_availability_check(
            checks,
            "db.inference_traces.request_id",
            ReplayAvailabilityCheckStatus::Fail,
            true,
            Some(format!(
                "no rows found for request_id={}",
                manifest.request_id
            )),
        ),
        Err(err) if sqlite_error_mentions_missing_table(&err) => push_availability_check(
            checks,
            "db.inference_traces.request_id",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some("table inference_traces not found".to_string()),
        ),
        Err(err) => push_availability_check(
            checks,
            "db.inference_traces.request_id",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some(format!("query failed: {}", err)),
        ),
    }

    let replay_session_count = conn.query_row(
        "SELECT COUNT(1) FROM replay_sessions WHERE cpid = ?1 AND plan_id = ?2",
        params![manifest.cpid, manifest.plan_id],
        |row| row.get::<_, i64>(0),
    );
    match replay_session_count {
        Ok(count) if count > 0 => push_availability_check(
            checks,
            "db.replay_sessions.cpid_plan",
            ReplayAvailabilityCheckStatus::Pass,
            true,
            Some(format!("matched {} row(s)", count)),
        ),
        Ok(_) => push_availability_check(
            checks,
            "db.replay_sessions.cpid_plan",
            ReplayAvailabilityCheckStatus::Fail,
            true,
            Some(format!(
                "no rows found for cpid={} plan_id={}",
                manifest.cpid, manifest.plan_id
            )),
        ),
        Err(err) if sqlite_error_mentions_missing_table(&err) => push_availability_check(
            checks,
            "db.replay_sessions.cpid_plan",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some("table replay_sessions not found".to_string()),
        ),
        Err(err) => push_availability_check(
            checks,
            "db.replay_sessions.cpid_plan",
            ReplayAvailabilityCheckStatus::Unknown,
            false,
            Some(format!("query failed: {}", err)),
        ),
    }

    ("reachable".to_string(), Some(db_path))
}

pub fn generate_availability_report(
    dir: &Path,
    output: &OutputWriter,
) -> Result<ReplayAvailabilitySummary> {
    let mut checks = Vec::new();
    let mut manifest: Option<ContextManifest> = None;
    let mut expectation: Option<ReplayExpectation> = None;

    for name in REQUIRED_ARTIFACTS {
        let path = dir.join(name);
        if path.exists() {
            push_availability_check(
                &mut checks,
                format!("artifact:{}", name),
                ReplayAvailabilityCheckStatus::Pass,
                true,
                None,
            );
        } else {
            push_availability_check(
                &mut checks,
                format!("artifact:{}", name),
                ReplayAvailabilityCheckStatus::Fail,
                true,
                Some(format!("missing required artifact: {}", path.display())),
            );
        }
    }

    let manifest_path = dir.join("context_manifest.json");
    if manifest_path.exists() {
        match load_json::<ContextManifest>(&manifest_path) {
            Ok(value) => {
                manifest = Some(value);
                push_availability_check(
                    &mut checks,
                    "manifest.parse",
                    ReplayAvailabilityCheckStatus::Pass,
                    true,
                    None,
                );
            }
            Err(err) => push_availability_check(
                &mut checks,
                "manifest.parse",
                ReplayAvailabilityCheckStatus::Fail,
                true,
                Some(err.to_string()),
            ),
        }
    }

    let expected_path = dir.join("expected_report.json");
    if expected_path.exists() {
        match load_json::<ReplayExpectation>(&expected_path) {
            Ok(value) => {
                expectation = Some(value);
                push_availability_check(
                    &mut checks,
                    "expected_report.parse",
                    ReplayAvailabilityCheckStatus::Pass,
                    true,
                    None,
                );
            }
            Err(err) => push_availability_check(
                &mut checks,
                "expected_report.parse",
                ReplayAvailabilityCheckStatus::Fail,
                true,
                Some(err.to_string()),
            ),
        }
    }

    if let Some(manifest_ref) = manifest.as_ref() {
        validate_manifest_components(manifest_ref, &mut checks);
    }

    if let (Some(manifest_ref), Some(expectation_ref)) = (manifest.as_ref(), expectation.as_ref()) {
        if let Some(reason) = metadata_mismatch(manifest_ref, expectation_ref) {
            push_availability_check(
                &mut checks,
                "manifest.expected_metadata",
                ReplayAvailabilityCheckStatus::Fail,
                true,
                Some(reason),
            );
        } else {
            push_availability_check(
                &mut checks,
                "manifest.expected_metadata",
                ReplayAvailabilityCheckStatus::Pass,
                true,
                None,
            );
        }

        let worker_match = expectation_ref.allow_cross_worker
            || expectation_ref.worker_id.is_none()
            || manifest_ref.worker_id == expectation_ref.worker_id;
        push_availability_check(
            &mut checks,
            "manifest.expected_worker",
            if worker_match {
                ReplayAvailabilityCheckStatus::Pass
            } else {
                ReplayAvailabilityCheckStatus::Fail
            },
            true,
            if worker_match {
                None
            } else {
                Some("metadata_mismatch:worker_id".to_string())
            },
        );
    }

    let (db_status, db_path) = add_db_presence_checks(manifest.as_ref(), &mut checks);
    let blocking_failures: Vec<String> = checks
        .iter()
        .filter(|check| {
            check.blocking && matches!(check.status, ReplayAvailabilityCheckStatus::Fail)
        })
        .map(|check| {
            format!(
                "{}{}",
                check.name,
                check
                    .detail
                    .as_ref()
                    .map(|d| format!(" ({})", d))
                    .unwrap_or_default()
            )
        })
        .collect();
    let required_checks_passed = blocking_failures.is_empty();
    let status = if required_checks_passed {
        "available".to_string()
    } else {
        "unavailable".to_string()
    };

    let report_path = dir.join(REPLAY_AVAILABILITY_REPORT_FILE);
    let report = ReplayAvailabilityReport {
        status: status.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        fixture_dir: dir.display().to_string(),
        required_checks_passed,
        blocking_failures: blocking_failures.clone(),
        db_status: db_status.clone(),
        db_path: db_path.map(|p| p.display().to_string()),
        checks,
    };
    let report_json = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("Failed to write {}", report_path.display()))?;

    if output.is_verbose() {
        output.progress(format!(
            "Replay availability report written to {}",
            report_path.display()
        ));
    }

    Ok(ReplayAvailabilitySummary {
        status,
        required_checks_passed,
        db_status,
        report_path,
        blocking_failures,
    })
}

pub fn compute_context_digest(manifest: &ContextManifest) -> Result<B3Hash> {
    let mut adapters = manifest.adapters.clone();
    adapters.sort_by(|a, b| a.id.cmp(&b.id));

    let worker_id_included = manifest.worker_id_included || manifest.worker_id.is_some();
    let worker_component = if worker_id_included {
        manifest.worker_id.clone().unwrap_or_default()
    } else {
        String::new()
    };

    let digest_payload = serde_json::json!({
        "base_model": {
            "id": manifest.base_model.id,
            "hash": manifest.base_model.hash,
        },
        "adapters": adapters
            .iter()
            .map(|a| serde_json::json!({"id": a.id, "hash": a.hash}))
            .collect::<Vec<_>>(),
        "policy_mask_digest_b3": manifest.policy_mask_digest_b3.as_deref().unwrap_or(""),
        "worker_id_included": worker_id_included,
        "worker_id": worker_component,
        "backend_used": manifest.backend_used.as_deref().unwrap_or(""),
        "kernel_version_id": manifest.kernel_version_id.as_deref().unwrap_or(""),
        "tokenizer_hash_b3": manifest.tokenizer_hash_b3.as_deref().unwrap_or(""),
        "tokenizer_version": manifest.tokenizer_version.as_deref().unwrap_or(""),
        "tokenizer_normalization": manifest.tokenizer_normalization.as_deref().unwrap_or(""),
    });
    let serialized = canonical_json_string(&digest_payload)?;
    Ok(B3Hash::hash(serialized.as_bytes()))
}

pub fn compute_receipt(
    context_digest: &B3Hash,
    input_tokens: &[u32],
    trace: &TokenTrace,
) -> Result<B3Hash> {
    let mut hasher = Hasher::new();
    hasher.update(b"aos-replay-v1");
    hasher.update(context_digest.as_bytes());

    for token in input_tokens {
        hasher.update(&token.to_le_bytes());
    }

    for step in &trace.steps {
        hasher.update(&step.step.to_le_bytes());
        hasher.update(&step.input_id.to_le_bytes());
        hasher.update(&step.output_id.to_le_bytes());
        hasher.update(&step.gate_q15.to_le_bytes());
        if let Some(adapter) = &step.adapter_id {
            hasher.update(adapter.as_bytes());
        }
    }

    Ok(B3Hash::from_bytes(*hasher.finalize().as_bytes()))
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let data =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let parsed = serde_json::from_str(&data)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(parsed)
}

fn metadata_mismatch(
    manifest: &ContextManifest,
    expectation: &ReplayExpectation,
) -> Option<String> {
    if manifest.request_id != expectation.request_id {
        return Some("metadata_mismatch:request_id".to_string());
    }
    if manifest.cpid != expectation.cpid {
        return Some("metadata_mismatch:cpid".to_string());
    }
    if manifest.plan_id != expectation.plan_id {
        return Some("metadata_mismatch:plan_id".to_string());
    }
    None
}

pub fn load_inputs(
    dir: &Path,
) -> Result<(ContextManifest, TokenTrace, InputTokens, ReplayExpectation)> {
    let manifest_path = dir.join("context_manifest.json");
    let trace_path = dir.join("token_trace.json");
    let tokens_path = dir.join("input_tokens.json");
    let expected_path = dir.join("expected_report.json");

    let manifest: ContextManifest = load_json(&manifest_path)?;
    let trace: TokenTrace = load_json(&trace_path)?;
    let input_tokens: InputTokens = load_json(&tokens_path)?;
    let expectation: ReplayExpectation = load_json(&expected_path)?;

    Ok((manifest, trace, input_tokens, expectation))
}

pub fn run(
    dir: &Path,
    verify: bool,
    report_override: Option<&Path>,
    availability: &ReplayAvailabilitySummary,
    output: &OutputWriter,
) -> Result<ReplayReport> {
    ensure_artifacts_exist(dir)?;
    let (manifest, trace, input_tokens, expectation) = load_inputs(dir)?;

    let computed_context = compute_context_digest(&manifest)?;
    let computed_receipt = compute_receipt(&computed_context, &input_tokens.tokens, &trace)?;
    let output_tokens: Vec<u32> = trace.steps.iter().map(|s| s.output_id).collect();

    let context_match = computed_context.to_hex() == expectation.expected_context_digest;
    let receipt_match = computed_receipt.to_hex() == expectation.expected_receipt;
    let output_match = output_tokens == expectation.expected_output_tokens;
    let worker_match = expectation.allow_cross_worker
        || expectation.worker_id.is_none()
        || manifest.worker_id == expectation.worker_id;

    let mut status = "pass".to_string();
    let mut reason = None;

    if let Some(metadata_reason) = metadata_mismatch(&manifest, &expectation) {
        status = "fail".to_string();
        reason = Some(metadata_reason);
    } else if !context_match {
        status = "fail".to_string();
        reason = Some("context_digest_mismatch".to_string());
    } else if !receipt_match {
        status = "fail".to_string();
        reason = Some("receipt_mismatch".to_string());
    } else if !output_match {
        status = "fail".to_string();
        reason = Some("output_tokens_mismatch".to_string());
    } else if !worker_match {
        status = "fail".to_string();
        reason = Some("worker_mismatch".to_string());
    }

    let report = ReplayReport {
        request_id: expectation.request_id.clone(),
        cpid: expectation.cpid.clone(),
        plan_id: expectation.plan_id.clone(),
        status: status.clone(),
        reason: reason.clone(),
        context_digest_match: context_match,
        receipt_match,
        output_match,
        worker_check_passed: worker_match,
        computed_context_digest: computed_context.to_hex(),
        expected_context_digest: expectation.expected_context_digest.clone(),
        computed_receipt: computed_receipt.to_hex(),
        expected_receipt: expectation.expected_receipt.clone(),
        output_tokens: output_tokens.clone(),
        expected_output_tokens: expectation.expected_output_tokens.clone(),
        availability_status: availability.status.clone(),
        availability_required_checks_passed: availability.required_checks_passed,
        availability_db_status: availability.db_status.clone(),
        availability_report_path: availability.report_path.display().to_string(),
    };

    let report_path: PathBuf = report_override
        .map(PathBuf::from)
        .unwrap_or_else(|| dir.join("replay_report.json"));
    let report_json = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("Failed to write {}", report_path.display()))?;

    if output.is_verbose() {
        output.progress(format!(
            "Replay report written to {}",
            report_path.display()
        ));
    }

    if verify && !report.passed() {
        return Err(anyhow!(
            "replay verification failed ({})",
            reason.unwrap_or_else(|| "unknown".to_string())
        ));
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputMode, OutputWriter};
    use serial_test::serial;
    use std::path::Path;
    use tempfile::tempdir;

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn sample_manifest() -> ContextManifest {
        ContextManifest {
            request_id: "req-1".to_string(),
            cpid: "tenant-a".to_string(),
            plan_id: "plan-1".to_string(),
            worker_id: None,
            worker_id_included: false,
            allow_cross_worker: false,
            base_model: ModelHash {
                id: "base".to_string(),
                hash: "base-hash".to_string(),
            },
            adapters: vec![AdapterHash {
                id: "adapter-a".to_string(),
                hash: "adapter-hash".to_string(),
            }],
            policy_mask_digest_b3: None,
            backend_used: Some("coreml".to_string()),
            kernel_version_id: Some("kernel-v1".to_string()),
            tokenizer_hash_b3: None,
            tokenizer_version: None,
            tokenizer_normalization: None,
        }
    }

    fn sample_trace() -> TokenTrace {
        TokenTrace {
            global_seed: Some("seed-1".to_string()),
            steps: vec![
                TraceStep {
                    step: 0,
                    input_id: 11,
                    output_id: 101,
                    gate_q15: 123,
                    adapter_id: Some("adapter-a".to_string()),
                },
                TraceStep {
                    step: 1,
                    input_id: 12,
                    output_id: 102,
                    gate_q15: 120,
                    adapter_id: Some("adapter-a".to_string()),
                },
            ],
        }
    }

    fn sample_input_tokens() -> InputTokens {
        InputTokens {
            tokens: vec![11, 12],
        }
    }

    fn write_json_file<T: Serialize>(path: &Path, value: &T) {
        let body = serde_json::to_string_pretty(value).expect("serialize test json");
        std::fs::write(path, body).expect("write test json");
    }

    fn write_valid_fixture(dir: &Path) {
        let manifest = sample_manifest();
        let trace = sample_trace();
        let input_tokens = sample_input_tokens();
        let computed_context = compute_context_digest(&manifest).expect("context digest");
        let computed_receipt =
            compute_receipt(&computed_context, &input_tokens.tokens, &trace).expect("receipt");
        let expectation = ReplayExpectation {
            request_id: manifest.request_id.clone(),
            cpid: manifest.cpid.clone(),
            plan_id: manifest.plan_id.clone(),
            worker_id: manifest.worker_id.clone(),
            allow_cross_worker: manifest.allow_cross_worker,
            expected_context_digest: computed_context.to_hex(),
            expected_receipt: computed_receipt.to_hex(),
            expected_output_tokens: trace.steps.iter().map(|step| step.output_id).collect(),
        };

        write_json_file(&dir.join("context_manifest.json"), &manifest);
        write_json_file(&dir.join("token_trace.json"), &trace);
        write_json_file(&dir.join("input_tokens.json"), &input_tokens);
        write_json_file(&dir.join("expected_report.json"), &expectation);
    }

    fn quiet_output() -> OutputWriter {
        OutputWriter::new(OutputMode::Quiet, false)
    }

    #[test]
    fn context_digest_changes_with_worker_presence() {
        let mut manifest = sample_manifest();
        manifest.worker_id_included = true;
        manifest.worker_id = Some("worker-a".to_string());
        let digest_a = compute_context_digest(&manifest).expect("digest a");

        let mut manifest_worker_b = manifest.clone();
        manifest_worker_b.worker_id = Some("worker-b".to_string());
        let digest_b = compute_context_digest(&manifest_worker_b).expect("digest b");
        assert_ne!(
            digest_a, digest_b,
            "context digest must change when worker id changes"
        );

        let mut manifest_flagged_off = manifest.clone();
        manifest_flagged_off.worker_id_included = false;
        manifest_flagged_off.worker_id = None;
        let digest_c = compute_context_digest(&manifest_flagged_off).expect("digest c");
        assert_ne!(
            digest_a, digest_c,
            "removing worker presence should change the digest"
        );
    }

    #[test]
    #[serial]
    fn availability_report_blocks_on_missing_required_artifact() {
        let fixture = tempdir().expect("temp dir");
        write_valid_fixture(fixture.path());
        std::fs::remove_file(fixture.path().join("expected_report.json"))
            .expect("remove expected report");

        let _aos_db = EnvVarGuard::set(
            "AOS_DATABASE_URL",
            "sqlite:///definitely/not/here/replay-preflight.sqlite3",
        );
        let _legacy_db = EnvVarGuard::set(
            "DATABASE_URL",
            "sqlite:///definitely/not/here/replay-preflight.sqlite3",
        );

        let summary = generate_availability_report(fixture.path(), &quiet_output())
            .expect("availability report");
        assert!(!summary.required_checks_passed);
        assert_eq!(summary.status, "unavailable");
        assert!(summary
            .blocking_failures
            .iter()
            .any(|failure| failure.contains("artifact:expected_report.json")));
        assert!(summary.report_path.exists());
    }

    #[test]
    #[serial]
    fn availability_report_marks_unreachable_db_as_unknown_non_fatal() {
        let fixture = tempdir().expect("temp dir");
        write_valid_fixture(fixture.path());

        let _aos_db = EnvVarGuard::set(
            "AOS_DATABASE_URL",
            "sqlite:///definitely/not/here/replay-preflight.sqlite3",
        );
        let _legacy_db = EnvVarGuard::set(
            "DATABASE_URL",
            "sqlite:///definitely/not/here/replay-preflight.sqlite3",
        );

        let summary = generate_availability_report(fixture.path(), &quiet_output())
            .expect("availability report");
        assert!(summary.required_checks_passed);
        assert_eq!(summary.status, "available");
        assert_eq!(summary.db_status, "unknown");
        assert!(summary.report_path.exists());

        let report: ReplayAvailabilityReport =
            load_json(&summary.report_path).expect("read availability report");
        assert_eq!(report.db_status, "unknown");
    }

    #[test]
    #[serial]
    fn replay_report_includes_availability_metadata() {
        let fixture = tempdir().expect("temp dir");
        write_valid_fixture(fixture.path());

        let _aos_db = EnvVarGuard::set(
            "AOS_DATABASE_URL",
            "sqlite:///definitely/not/here/replay-preflight.sqlite3",
        );
        let _legacy_db = EnvVarGuard::set(
            "DATABASE_URL",
            "sqlite:///definitely/not/here/replay-preflight.sqlite3",
        );

        let output = quiet_output();
        let summary =
            generate_availability_report(fixture.path(), &output).expect("availability report");
        assert!(summary.required_checks_passed);

        let report = run(fixture.path(), false, None, &summary, &output).expect("replay run");
        assert_eq!(report.availability_status, "available");
        assert!(report.availability_required_checks_passed);
        assert_eq!(report.availability_db_status, "unknown");
        assert!(report
            .availability_report_path
            .ends_with(REPLAY_AVAILABILITY_REPORT_FILE));

        let persisted: ReplayReport =
            load_json(&fixture.path().join("replay_report.json")).expect("persisted replay report");
        assert_eq!(persisted.availability_status, "available");
    }
}
