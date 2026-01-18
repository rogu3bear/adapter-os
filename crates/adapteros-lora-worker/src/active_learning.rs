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

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::events::{AbstainEvent, AbstainReason};
    use serial_test::serial;
    use tempfile::TempDir;

    // =========================================================================
    // AbstainSampleRecord Tests
    // =========================================================================

    #[test]
    fn abstain_sample_record_creation() {
        let record = AbstainSampleRecord {
            id: "sample-123".to_string(),
            timestamp_us: 1700000000000000,
            reason: "low_confidence<threshold 0.500".to_string(),
            confidence: 0.35,
            entropy: Some(2.5),
            request_id: Some("req-456".to_string()),
            stack_id: Some("stack-789".to_string()),
            stack_version: Some(1),
            tenant_id: Some("tenant-abc".to_string()),
            prompt_digest_b3: Some("deadbeef".to_string()),
            prompt_chars: Some(100),
            prompt: Some("Test prompt".to_string()),
            prompt_truncated: false,
            status: "pending_label".to_string(),
        };

        assert_eq!(record.id, "sample-123");
        assert_eq!(record.confidence, 0.35);
        assert!(!record.prompt_truncated);
        assert_eq!(record.status, "pending_label");
    }

    #[test]
    fn abstain_sample_record_serialize() {
        let record = AbstainSampleRecord {
            id: "test-id".to_string(),
            timestamp_us: 1000000,
            reason: "high_entropy".to_string(),
            confidence: 0.5,
            entropy: None,
            request_id: None,
            stack_id: None,
            stack_version: None,
            tenant_id: None,
            prompt_digest_b3: None,
            prompt_chars: None,
            prompt: None,
            prompt_truncated: false,
            status: "pending_label".to_string(),
        };

        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("pending_label"));
    }

    // =========================================================================
    // GoldenCandidate Tests
    // =========================================================================

    #[test]
    fn golden_candidate_creation() {
        let candidate = GoldenCandidate {
            sample_id: "sample-123".to_string(),
            prompt: Some("What is the capital of France?".to_string()),
            prompt_digest_b3: Some("abc123".to_string()),
            reason: "low_confidence<threshold 0.400".to_string(),
            confidence: 0.3,
            entropy: Some(1.8),
            status: "needs_label".to_string(),
        };

        assert_eq!(candidate.sample_id, "sample-123");
        assert_eq!(candidate.status, "needs_label");
        assert!(candidate.prompt.is_some());
    }

    #[test]
    fn golden_candidate_serialize() {
        let candidate = GoldenCandidate {
            sample_id: "s1".to_string(),
            prompt: None,
            prompt_digest_b3: None,
            reason: "missing_fields".to_string(),
            confidence: 0.0,
            entropy: None,
            status: "needs_label".to_string(),
        };

        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains("needs_label"));
        assert!(json.contains("missing_fields"));
    }

    // =========================================================================
    // RouterRetrainRequest Tests
    // =========================================================================

    #[test]
    fn router_retrain_request_creation() {
        let request = RouterRetrainRequest {
            sample_id: "sample-001".to_string(),
            created_at_us: 1700000000000000,
            status: "pending".to_string(),
        };

        assert_eq!(request.sample_id, "sample-001");
        assert_eq!(request.status, "pending");
    }

    #[test]
    fn router_retrain_request_serialize() {
        let request = RouterRetrainRequest {
            sample_id: "req-123".to_string(),
            created_at_us: 999999,
            status: "pending".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("req-123"));
        assert!(json.contains("pending"));
    }

    // =========================================================================
    // reason_label Tests
    // =========================================================================

    #[test]
    fn reason_label_low_confidence() {
        let reason = AbstainReason::LowConfidence { threshold: 0.5 };
        let label = reason_label(&reason);
        assert!(label.starts_with("low_confidence<threshold"));
        assert!(label.contains("0.500"));
    }

    #[test]
    fn reason_label_high_entropy() {
        let reason = AbstainReason::HighEntropy { threshold: 2.0 };
        let label = reason_label(&reason);
        assert!(label.starts_with("high_entropy>threshold"));
        assert!(label.contains("2.000"));
    }

    #[test]
    fn reason_label_insufficient_evidence() {
        let reason = AbstainReason::InsufficientEvidence { min_required: 5 };
        let label = reason_label(&reason);
        assert!(label.contains("insufficient_evidence"));
        assert!(label.contains("min_required=5"));
    }

    #[test]
    fn reason_label_missing_fields() {
        let reason = AbstainReason::MissingFields;
        let label = reason_label(&reason);
        assert_eq!(label, "missing_fields");
    }

    // =========================================================================
    // truncate_prompt Tests
    // =========================================================================

    #[test]
    fn truncate_prompt_short_string() {
        let prompt = "Hello, world!";
        let (result, truncated) = truncate_prompt(prompt);
        assert_eq!(result, prompt);
        assert!(!truncated);
    }

    #[test]
    fn truncate_prompt_exactly_max_chars() {
        // Create a string of exactly 4096 characters
        let prompt: String = "a".repeat(4096);
        let (result, truncated) = truncate_prompt(&prompt);
        assert_eq!(result.chars().count(), 4096);
        assert!(!truncated);
    }

    #[test]
    fn truncate_prompt_exceeds_max_chars() {
        // Create a string exceeding 4096 characters
        let prompt: String = "b".repeat(5000);
        let (result, truncated) = truncate_prompt(&prompt);
        assert_eq!(result.chars().count(), 4096);
        assert!(truncated);
    }

    #[test]
    fn truncate_prompt_unicode() {
        // Test with multi-byte unicode characters
        let prompt: String = "🔥".repeat(5000); // Each emoji is one char but multiple bytes
        let (result, truncated) = truncate_prompt(&prompt);
        assert_eq!(result.chars().count(), 4096);
        assert!(truncated);
    }

    #[test]
    fn truncate_prompt_empty_string() {
        let (result, truncated) = truncate_prompt("");
        assert_eq!(result, "");
        assert!(!truncated);
    }

    // =========================================================================
    // active_learning_dir Tests
    // =========================================================================

    #[test]
    #[serial]
    fn active_learning_dir_default() {
        // Clear the env var to test default
        std::env::remove_var("AOS_ACTIVE_LEARNING_DIR");
        let dir = active_learning_dir();
        assert_eq!(dir, PathBuf::from("var/active_learning"));
    }

    #[test]
    #[serial]
    fn active_learning_dir_from_env() {
        std::env::set_var("AOS_ACTIVE_LEARNING_DIR", "/custom/path");
        let dir = active_learning_dir();
        assert_eq!(dir, PathBuf::from("/custom/path"));
        std::env::remove_var("AOS_ACTIVE_LEARNING_DIR");
    }

    // =========================================================================
    // golden_dataset_path Tests
    // =========================================================================

    #[test]
    #[serial]
    fn golden_dataset_path_default() {
        std::env::remove_var("AOS_ACTIVE_LEARNING_GOLDEN_PATH");
        let path = golden_dataset_path();
        assert_eq!(
            path,
            PathBuf::from("training/datasets/abstain_golden/pending.ndjson")
        );
    }

    #[test]
    #[serial]
    fn golden_dataset_path_from_env() {
        std::env::set_var("AOS_ACTIVE_LEARNING_GOLDEN_PATH", "/custom/golden.ndjson");
        let path = golden_dataset_path();
        assert_eq!(path, PathBuf::from("/custom/golden.ndjson"));
        std::env::remove_var("AOS_ACTIVE_LEARNING_GOLDEN_PATH");
    }

    // =========================================================================
    // write_ndjson Tests
    // =========================================================================

    #[test]
    fn write_ndjson_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.ndjson");

        let record = GoldenCandidate {
            sample_id: "test".to_string(),
            prompt: None,
            prompt_digest_b3: None,
            reason: "test".to_string(),
            confidence: 0.5,
            entropy: None,
            status: "needs_label".to_string(),
        };

        write_ndjson(&file_path, &record).unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("test"));
        assert!(content.ends_with('\n'));
    }

    #[test]
    fn write_ndjson_appends_to_existing() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("append.ndjson");

        let record1 = RouterRetrainRequest {
            sample_id: "first".to_string(),
            created_at_us: 1,
            status: "pending".to_string(),
        };
        let record2 = RouterRetrainRequest {
            sample_id: "second".to_string(),
            created_at_us: 2,
            status: "pending".to_string(),
        };

        write_ndjson(&file_path, &record1).unwrap();
        write_ndjson(&file_path, &record2).unwrap();

        let content = std::fs::read_to_string(&file_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("first"));
        assert!(lines[1].contains("second"));
    }

    #[test]
    fn write_ndjson_creates_parent_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested/deep/path/file.ndjson");

        let record = RouterRetrainRequest {
            sample_id: "nested".to_string(),
            created_at_us: 0,
            status: "pending".to_string(),
        };

        write_ndjson(&file_path, &record).unwrap();

        assert!(file_path.exists());
    }

    // =========================================================================
    // enqueue_abstain_sample Integration Tests
    // =========================================================================

    #[test]
    #[serial]
    fn enqueue_abstain_sample_generates_unique_ids() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("AOS_ACTIVE_LEARNING_DIR", temp_dir.path().to_str().unwrap());
        std::env::set_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN", "1");

        let event1 = AbstainEvent {
            timestamp_us: 1000000,
            reason: AbstainReason::LowConfidence { threshold: 0.5 },
            confidence: 0.3,
            entropy: None,
            missing_fields: vec![],
            evidence_span_count: 0,
            request_id: Some("req-1".to_string()),
            stack_id: None,
            stack_version: None,
            prompt_digest_b3: Some("hash1".to_string()),
            prompt_chars: None,
            tenant_id: None,
        };

        let event2 = AbstainEvent {
            timestamp_us: 1000001,
            reason: AbstainReason::LowConfidence { threshold: 0.5 },
            confidence: 0.3,
            entropy: None,
            missing_fields: vec![],
            evidence_span_count: 0,
            request_id: Some("req-2".to_string()),
            stack_id: None,
            stack_version: None,
            prompt_digest_b3: Some("hash2".to_string()),
            prompt_chars: None,
            tenant_id: None,
        };

        let id1 = enqueue_abstain_sample(&event1, None).unwrap();
        let id2 = enqueue_abstain_sample(&event2, None).unwrap();

        assert_ne!(id1, id2, "Different events should produce different IDs");

        std::env::remove_var("AOS_ACTIVE_LEARNING_DIR");
        std::env::remove_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN");
    }

    #[test]
    #[serial]
    fn enqueue_abstain_sample_with_prompt() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("AOS_ACTIVE_LEARNING_DIR", temp_dir.path().to_str().unwrap());
        std::env::set_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN", "1");

        let event = AbstainEvent {
            timestamp_us: 2000000,
            reason: AbstainReason::HighEntropy { threshold: 2.0 },
            confidence: 0.6,
            entropy: Some(2.5),
            missing_fields: vec![],
            evidence_span_count: 1,
            request_id: None,
            stack_id: Some("stack-1".to_string()),
            stack_version: Some(1),
            prompt_digest_b3: None,
            prompt_chars: None,
            tenant_id: Some("tenant-1".to_string()),
        };

        let id = enqueue_abstain_sample(&event, Some("Test prompt text")).unwrap();
        assert!(!id.is_empty());

        // Check the queue file was created
        let queue_file = temp_dir.path().join("abstain_queue.ndjson");
        assert!(queue_file.exists());

        let content = std::fs::read_to_string(&queue_file).unwrap();
        assert!(content.contains("Test prompt text"));
        assert!(content.contains("high_entropy"));

        std::env::remove_var("AOS_ACTIVE_LEARNING_DIR");
        std::env::remove_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN");
    }

    #[test]
    #[serial]
    fn enqueue_abstain_sample_truncates_long_prompt() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("AOS_ACTIVE_LEARNING_DIR", temp_dir.path().to_str().unwrap());
        std::env::set_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN", "1");

        let event = AbstainEvent {
            timestamp_us: 3000000,
            reason: AbstainReason::MissingFields,
            confidence: 0.0,
            entropy: None,
            missing_fields: vec!["field1".to_string()],
            evidence_span_count: 0,
            request_id: None,
            stack_id: None,
            stack_version: None,
            prompt_digest_b3: None,
            prompt_chars: None,
            tenant_id: None,
        };

        let long_prompt = "x".repeat(5000);
        let id = enqueue_abstain_sample(&event, Some(&long_prompt)).unwrap();
        assert!(!id.is_empty());

        let queue_file = temp_dir.path().join("abstain_queue.ndjson");
        let content = std::fs::read_to_string(&queue_file).unwrap();
        assert!(content.contains("prompt_truncated\":true"));

        std::env::remove_var("AOS_ACTIVE_LEARNING_DIR");
        std::env::remove_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN");
    }

    // =========================================================================
    // maybe_enqueue_golden_candidate Tests
    // =========================================================================

    #[test]
    #[serial]
    fn maybe_enqueue_golden_candidate_disabled() {
        std::env::set_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN", "1");

        let record = AbstainSampleRecord {
            id: "test".to_string(),
            timestamp_us: 0,
            reason: "test".to_string(),
            confidence: 0.5,
            entropy: None,
            request_id: None,
            stack_id: None,
            stack_version: None,
            tenant_id: None,
            prompt_digest_b3: None,
            prompt_chars: None,
            prompt: None,
            prompt_truncated: false,
            status: "pending_label".to_string(),
        };

        // Should return Ok and not create any files
        let result = maybe_enqueue_golden_candidate(&record);
        assert!(result.is_ok());

        std::env::remove_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN");
    }

    #[test]
    #[serial]
    fn maybe_enqueue_golden_candidate_enabled() {
        let temp_dir = TempDir::new().unwrap();
        let golden_path = temp_dir.path().join("golden.ndjson");

        std::env::remove_var("AOS_ACTIVE_LEARNING_DISABLE_GOLDEN");
        std::env::set_var("AOS_ACTIVE_LEARNING_GOLDEN_PATH", golden_path.to_str().unwrap());

        let record = AbstainSampleRecord {
            id: "golden-test".to_string(),
            timestamp_us: 1234567890,
            reason: "low_confidence".to_string(),
            confidence: 0.4,
            entropy: Some(1.5),
            request_id: None,
            stack_id: None,
            stack_version: None,
            tenant_id: None,
            prompt_digest_b3: Some("deadbeef".to_string()),
            prompt_chars: Some(50),
            prompt: Some("Golden candidate prompt".to_string()),
            prompt_truncated: false,
            status: "pending_label".to_string(),
        };

        let result = maybe_enqueue_golden_candidate(&record);
        assert!(result.is_ok());

        assert!(golden_path.exists());
        let content = std::fs::read_to_string(&golden_path).unwrap();
        assert!(content.contains("golden-test"));
        assert!(content.contains("needs_label"));

        std::env::remove_var("AOS_ACTIVE_LEARNING_GOLDEN_PATH");
    }

    // =========================================================================
    // maybe_enqueue_retrain_request Tests
    // =========================================================================

    #[test]
    #[serial]
    fn maybe_enqueue_retrain_request_disabled() {
        std::env::remove_var("AOS_ACTIVE_LEARNING_TRIGGER_ROUTER_TRAIN");

        let record = AbstainSampleRecord {
            id: "retrain-test".to_string(),
            timestamp_us: 9999999,
            reason: "test".to_string(),
            confidence: 0.5,
            entropy: None,
            request_id: None,
            stack_id: None,
            stack_version: None,
            tenant_id: None,
            prompt_digest_b3: None,
            prompt_chars: None,
            prompt: None,
            prompt_truncated: false,
            status: "pending_label".to_string(),
        };

        // Should return Ok and not create any files
        let result = maybe_enqueue_retrain_request(&record);
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn maybe_enqueue_retrain_request_enabled() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("AOS_ACTIVE_LEARNING_DIR", temp_dir.path().to_str().unwrap());
        std::env::set_var("AOS_ACTIVE_LEARNING_TRIGGER_ROUTER_TRAIN", "true");

        let record = AbstainSampleRecord {
            id: "retrain-enabled".to_string(),
            timestamp_us: 8888888,
            reason: "test".to_string(),
            confidence: 0.5,
            entropy: None,
            request_id: None,
            stack_id: None,
            stack_version: None,
            tenant_id: None,
            prompt_digest_b3: None,
            prompt_chars: None,
            prompt: None,
            prompt_truncated: false,
            status: "pending_label".to_string(),
        };

        let result = maybe_enqueue_retrain_request(&record);
        assert!(result.is_ok());

        let retrain_file = temp_dir.path().join("router_retrain_requests.ndjson");
        assert!(retrain_file.exists());

        let content = std::fs::read_to_string(&retrain_file).unwrap();
        assert!(content.contains("retrain-enabled"));
        assert!(content.contains("pending"));

        std::env::remove_var("AOS_ACTIVE_LEARNING_DIR");
        std::env::remove_var("AOS_ACTIVE_LEARNING_TRIGGER_ROUTER_TRAIN");
    }
}
