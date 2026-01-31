//! Internal helper functions for dataset handlers.

use crate::api_error::ApiError;
use crate::handlers::datasets::validation::ValidationError;
use crate::types::{DatasetValidationStatus, ErrorResponse};
use adapteros_api_types::training::{JsonlFieldTypeMismatch, JsonlValidationDiagnostic};
use adapteros_core::B3Hash;
use adapteros_db::training_datasets::DatasetFile;
use axum::http::StatusCode;
use axum::Json;
use std::collections::HashSet;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::services::CanonicalRow;
use crate::state::AppState;

use super::paths::resolve_dataset_root;
use adapteros_storage::secure_fs::path_policy::canonicalize_strict_in_allowed_roots;

/// Maximum file size (100MB)
pub const MAX_FILE_SIZE: usize = 100 * 1024 * 1024;

/// Maximum total upload size (500MB)
pub const MAX_TOTAL_SIZE: usize = 500 * 1024 * 1024;

/// Maximum number of files per upload
pub const MAX_FILE_COUNT: usize = 1000;

pub const DEFAULT_DATASET_HARD_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
pub const DEFAULT_SOFT_PCT: f64 = 0.8;

/// Buffer size for streaming operations (64KB)
pub const STREAM_BUFFER_SIZE: usize = 64 * 1024;

/// Validation batch size to reduce database transaction overhead
pub const VALIDATION_BATCH_SIZE: usize = 10;

/// Safety signal sample cap to avoid large audit payloads
pub const SAFETY_SAMPLE_LIMIT: usize = 5;
pub const PROMPT_WARN_LEN: usize = 4096;
pub const PROMPT_BLOCK_LEN: usize = 12000;

pub fn dataset_quota_limits() -> (u64, u64) {
    let hard = std::env::var("AOS_DATASET_HARD_QUOTA_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DATASET_HARD_QUOTA_BYTES);
    let soft = std::env::var("AOS_DATASET_SOFT_QUOTA_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or((hard as f64 * DEFAULT_SOFT_PCT) as u64);
    (soft, hard)
}

pub fn quota_error(message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::new(message.into()).with_code("DATASET_QUOTA_EXCEEDED".to_string())),
    )
}

/// Map validation status: 'pending' -> 'pending' for API responses
pub fn map_validation_status(status: &str) -> DatasetValidationStatus {
    match status {
        "validating" => DatasetValidationStatus::Validating,
        "valid" => DatasetValidationStatus::Valid,
        "invalid" => DatasetValidationStatus::Invalid,
        "failed" => DatasetValidationStatus::Invalid,
        "pending" => DatasetValidationStatus::Pending,
        "skipped" => DatasetValidationStatus::Skipped,
        _ => DatasetValidationStatus::Pending,
    }
}

pub fn map_validation_errors(errors: Option<String>) -> Option<Vec<String>> {
    errors.map(|raw| {
        if let Ok(values) = serde_json::from_str::<Vec<String>>(&raw) {
            return values;
        }
        if let Ok(payload) = serde_json::from_str::<ValidationDiagnosticsPayload>(&raw) {
            if !payload.messages.is_empty() {
                return payload.messages;
            }
        }
        vec![raw]
    })
}

pub fn map_validation_diagnostics(
    errors: Option<String>,
) -> Option<Vec<JsonlValidationDiagnostic>> {
    errors.and_then(|raw| {
        serde_json::from_str::<ValidationDiagnosticsPayload>(&raw)
            .ok()
            .and_then(|payload| {
                if payload.diagnostics.is_empty() {
                    None
                } else {
                    Some(payload.diagnostics)
                }
            })
    })
}

pub fn build_validation_error_payload(errors: &[ValidationError]) -> Option<String> {
    if errors.is_empty() {
        return None;
    }

    let messages: Vec<String> = errors.iter().map(|err| err.to_string()).collect();
    let diagnostics: Vec<JsonlValidationDiagnostic> = errors
        .iter()
        .filter_map(|err| {
            let line_number = err.line_number?;
            let has_details = err.raw_snippet.is_some()
                || err.missing_fields.as_ref().is_some_and(|v| !v.is_empty())
                || err
                    .invalid_field_types
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
                || err.contract_version_expected.is_some();
            if !has_details {
                return None;
            }

            Some(JsonlValidationDiagnostic {
                line_number,
                raw_snippet: err.raw_snippet.clone(),
                missing_fields: err.missing_fields.clone(),
                invalid_field_types: err.invalid_field_types.as_ref().map(|items| {
                    items
                        .iter()
                        .map(|item| JsonlFieldTypeMismatch {
                            field: item.field.clone(),
                            expected: item.expected.clone(),
                            actual: item.actual.clone(),
                        })
                        .collect()
                }),
                contract_version_expected: err.contract_version_expected.clone(),
            })
        })
        .collect();

    if diagnostics.is_empty() {
        return Some(messages.join("; "));
    }

    serde_json::to_string(&ValidationDiagnosticsPayload {
        messages,
        diagnostics,
    })
    .ok()
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ValidationDiagnosticsPayload {
    #[serde(default)]
    messages: Vec<String>,
    #[serde(default)]
    diagnostics: Vec<JsonlValidationDiagnostic>,
}

/// Build a standardized error for path policy violations so the UI can map it.
pub fn path_policy_error(path: &Path, err: impl std::fmt::Display) -> ApiError {
    ApiError::bad_request("Path policy violation")
        .with_code("PATH_POLICY_VIOLATION")
        .with_json_details(serde_json::json!({
            "path": path.to_string_lossy(),
            "error": err.to_string(),
        }))
}

/// Validate file hash using streaming to avoid loading entire file into memory
pub async fn validate_file_hash_streaming(
    file_path: &std::path::Path,
    expected_hash: &str,
) -> Result<bool, String> {
    // Parse expected hash
    let expected =
        B3Hash::from_hex(expected_hash).map_err(|e| format!("Invalid hash format: {}", e))?;

    // Use IntegrityChecker for efficient streaming hash computation
    // Note: IntegrityChecker is from adapteros-model-hub which may not be available here
    // Fallback to manual streaming implementation
    let mut file = fs::File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut hasher = blake3::Hasher::new();

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        if n == 0 {
            break;
        }

        hasher.update(&buffer[..n]);
    }

    let computed = B3Hash::from_bytes(*hasher.finalize().as_bytes());
    Ok(computed == expected)
}

/// Ensure a dataset file path stays within the configured dataset root.
pub async fn ensure_dataset_file_within_root(
    state: &AppState,
    file_path: &std::path::Path,
) -> Result<std::path::PathBuf, ApiError> {
    let dataset_root =
        resolve_dataset_root(state).map_err(|e| ApiError::internal(e.to_string()))?;
    let candidate = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        dataset_root.join(file_path)
    };
    let allowed_roots = [dataset_root];
    let canonical =
        canonicalize_strict_in_allowed_roots(&candidate, &allowed_roots).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("allowed roots") || msg.contains("traversal") {
                ApiError::forbidden(format!("Dataset file path rejected: {}", msg))
            } else {
                ApiError::internal(format!(
                    "Failed to resolve dataset file path {}: {}",
                    candidate.display(),
                    msg
                ))
            }
        })?;
    Ok(canonical)
}

/// Batch insert file records to reduce database transaction overhead
/// Reserved for future optimized bulk insert operations
#[allow(dead_code)]
pub async fn batch_add_files(
    state: &AppState,
    dataset_id: &str,
    files: &[DatasetFile],
) -> Result<(), String> {
    for batch in files.chunks(VALIDATION_BATCH_SIZE) {
        for file in batch {
            state
                .db
                .add_dataset_file(
                    dataset_id,
                    &file.file_name,
                    &file.file_path,
                    file.size_bytes,
                    &file.hash_b3,
                    file.mime_type.as_deref(),
                )
                .await
                .map_err(|e| format!("Failed to add file record: {}", e))?;
        }
    }
    Ok(())
}

/// Stream file preview without loading entire file into memory
pub async fn stream_preview_file(
    file_path: &std::path::Path,
    format: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, String> {
    let mut file = fs::File::open(file_path)
        .await
        .map_err(|e| format!("Failed to open file: {}", e))?;

    let mut buffer = vec![0u8; STREAM_BUFFER_SIZE];
    let mut examples = Vec::new();
    let mut count = 0;

    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        if n == 0 {
            break;
        }

        if count >= limit {
            break;
        }

        let text = String::from_utf8_lossy(&buffer[..n]);
        for line in text.lines() {
            if count >= limit {
                break;
            }

            match format {
                "jsonl" => {
                    if !line.trim().is_empty() {
                        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line) {
                            examples.push(json_value);
                            count += 1;
                        }
                    }
                }
                "json" => {
                    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(line) {
                        if let Some(array) = json_value.as_array() {
                            for item in array.iter().take(limit - count) {
                                examples.push(item.clone());
                                count += 1;
                            }
                        } else {
                            examples.push(json_value);
                            count += 1;
                        }
                    }
                }
                "txt" | "text" => {
                    examples.push(serde_json::json!({ "text": line }));
                    count += 1;
                }
                _ => {
                    examples.push(serde_json::json!({ "content": line }));
                    count += 1;
                }
            }
        }
    }

    Ok(examples)
}

// ===== Safety Scan Types and Functions =====

#[derive(Default)]
pub struct SignalAccumulator {
    pub warn: usize,
    pub block: usize,
    pub reasons: Vec<String>,
    pub sample_row_ids: Vec<String>,
}

impl SignalAccumulator {
    pub fn note_warn(&mut self, reason: impl Into<String>, row_id: Option<&str>) {
        self.warn += 1;
        self.reasons.push(reason.into());
        if let Some(id) = row_id {
            push_sample(&mut self.sample_row_ids, id);
        }
    }

    pub fn note_block(&mut self, reason: impl Into<String>, row_id: Option<&str>) {
        self.block += 1;
        self.reasons.push(reason.into());
        if let Some(id) = row_id {
            push_sample(&mut self.sample_row_ids, id);
        }
    }

    pub fn status(&self) -> String {
        if self.block > 0 {
            "block".to_string()
        } else if self.warn > 0 {
            "warn".to_string()
        } else {
            "clean".to_string()
        }
    }
}

#[derive(Default)]
pub struct SafetyScanOutcome {
    pub pii: SignalAccumulator,
    pub toxicity: SignalAccumulator,
    pub leak: SignalAccumulator,
    pub anomaly: SignalAccumulator,
}

fn push_sample(target: &mut Vec<String>, row_id: &str) {
    if target.len() < SAFETY_SAMPLE_LIMIT {
        target.push(row_id.to_string());
    }
}

pub fn has_email_like_token(text: &str) -> bool {
    text.split_whitespace().any(|token| {
        let cleaned = token
            .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '@' && c != '.' && c != '-');
        let mut parts = cleaned.split('@');
        if let (Some(local), Some(domain)) = (parts.next(), parts.next()) {
            !local.is_empty() && domain.contains('.')
        } else {
            false
        }
    })
}

pub fn has_secret_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("aws_secret_access_key")
        || lower.contains("aws_access_key_id")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("-----begin")
}

pub fn has_toxic_marker(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["hate", "kill", "bomb", "terror", "violent"]
        .iter()
        .any(|marker| lower.contains(marker))
}

pub fn classify_row(
    row: &CanonicalRow,
    seen_ids: &mut HashSet<String>,
    outcome: &mut SafetyScanOutcome,
) {
    // Duplicate row_ids should not be allowed
    if !seen_ids.insert(row.row_id.clone()) {
        outcome
            .anomaly
            .note_block("duplicate_row_id", Some(&row.row_id));
    }

    // Length bounds
    let prompt_len = row.prompt.len();
    let response_len = row.response.len();
    if prompt_len > PROMPT_BLOCK_LEN || response_len > PROMPT_BLOCK_LEN {
        outcome
            .anomaly
            .note_block("text_too_long", Some(&row.row_id));
    } else if prompt_len > PROMPT_WARN_LEN || response_len > PROMPT_WARN_LEN {
        outcome
            .anomaly
            .note_warn("text_near_limit", Some(&row.row_id));
    }

    let combined = format!("{} {}", row.prompt, row.response);
    if has_email_like_token(&combined) {
        outcome
            .pii
            .note_warn("email_like_pattern", Some(&row.row_id));
    }
    if has_secret_marker(&combined) {
        outcome.leak.note_block("secret_marker", Some(&row.row_id));
    }
    if has_toxic_marker(&combined) {
        outcome
            .toxicity
            .note_warn("toxic_language", Some(&row.row_id));
    }
}

pub async fn run_tier2_safety_scan(path: &str) -> Result<SafetyScanOutcome, String> {
    use tokio::io::AsyncBufReadExt;
    use tokio::io::BufReader;

    let file = fs::File::open(path)
        .await
        .map_err(|e| format!("Failed to open dataset for safety scan: {}", e))?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    let mut outcome = SafetyScanOutcome::default();
    let mut seen_ids: HashSet<String> = HashSet::new();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| format!("Failed to read dataset line: {}", e))?
    {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<CanonicalRow>(trimmed) {
            Ok(row) => classify_row(&row, &mut seen_ids, &mut outcome),
            Err(e) => outcome
                .anomaly
                .note_warn(format!("row_parse_error:{e}"), None),
        }
    }

    Ok(outcome)
}

pub async fn record_safety_validation_runs(
    state: &AppState,
    dataset_version_id: &str,
    actor: &str,
    outcome: &SafetyScanOutcome,
) {
    let signals = [
        ("pii", &outcome.pii),
        ("toxicity", &outcome.toxicity),
        ("leak", &outcome.leak),
        ("anomaly", &outcome.anomaly),
    ];

    for (signal, acc) in signals {
        let status = match acc.status().as_str() {
            "block" => "block",
            "warn" => "warn",
            "clean" => "valid",
            _ => "pending",
        };
        let reasons_json = if acc.reasons.is_empty() {
            None
        } else {
            serde_json::to_string(&acc.reasons).ok()
        };
        let samples_json = if acc.sample_row_ids.is_empty() {
            None
        } else {
            serde_json::to_string(&acc.sample_row_ids).ok()
        };

        let _ = state
            .db
            .record_dataset_version_validation_run(
                dataset_version_id,
                "tier2_safety",
                status,
                Some(signal),
                reasons_json.as_deref(),
                samples_json.as_deref(),
                Some(actor),
                None,
                None,
                None,
            )
            .await;
    }
}

/// Spawn asynchronous tier2 safety validation (heuristic scan with trust gating).
pub fn spawn_tier2_safety_validation(state: AppState, dataset_version_id: String, actor: String) {
    tokio::spawn(async move {
        // Record pending safety validation
        let _ = state
            .db
            .record_dataset_version_validation_run(
                &dataset_version_id,
                "tier2_safety",
                "pending",
                Some("safety"),
                None,
                None,
                Some(actor.as_str()),
                None,
                None,
                None,
            )
            .await;

        let version = match state
            .db
            .get_training_dataset_version(&dataset_version_id)
            .await
        {
            Ok(Some(v)) => v,
            Ok(None) => {
                let _ = state
                    .db
                    .record_dataset_version_validation_run(
                        &dataset_version_id,
                        "tier2_safety",
                        "failed",
                        Some("safety"),
                        Some("Dataset version not found"),
                        None,
                        Some(actor.as_str()),
                        None,
                        None,
                        None,
                    )
                    .await;
                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                    )
                    .await;
                return;
            }
            Err(e) => {
                let msg = format!("Failed to load dataset version: {}", e);
                let _ = state
                    .db
                    .record_dataset_version_validation_run(
                        &dataset_version_id,
                        "tier2_safety",
                        "failed",
                        Some("safety"),
                        Some(msg.as_str()),
                        None,
                        Some(actor.as_str()),
                        None,
                        None,
                        None,
                    )
                    .await;
                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                    )
                    .await;
                return;
            }
        };

        match run_tier2_safety_scan(&version.storage_path).await {
            Ok(outcome) => {
                let pii_status = outcome.pii.status();
                let toxicity_status = outcome.toxicity.status();
                let leak_status = outcome.leak.status();
                let anomaly_status = outcome.anomaly.status();

                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some(pii_status.as_str()),
                        Some(toxicity_status.as_str()),
                        Some(leak_status.as_str()),
                        Some(anomaly_status.as_str()),
                    )
                    .await;

                record_safety_validation_runs(
                    &state,
                    &dataset_version_id,
                    actor.as_str(),
                    &outcome,
                )
                .await;
            }
            Err(err) => {
                let msg = err;
                let _ = state
                    .db
                    .record_dataset_version_validation_run(
                        &dataset_version_id,
                        "tier2_safety",
                        "failed",
                        Some("safety"),
                        Some(msg.as_str()),
                        None,
                        Some(actor.as_str()),
                        None,
                        None,
                        None,
                    )
                    .await;
                let _ = state
                    .db
                    .update_dataset_version_safety_status(
                        &dataset_version_id,
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                        Some("unknown"),
                    )
                    .await;
            }
        }
    });
}

#[cfg(test)]
mod path_policy_tests {
    use super::*;

    #[test]
    fn path_policy_error_is_structured() {
        let path = std::env::temp_dir().join("..").join("escape");
        let err = path_policy_error(&path, "outside allowed roots");
        assert_eq!(err.code, "PATH_POLICY_VIOLATION");
        let details = err.details.expect("details present");
        let serialized = details.to_string();
        assert!(serialized.contains("escape"));
        assert!(serialized.contains("outside allowed roots"));
    }
}

#[cfg(test)]
mod safety_scan_tests {
    use super::*;
    use crate::test_utils;

    fn mk_row(prompt: &str, response: &str, row_id: &str) -> CanonicalRow {
        CanonicalRow {
            row_id: row_id.to_string(),
            split: "train".into(),
            prompt: prompt.into(),
            response: response.into(),
            weight: 1.0,
            metadata: Default::default(),
        }
    }

    #[test]
    fn detects_email_secret_and_duplicates() {
        let mut outcome = SafetyScanOutcome::default();
        let mut seen = HashSet::new();
        let row1 = mk_row("reach me at user@example.com", "ok", "row-1");
        let row2 = mk_row("api_key=SECRET", "body", "row-2");
        let row3 = mk_row("neutral", "text", "row-1"); // duplicate id

        classify_row(&row1, &mut seen, &mut outcome);
        classify_row(&row2, &mut seen, &mut outcome);
        classify_row(&row3, &mut seen, &mut outcome);

        assert_eq!(outcome.pii.status(), "warn");
        assert_eq!(outcome.leak.status(), "block");
        assert_eq!(outcome.anomaly.status(), "block");
        assert!(outcome.pii.sample_row_ids.contains(&row1.row_id));
        assert!(outcome.leak.sample_row_ids.contains(&row2.row_id));
    }

    #[tokio::test]
    async fn safety_scan_marks_parse_errors_as_anomaly() {
        let tmp = test_utils::tempdir_with_prefix("aos-test-");
        let path = tmp.path().join("data.jsonl");
        let row = mk_row("ok prompt", "resp", "row-1");
        let content = format!(
            "{}\n{}\n",
            serde_json::to_string(&row).unwrap(),
            "{invalid json"
        );
        tokio::fs::write(&path, content).await.unwrap();

        let outcome = run_tier2_safety_scan(path.to_str().unwrap()).await.unwrap();
        assert!(matches!(
            outcome.anomaly.status().as_str(),
            "warn" | "block"
        ));
    }
}
