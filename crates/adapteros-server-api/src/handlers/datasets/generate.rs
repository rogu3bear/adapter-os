//! Handler for generating datasets from raw files using local inference.
//!
//! This module provides an endpoint that accepts a file upload (e.g., README.md),
//! chunks the content, calls InferenceCore for each chunk with strategy-specific
//! prompts (QA or Summary), and writes the generated samples to a JSONL dataset.
//!
//! ## Synthetic Provenance
//!
//! Generated datasets include provenance metadata:
//! - `is_synthetic: true` - indicates the dataset was synthetically generated
//! - `source_model_hash` - BLAKE3 hash of the model used for generation
//! - `generation_receipt_digests` - BLAKE3 digests of per-chunk generation receipts

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::inference_core::InferenceCore;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::{ErrorResponse, InferenceRequestInternal};
use adapteros_api_types::training::{
    GenerateDatasetResponse, GeneratedSample, GenerationStrategy, SyntheticProvenance,
};
use adapteros_db::training_datasets::CreateDatasetParams;
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use tracing::{debug, info, warn};
use uuid::Uuid;

use super::fs_utils::ensure_dirs;
use super::hashing::{hash_dataset_manifest, hash_file, DatasetHashInput};
use super::helpers::MAX_FILE_SIZE;
use super::paths::{resolve_dataset_root, DatasetPaths};
use super::tenant::bind_dataset_to_tenant;

/// Maximum number of samples to include in the preview
const PREVIEW_LIMIT: usize = 25;

/// Maximum number of chunks to process (to prevent runaway generation)
const MAX_CHUNKS: usize = 100;

/// Minimum chunk size to be considered valid
const MIN_CHUNK_SIZE: usize = 50;

/// Prompt template for QA generation
const QA_SYSTEM_PROMPT: &str = r#"You are a training data generator. Given a text passage, generate exactly one high-quality question-answer pair that tests understanding of the content.

Output format (JSON only, no other text):
{"question": "...", "answer": "..."}

Requirements:
- Question should be specific and answerable from the passage
- Answer should be concise but complete
- Do not include information not in the passage"#;

/// Prompt template for Summary generation
const SUMMARY_SYSTEM_PROMPT: &str = r#"You are a training data generator. Given a text passage, generate a summary instruction-response pair.

Output format (JSON only, no other text):
{"instruction": "Summarize the following text.", "response": "..."}

Requirements:
- Response should be a concise summary (2-3 sentences)
- Capture the key points from the passage
- Use clear, professional language"#;

/// Simple character-based text chunking with paragraph/sentence boundary preference
fn chunk_text(text: &str, chunk_size: usize) -> Vec<(usize, String)> {
    let mut chunks = Vec::new();
    let mut start = 0;
    let text_len = text.len();
    let effective_chunk_size = chunk_size.clamp(500, 10000);

    while start < text_len && chunks.len() < MAX_CHUNKS {
        let mut end = (start + effective_chunk_size).min(text_len);

        // Try to break at paragraph or sentence boundary
        if end < text_len {
            // Look for paragraph break first
            if let Some(para_break) = text[start..end].rfind("\n\n") {
                if para_break > MIN_CHUNK_SIZE {
                    end = start + para_break + 2;
                }
            }
            // Then sentence boundary
            else if let Some(sent_break) = text[start..end].rfind(". ") {
                if sent_break > MIN_CHUNK_SIZE {
                    end = start + sent_break + 2;
                }
            }
        }

        let chunk = text[start..end].trim().to_string();
        if !chunk.is_empty() && chunk.len() >= MIN_CHUNK_SIZE {
            chunks.push((chunks.len(), chunk));
        }
        start = end;
    }

    chunks
}

/// Parse generated JSON from model output
fn parse_generated_pair(output: &str, strategy: GenerationStrategy) -> Option<(String, String)> {
    // Try to extract JSON from output (model might include extra text)
    let json_start = output.find('{')?;
    let json_end = output.rfind('}')? + 1;
    let json_str = &output[json_start..json_end];

    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let obj = value.as_object()?;

    match strategy {
        GenerationStrategy::Qa => {
            let question = obj.get("question")?.as_str()?.trim().to_string();
            let answer = obj.get("answer")?.as_str()?.trim().to_string();
            if question.is_empty() || answer.is_empty() {
                return None;
            }
            Some((question, answer))
        }
        GenerationStrategy::Summary => {
            let instruction = obj.get("instruction")?.as_str()?.trim().to_string();
            let response = obj.get("response")?.as_str()?.trim().to_string();
            if instruction.is_empty() || response.is_empty() {
                return None;
            }
            Some((instruction, response))
        }
    }
}

/// Generate a training dataset from an uploaded file using local inference
///
/// Accepts a multipart form with:
/// - `file`: The text file to generate from (required)
/// - `name`: Dataset name (optional, auto-generated from filename if empty)
/// - `strategy`: Generation strategy - "qa" or "summary" (default: qa)
/// - `chunk_size`: Target chunk size in characters (default: 2000)
/// - `max_tokens`: Max tokens per inference call (default: 512)
/// - `description`: Optional dataset description
#[utoipa::path(
    post,
    path = "/v1/training/datasets/generate",
    responses(
        (status = 200, description = "Dataset generated successfully", body = GenerateDatasetResponse),
        (status = 400, description = "Invalid request - empty file, oversize, unknown strategy"),
        (status = 413, description = "File too large"),
        (status = 500, description = "Internal server error - inference failure")
    ),
    tag = "datasets"
)]
pub async fn generate_dataset_from_file(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Parse multipart form
    let mut file_content: Option<String> = None;
    let mut file_name = String::new();
    let mut name = String::new();
    let mut strategy = GenerationStrategy::Qa;
    let mut chunk_size: usize = 2000;
    let mut max_tokens: u32 = 512;
    let mut description: Option<String> = None;
    let mut target_volume: usize = 0;
    let mut generation_seed: Option<u64> = None;
    let mut seed_prompts: Vec<String> = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(format!("Failed to read multipart field: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "file" => {
                file_name = field.file_name().unwrap_or("input.txt").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read file: {}", e)))?;

                if data.is_empty() {
                    return Err(ApiError::bad_request("File is empty").into());
                }
                if data.len() > MAX_FILE_SIZE {
                    return Err(ApiError::payload_too_large("File exceeds maximum size").into());
                }

                file_content = Some(
                    String::from_utf8(data.to_vec())
                        .map_err(|_| ApiError::bad_request("File must be valid UTF-8 text"))?,
                );
            }
            "name" => {
                name = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read name: {}", e)))?;
            }
            "strategy" => {
                let strategy_str = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read strategy: {}", e)))?;
                strategy = match strategy_str.to_lowercase().as_str() {
                    "qa" => GenerationStrategy::Qa,
                    "summary" => GenerationStrategy::Summary,
                    _ => {
                        return Err(ApiError::bad_request(format!(
                            "Unknown strategy '{}'. Valid: qa, summary",
                            strategy_str
                        ))
                        .into())
                    }
                };
            }
            "chunk_size" => {
                let size_str = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read chunk_size: {}", e)))?;
                chunk_size = size_str
                    .parse()
                    .map_err(|_| ApiError::bad_request("chunk_size must be a number"))?;
            }
            "max_tokens" => {
                let tokens_str = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read max_tokens: {}", e)))?;
                max_tokens = tokens_str
                    .parse()
                    .map_err(|_| ApiError::bad_request("max_tokens must be a number"))?;
            }
            "description" => {
                let desc = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read description: {}", e)))?;
                if !desc.trim().is_empty() {
                    description = Some(desc);
                }
            }
            "target_volume" => {
                let vol_str = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read target_volume: {}", e)))?;
                target_volume = vol_str
                    .parse()
                    .map_err(|_| ApiError::bad_request("target_volume must be a number"))?;
            }
            "generation_seed" => {
                let seed_str = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read generation_seed: {}", e)))?;
                if !seed_str.trim().is_empty() {
                    generation_seed = Some(
                        seed_str
                            .parse()
                            .map_err(|_| ApiError::bad_request("generation_seed must be a number"))?,
                    );
                }
            }
            "seed_prompts" => {
                let prompts_str = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(format!("Failed to read seed_prompts: {}", e)))?;
                seed_prompts = prompts_str
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    let content = file_content.ok_or_else(|| ApiError::bad_request("No file uploaded"))?;

    // Default name from filename if not provided
    if name.is_empty() {
        name = format!("Generated from {}", file_name);
    }

    // Chunk the content
    let mut chunks = chunk_text(&content, chunk_size);
    if chunks.is_empty() {
        return Err(ApiError::bad_request(
            "File content too short to generate samples (minimum 50 characters per chunk)",
        )
        .into());
    }

    // Limit chunks if target_volume is specified
    if target_volume > 0 && chunks.len() > target_volume {
        chunks.truncate(target_volume);
    }

    info!(
        file_name = %file_name,
        chunks = chunks.len(),
        strategy = %strategy,
        chunk_size,
        max_tokens,
        target_volume,
        generation_seed = ?generation_seed,
        seed_prompts_count = seed_prompts.len(),
        "Generating synthetic dataset from file"
    );

    // Generate samples using InferenceCore
    let core = InferenceCore::new(&state);
    let system_prompt = match strategy {
        GenerationStrategy::Qa => QA_SYSTEM_PROMPT,
        GenerationStrategy::Summary => SUMMARY_SYSTEM_PROMPT,
    };

    let mut samples: Vec<GeneratedSample> = Vec::new();
    let mut failed_chunks = 0usize;
    let mut total_tokens = 0u64;
    let mut generation_receipt_digests: Vec<String> = Vec::new();
    let mut source_model_hash: Option<String> = None;

    // Use seed prompts if provided, otherwise use chunks directly
    let use_seed_prompts = !seed_prompts.is_empty();

    for (chunk_idx, chunk_text) in &chunks {
        // Build prompt with optional seed prompt context
        let prompt = if use_seed_prompts && *chunk_idx < seed_prompts.len() {
            format!(
                "{}\n\nContext hint: {}\n\nPassage:\n{}\n\nGenerate the JSON output:",
                system_prompt, seed_prompts[*chunk_idx], chunk_text
            )
        } else {
            format!(
                "{}\n\nPassage:\n{}\n\nGenerate the JSON output:",
                system_prompt, chunk_text
            )
        };

        let mut internal_request = InferenceRequestInternal::new(claims.tenant_id.clone(), prompt);
        internal_request.max_tokens = max_tokens as usize;
        // Use deterministic temperature if seed is provided
        internal_request.temperature = if generation_seed.is_some() { 0.0 } else { 0.7 };
        internal_request.stream = false;

        // Apply generation seed if provided (for determinism)
        if let Some(seed) = generation_seed {
            internal_request.seed = Some(seed + *chunk_idx as u64);
        }

        match core
            .route_and_infer(internal_request, None, None, None)
            .await
        {
            Ok(result) => {
                total_tokens += result.tokens_generated as u64;

                // Collect provenance from run receipt
                if let Some(ref receipt) = result.run_receipt {
                    let digest_hex = receipt.receipt_digest.to_hex();
                    generation_receipt_digests.push(digest_hex);
                    debug!(
                        chunk_idx,
                        receipt_digest = %receipt.receipt_digest.to_hex(),
                        "Collected generation receipt"
                    );
                }

                if let Some((instruction, response)) = parse_generated_pair(&result.text, strategy)
                {
                    samples.push(GeneratedSample {
                        instruction,
                        response,
                        source_chunk_index: *chunk_idx,
                    });
                } else {
                    warn!(
                        chunk_idx,
                        output = %result.text,
                        "Failed to parse generated output as JSON"
                    );
                    failed_chunks += 1;
                }
            }
            Err(e) => {
                warn!(chunk_idx, error = %e, "Inference failed for chunk");
                failed_chunks += 1;
            }
        }
    }

    // Try to get source model hash from worker runtime info
    for entry in state.worker_runtime.iter() {
        if let Some(ref hash) = entry.value().model_hash {
            source_model_hash = Some(hash.clone());
            break;
        }
    }

    if samples.is_empty() {
        return Err(ApiError::internal(
            "Failed to generate any valid samples. Check that inference is working.",
        )
        .into());
    }

    // Write JSONL file
    let dataset_id = Uuid::now_v7().to_string();
    let dataset_root = resolve_dataset_root(&state).map_err(|e| ApiError::internal(e.to_string()))?;
    let paths = DatasetPaths::new(dataset_root);

    ensure_dirs([paths.files.as_path()]).await?;

    // Build JSONL content
    let jsonl_content: String = samples
        .iter()
        .map(|s| {
            serde_json::json!({
                "prompt": s.instruction,
                "response": s.response,
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");

    let jsonl_bytes = jsonl_content.as_bytes();
    let file_hash = hash_file(jsonl_bytes);

    // Create manifest for dataset hash
    let manifest = vec![DatasetHashInput {
        file_name: "data.jsonl".to_string(),
        size_bytes: jsonl_bytes.len() as u64,
        file_hash_b3: file_hash.clone(),
    }];
    let dataset_hash = hash_dataset_manifest(&manifest);

    // Write to storage
    let storage_path = paths.files.join(&dataset_id);
    tokio::fs::create_dir_all(&storage_path)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create dataset dir: {}", e)))?;

    let data_file = storage_path.join("data.jsonl");
    tokio::fs::write(&data_file, &jsonl_content)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to write dataset: {}", e)))?;

    // Build synthetic provenance for manifest
    let provenance = SyntheticProvenance {
        is_synthetic: true,
        source_model_hash: source_model_hash.clone(),
        generation_receipt_digests: generation_receipt_digests.clone(),
        strategy: Some(strategy.as_str().to_string()),
        generation_seed,
        generated_at: Some(chrono::Utc::now().to_rfc3339()),
    };

    // Write manifest with provenance
    let manifest_json = serde_json::json!({
        "name": name,
        "description": description.as_deref().unwrap_or("Auto-generated synthetic dataset"),
        "version": "1.0",
        "training_contract_version": "1.0",
        "generation_strategy": strategy.as_str(),
        "sample_count": samples.len(),
        "is_synthetic": true,
        "provenance": provenance,
        "entries": [
            {
                "path": "data.jsonl",
                "format": "jsonl",
                "weight": 1.0,
                "role": "training",
                "notes": format!("Synthetically generated from {} using {} strategy", file_name, strategy)
            }
        ]
    });

    let manifest_file = storage_path.join("manifest.json");
    let manifest_content = serde_json::to_string_pretty(&manifest_json)
        .map_err(|e| ApiError::internal(format!("Failed to serialize manifest: {}", e)))?;
    tokio::fs::write(&manifest_file, manifest_content)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to write manifest: {}", e)))?;

    // Create DB record
    let dataset_params = CreateDatasetParams::builder()
        .id(&dataset_id)
        .name(&name)
        .format("jsonl")
        .hash_b3(&dataset_hash)
        .dataset_hash_b3(&dataset_hash)
        .storage_path(storage_path.to_string_lossy())
        .status("ready")
        .created_by(&claims.sub)
        .tenant_id(&claims.tenant_id)
        .dataset_type("training")
        .collection_method("pipeline")
        .category("synthetic")
        .build()
        .map_err(|e| ApiError::internal(format!("Failed to build dataset params: {}", e)))?;

    let (_, dataset_version_id) = state
        .db
        .create_training_dataset_from_params_with_version(
            &dataset_params,
            None, // version_label
            &storage_path.to_string_lossy(),
            &dataset_hash,
            None, // manifest_path
            None, // manifest_json
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to create dataset record: {}", e)))?;

    bind_dataset_to_tenant(&state.db, &dataset_id, &claims.tenant_id).await?;

    info!(
        dataset_id = %dataset_id,
        samples = samples.len(),
        failed_chunks,
        total_tokens,
        "Generated dataset successfully"
    );

    // Build preview (limited to PREVIEW_LIMIT)
    let preview: Vec<GeneratedSample> = samples.iter().take(PREVIEW_LIMIT).cloned().collect();

    Ok(Json(GenerateDatasetResponse {
        schema_version: adapteros_api_types::schema_version(),
        dataset_id,
        dataset_version_id: Some(dataset_version_id),
        name,
        sample_count: samples.len(),
        total_tokens_used: total_tokens,
        preview,
        failed_chunks,
        dataset_hash_b3: Some(dataset_hash),
        is_synthetic: true,
        source_model_hash,
        generation_receipt_digests,
        generation_seed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_basic() {
        let text = "This is a test paragraph.\n\nThis is another paragraph.";
        let chunks = chunk_text(text, 100);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_respects_max_chunks() {
        let text = "a".repeat(10000);
        let chunks = chunk_text(&text, 50);
        assert!(chunks.len() <= MAX_CHUNKS);
    }

    #[test]
    fn test_parse_qa_json() {
        let output = r#"{"question": "What is X?", "answer": "X is Y."}"#;
        let result = parse_generated_pair(output, GenerationStrategy::Qa);
        assert!(result.is_some());
        let (q, a) = result.unwrap();
        assert_eq!(q, "What is X?");
        assert_eq!(a, "X is Y.");
    }

    #[test]
    fn test_parse_summary_json() {
        let output = r#"{"instruction": "Summarize this.", "response": "The text discusses..."}"#;
        let result = parse_generated_pair(output, GenerationStrategy::Summary);
        assert!(result.is_some());
        let (i, r) = result.unwrap();
        assert_eq!(i, "Summarize this.");
        assert_eq!(r, "The text discusses...");
    }

    #[test]
    fn test_parse_json_with_extra_text() {
        let output = "Here is the output:\n{\"question\": \"Test?\", \"answer\": \"Yes.\"}\n";
        let result = parse_generated_pair(output, GenerationStrategy::Qa);
        assert!(result.is_some());
    }

    #[test]
    fn test_parse_invalid_json() {
        let output = "This is not JSON at all";
        let result = parse_generated_pair(output, GenerationStrategy::Qa);
        assert!(result.is_none());
    }
}
