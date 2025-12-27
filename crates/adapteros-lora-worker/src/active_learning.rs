//! Active-learning helpers for abstain events.
//!
//! When the router abstains, we capture the prompt and enough context to
//! enqueue a human/teacher review task and seed a golden dataset entry.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_telemetry::events::{AbstainEvent, AbstainReason};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Record written to the active-learning queue.
#[derive(Debug, Serialize)]
pub struct AbstainSampleRecord {
    pub id: String,
    pub timestamp_us: u64,
    pub reason: String,
    pub confidence: f32,
    pub entropy: Option<f32>,
    pub request_id: Option<String>,
    pub stack_id: Option<String>,
    pub stack_version: Option<i64>,
    pub tenant_id: Option<String>,
    pub prompt_digest_b3: Option<String>,
    pub prompt_chars: Option<usize>,
    pub prompt: Option<String>,
    pub prompt_truncated: bool,
    pub status: String,
}

/// Candidate written to the golden dataset staging file.
#[derive(Debug, Serialize)]
pub struct GoldenCandidate {
    pub sample_id: String,
    pub prompt: Option<String>,
    pub prompt_digest_b3: Option<String>,
    pub reason: String,
    pub confidence: f32,
    pub entropy: Option<f32>,
    pub status: String,
}

/// Lightweight retrain request record to be consumed by automation.
#[derive(Debug, Serialize)]
pub struct RouterRetrainRequest {
    pub sample_id: String,
    pub created_at_us: u64,
    pub status: String,
}

fn active_learning_dir() -> PathBuf {
    std::env::var("AOS_ACTIVE_LEARNING_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/active_learning"))
}

fn golden_dataset_path() -> PathBuf {
    std::env::var("AOS_ACTIVE_LEARNING_GOLDEN_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("training/datasets/abstain_golden/pending.ndjson"))
}

fn reason_label(reason: &AbstainReason) -> String {
    match reason {
        AbstainReason::LowConfidence { threshold } => {
            format!("low_confidence<threshold {:.3}", threshold)
        }
        AbstainReason::HighEntropy { threshold } => {
            format!("high_entropy>threshold {:.3}", threshold)
        }
        AbstainReason::InsufficientEvidence { min_required } => {
            format!("insufficient_evidence(min_required={})", min_required)
        }
        AbstainReason::MissingFields => "missing_fields".to_string(),
    }
}

fn write_ndjson(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AosError::Io(format!("Failed to create {:?}: {}", parent, e)))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| AosError::Io(format!("Failed to open {:?}: {}", path, e)))?;

    let line = serde_json::to_string(value).map_err(AosError::Serialization)?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| AosError::Io(format!("Failed to write {:?}: {}", path, e)))?;
    Ok(())
}

fn truncate_prompt(prompt: &str) -> (String, bool) {
    const MAX_CHARS: usize = 4096;
    if prompt.chars().count() <= MAX_CHARS {
        (prompt.to_string(), false)
    } else {
        let truncated: String = prompt.chars().take(MAX_CHARS).collect();
        (truncated, true)
    }
}

/// Enqueue an abstain sample for human/teacher review.
///
/// Returns the generated sample ID.
pub fn enqueue_abstain_sample(event: &AbstainEvent, prompt: Option<&str>) -> Result<String> {
    let mut id_material = event.timestamp_us.to_le_bytes().to_vec();
    if let Some(digest) = &event.prompt_digest_b3 {
        id_material.extend_from_slice(digest.as_bytes());
    }
    if let Some(req) = &event.request_id {
        id_material.extend_from_slice(req.as_bytes());
    }
    let id = B3Hash::hash(&id_material).to_hex();

    let (prompt_value, prompt_truncated) = if let Some(p) = prompt {
        let (val, truncated) = truncate_prompt(p);
        (Some(val), truncated)
    } else {
        (None, false)
    };

    let record = AbstainSampleRecord {
        id: id.clone(),
        timestamp_us: event.timestamp_us,
        reason: reason_label(&event.reason),
        confidence: event.confidence,
        entropy: event.entropy,
        request_id: event.request_id.clone(),
        stack_id: event.stack_id.clone(),
        stack_version: event.stack_version,
        tenant_id: event.tenant_id.clone(),
        prompt_digest_b3: event.prompt_digest_b3.clone(),
        prompt_chars: event
            .prompt_chars
            .or_else(|| prompt_value.as_ref().map(|p| p.chars().count())),
        prompt: prompt_value,
        prompt_truncated,
        status: "pending_label".to_string(),
    };

    let queue_path = active_learning_dir().join("abstain_queue.ndjson");
    write_ndjson(&queue_path, &record)?;

    maybe_enqueue_golden_candidate(&record)?;
    maybe_enqueue_retrain_request(&record)?;

    Ok(id)
}

fn maybe_enqueue_golden_candidate(record: &AbstainSampleRecord) -> Result<()> {
    // Allow disabling golden writes (e.g., in constrained environments)
    if std::env::var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let candidate = GoldenCandidate {
        sample_id: record.id.clone(),
        prompt: record.prompt.clone(),
        prompt_digest_b3: record.prompt_digest_b3.clone(),
        reason: record.reason.clone(),
        confidence: record.confidence,
        entropy: record.entropy,
        status: "needs_label".to_string(),
    };

    write_ndjson(&golden_dataset_path(), &candidate)
}

fn maybe_enqueue_retrain_request(record: &AbstainSampleRecord) -> Result<()> {
    if std::env::var("AOS_ACTIVE_LEARNING_TRIGGER_ROUTER_TRAIN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
    {
        let request = RouterRetrainRequest {
            sample_id: record.id.clone(),
            created_at_us: record.timestamp_us,
            status: "pending".to_string(),
        };
        let path = active_learning_dir().join("router_retrain_requests.ndjson");
        write_ndjson(&path, &request)?;
    }
    Ok(())
}
