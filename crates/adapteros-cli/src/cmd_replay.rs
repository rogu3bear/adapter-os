use std::fs;
use std::path::{Path, PathBuf};

use adapteros_core::B3Hash;
use anyhow::{anyhow, bail, Context, Result};
use blake3::Hasher;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::output::OutputWriter;

const REQUIRED_ARTIFACTS: &[&str] = &[
    "context_manifest.json",
    "token_trace.json",
    "input_tokens.json",
    "expected_report.json",
];

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
    pub allow_cross_worker: bool,
    pub base_model: ModelHash,
    pub adapters: Vec<AdapterHash>,
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

pub fn compute_context_digest(manifest: &ContextManifest) -> Result<B3Hash> {
    let mut adapters = manifest.adapters.clone();
    adapters.sort_by(|a, b| a.id.cmp(&b.id));

    let digest_payload = serde_json::json!({
        "base_model": {
            "id": manifest.base_model.id,
            "hash": manifest.base_model.hash,
        },
        "adapters": adapters
            .iter()
            .map(|a| serde_json::json!({"id": a.id, "hash": a.hash}))
            .collect::<Vec<_>>(),
    });

    let serialized = serde_json::to_vec(&digest_payload)?;
    Ok(B3Hash::hash(&serialized))
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

fn metadata_mismatch(manifest: &ContextManifest, expectation: &ReplayExpectation) -> Option<String> {
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
