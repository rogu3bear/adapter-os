//! Handler for synthesizing training data from documents.
//!
//! This module provides the API endpoint for converting uploaded documents
//! into structured training datasets using the synthesis model.

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::services::SynthesisService;
use crate::state::AppState;
use crate::types::{ErrorResponse, MAX_TOKENS_LIMIT};
use adapteros_core::B3Hash;
use adapteros_orchestrator::synthesis::{create_synthesis_request, SynthesisBatchStats};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
use utoipa::ToSchema;

/// Request to synthesize training data from document chunks
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SynthesizeDatasetRequest {
    /// Name for the resulting dataset
    pub name: String,

    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Document chunks to synthesize from
    pub chunks: Vec<DocumentChunk>,

    /// Synthesis configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<SynthesisConfig>,

    /// Optional workspace ID for tenant isolation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// A document chunk to synthesize from
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentChunk {
    /// The text content of the chunk
    pub text: String,

    /// Source identifier (e.g., "document.pdf:page_3")
    pub source: String,

    /// Optional chunk index within the source
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_index: Option<usize>,
}

/// Configuration for synthesis
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SynthesisConfig {
    /// Temperature for generation (0.0 - 2.0, default 0.7)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum tokens to generate per chunk
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,

    /// Whether to include Q&A pairs
    #[serde(default = "default_true")]
    pub include_qa: bool,

    /// Whether to include instruction-following examples
    #[serde(default = "default_true")]
    pub include_instructions: bool,

    /// Whether to include completion examples
    #[serde(default = "default_true")]
    pub include_completions: bool,
}

fn default_temperature() -> f32 {
    0.7
}
fn default_max_tokens() -> usize {
    1024
}
fn default_true() -> bool {
    true
}

impl Default for SynthesisConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            include_qa: true,
            include_instructions: true,
            include_completions: true,
        }
    }
}

/// Response from synthesis
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SynthesizeDatasetResponse {
    /// ID of the created dataset
    pub dataset_id: String,

    /// Name of the dataset
    pub name: String,

    /// Number of chunks processed
    pub chunks_processed: usize,

    /// Total examples generated
    pub total_examples: usize,

    /// Breakdown by type
    pub examples_by_type: ExampleCounts,

    /// Parse success rate (0.0 - 1.0)
    pub success_rate: f32,

    /// Total processing time in milliseconds
    pub processing_time_ms: u64,

    /// Status message
    pub status: String,
}

/// Counts of examples by type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExampleCounts {
    pub qa_pairs: usize,
    pub instructions: usize,
    pub completions: usize,
}

/// Synthesize training data from document chunks
///
/// Takes document chunks and uses a local synthesis model to generate
/// structured training examples (Q&A pairs, instructions, completions).
///
/// The synthesis model runs locally on Apple Silicon using CoreML/ANE
/// for efficient inference.
#[utoipa::path(
    post,
    path = "/v1/datasets/synthesize",
    request_body = SynthesizeDatasetRequest,
    responses(
        (status = 200, description = "Dataset synthesized successfully", body = SynthesizeDatasetResponse),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Permission denied"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "Synthesis model not available")
    ),
    tag = "datasets"
)]
pub async fn synthesize_dataset(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<SynthesizeDatasetRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    // Validate request
    if request.chunks.is_empty() {
        return Err(ApiError::bad_request("At least one document chunk is required").into());
    }

    if request.name.trim().is_empty() {
        return Err(ApiError::bad_request("Dataset name is required").into());
    }

    // Limit chunks per request
    const MAX_CHUNKS_PER_REQUEST: usize = 100;
    if request.chunks.len() > MAX_CHUNKS_PER_REQUEST {
        return Err(ApiError::bad_request(format!(
            "Maximum {} chunks per request, got {}",
            MAX_CHUNKS_PER_REQUEST,
            request.chunks.len()
        ))
        .into());
    }

    // Get synthesis config and validate max_tokens
    let config = request.config.unwrap_or_default();
    if config.max_tokens > MAX_TOKENS_LIMIT {
        return Err(ApiError::bad_request(format!(
            "max_tokens ({}) exceeds maximum allowed ({})",
            config.max_tokens, MAX_TOKENS_LIMIT
        ))
        .into());
    }

    // Get or initialize the shared synthesis service (model loaded once)
    let service = SynthesisService::get_or_init(&state).await?;
    let engine = service.engine().clone();

    // Convert chunks to synthesis requests
    let synthesis_requests: Vec<_> = request
        .chunks
        .iter()
        .enumerate()
        .map(|(i, chunk)| {
            let source = if let Some(idx) = chunk.chunk_index {
                format!("{}:chunk_{}", chunk.source, idx)
            } else {
                format!("{}:chunk_{}", chunk.source, i)
            };
            create_synthesis_request(&chunk.text, &source)
        })
        .collect();

    // Run synthesis with write lock held for both config update and synthesis
    // This prevents race conditions where another request changes config mid-synthesis
    let (results, stats) = {
        let mut engine_guard = engine.write().await;

        // Update config for this request's parameters
        let mut new_config = engine_guard.config().clone();
        new_config.temperature = config.temperature;
        new_config.max_new_tokens = config.max_tokens;
        engine_guard.set_config(new_config);

        // Run synthesis while still holding the lock
        engine_guard
            .synthesize_batch(synthesis_requests)
            .await
            .map_err(|e| ApiError::internal(format!("Synthesis failed: {}", e)))?
    };

    // Convert synthesis results to training examples, applying config filters
    // Use the TrainingExample::new constructor which handles auto-classification
    use adapteros_orchestrator::synthesis::{ExampleType, TrainingExample};

    let training_examples: Vec<_> = results
        .iter()
        .filter(|r| r.parse_success)
        .flat_map(|r| {
            let mut examples = Vec::new();
            let source = &r.request.source;

            // Apply config filters - only include requested example types
            if config.include_qa {
                for (i, qa) in r.output.qa_pairs.iter().enumerate() {
                    examples.push(TrainingExample::new(
                        qa.question.clone(),
                        qa.answer.clone(),
                        ExampleType::QuestionAnswer,
                        format!("{}:qa_{}", source, i),
                        i,
                        qa.relevance, // Relevance auto-classifies sample_role
                    ));
                }
            }

            if config.include_instructions {
                for (i, inst) in r.output.instructions.iter().enumerate() {
                    examples.push(TrainingExample::new(
                        inst.instruction.clone(),
                        inst.response.clone(),
                        ExampleType::Instruction,
                        format!("{}:inst_{}", source, i),
                        i,
                        Some(0.8), // Instructions are generally high relevance
                    ));
                }
            }

            if config.include_completions {
                for (i, comp) in r.output.completions.iter().enumerate() {
                    examples.push(TrainingExample::new(
                        comp.context.clone(),
                        comp.continuation.clone(),
                        ExampleType::Completion,
                        format!("{}:comp_{}", source, i),
                        i,
                        Some(0.8), // Completions are generally high relevance
                    ));
                }
            }

            examples
        })
        .collect();

    // Calculate filtered stats
    let mut final_stats = SynthesisBatchStats::default();
    final_stats.chunks_processed = stats.chunks_processed;
    final_stats.parse_successes = stats.parse_successes;
    final_stats.parse_failures = stats.parse_failures;
    final_stats.total_latency_ms = stats.total_latency_ms;

    // Count actual examples by type from the filtered list
    for ex in &training_examples {
        match ex.example_type {
            ExampleType::QuestionAnswer => final_stats.qa_pairs += 1,
            ExampleType::Instruction => final_stats.instructions += 1,
            ExampleType::Completion => final_stats.completions += 1,
        }
    }

    // Persist dataset to storage and database
    let tenant_id = &claims.tenant_id;
    let created_by = Some(claims.sub.as_str());
    let workspace_id = request.workspace_id.as_deref();

    let (dataset_id, storage_path, hash_b3) = persist_synthesis_results(
        &state,
        &request.name,
        request.description.as_deref(),
        &training_examples,
        tenant_id,
        created_by,
        workspace_id,
    )
    .await
    .map_err(|e| ApiError::internal(format!("Failed to persist dataset: {}", e)))?;

    let response = SynthesizeDatasetResponse {
        dataset_id: dataset_id.clone(),
        name: request.name.clone(),
        chunks_processed: final_stats.chunks_processed,
        total_examples: training_examples.len(),
        examples_by_type: ExampleCounts {
            qa_pairs: final_stats.qa_pairs,
            instructions: final_stats.instructions,
            completions: final_stats.completions,
        },
        success_rate: final_stats.success_rate(),
        processing_time_ms: final_stats.total_latency_ms,
        status: if final_stats.parse_failures == 0 {
            "success".to_string()
        } else {
            format!(
                "completed_with_errors ({} parse failures)",
                final_stats.parse_failures
            )
        },
    };

    tracing::info!(
        dataset_id = %response.dataset_id,
        name = %response.name,
        chunks = response.chunks_processed,
        examples = response.total_examples,
        storage_path = %storage_path,
        hash_b3 = %hash_b3,
        success_rate = %format!("{:.1}%", response.success_rate * 100.0),
        "Dataset synthesis complete and persisted"
    );

    Ok(Json(response))
}

/// Persist synthesis results to storage and database
async fn persist_synthesis_results(
    state: &AppState,
    name: &str,
    description: Option<&str>,
    examples: &[adapteros_orchestrator::synthesis::TrainingExample],
    tenant_id: &str,
    created_by: Option<&str>,
    workspace_id: Option<&str>,
) -> Result<(String, String, String), adapteros_core::AosError> {
    use adapteros_core::AosError;

    // Generate dataset ID
    let dataset_id = crate::id_generator::readable_id(adapteros_id::IdPrefix::Dst, name);

    // Resolve storage path — block scope so RwLockReadGuard is dropped before any .await
    let datasets_root = {
        let config = state.config.read().map_err(|e| {
            tracing::error!(error = %e, "Failed to read dataset config");
            AosError::Internal(format!("Failed to read dataset config: {e}"))
        })?;
        PathBuf::from(&config.paths.datasets_root)
    };
    let dataset_dir = datasets_root.join(tenant_id).join(&dataset_id);

    // Create directory
    tokio::fs::create_dir_all(&dataset_dir)
        .await
        .map_err(|e| AosError::Io(format!("Failed to create dataset directory: {}", e)))?;

    // Write examples as JSONL
    let jsonl_path = dataset_dir.join("examples.jsonl");
    let mut file = tokio::fs::File::create(&jsonl_path)
        .await
        .map_err(|e| AosError::Io(format!("Failed to create JSONL file: {}", e)))?;

    let mut content = String::new();
    for example in examples {
        let json = serde_json::to_string(example)?;
        content.push_str(&json);
        content.push('\n');
    }

    file.write_all(content.as_bytes())
        .await
        .map_err(|e| AosError::Io(format!("Failed to write JSONL: {}", e)))?;

    file.flush()
        .await
        .map_err(|e| AosError::Io(format!("Failed to flush JSONL: {}", e)))?;

    // Compute hash of the file content
    let hash_b3 = B3Hash::hash(content.as_bytes()).to_string();

    // Store path relative to datasets root
    let storage_path = format!("{}/{}/examples.jsonl", tenant_id, dataset_id);

    // Create database records with our pre-generated ID to ensure consistency
    // 1. Create the dataset record
    state
        .db
        .create_training_dataset_with_id(
            &dataset_id, // use our pre-generated ID
            name,
            description,
            "jsonl",       // format
            &hash_b3,      // hash_b3
            &storage_path, // storage_path
            created_by,
            workspace_id,
            Some("synthesized"), // status
            Some(&hash_b3),      // dataset_hash_b3
            None,                // repo_slug
        )
        .await?;

    // 2. Create the dataset version record (required for training pipeline integration)
    let version_label = format!("v1-synthesis-{}", &dataset_id[..8]);
    let _version_id = state
        .db
        .create_training_dataset_version(
            &dataset_id,
            Some(tenant_id),
            Some(&version_label),
            &storage_path,
            &hash_b3,
            None, // manifest_path
            None, // manifest_json
            created_by,
        )
        .await?;

    tracing::debug!(
        dataset_id = %dataset_id,
        version_label = %version_label,
        storage_path = %storage_path,
        examples_count = examples.len(),
        "Persisted synthesis results with version"
    );

    Ok((dataset_id, storage_path, hash_b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthesis_config_defaults() {
        let config = SynthesisConfig::default();
        assert!((config.temperature - 0.7).abs() < 0.01);
        assert_eq!(config.max_tokens, 1024);
        assert!(config.include_qa);
        assert!(config.include_instructions);
        assert!(config.include_completions);
    }

    #[test]
    fn test_request_serialization() {
        let request = SynthesizeDatasetRequest {
            name: "test".to_string(),
            description: None,
            chunks: vec![DocumentChunk {
                text: "Test content".to_string(),
                source: "test.md".to_string(),
                chunk_index: Some(0),
            }],
            config: None,
            workspace_id: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: SynthesizeDatasetRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
        assert_eq!(parsed.chunks.len(), 1);
    }

    #[test]
    fn test_config_filtering_flags() {
        // Verify that config flags are independent
        let mut config = SynthesisConfig::default();
        assert!(config.include_qa);
        assert!(config.include_instructions);
        assert!(config.include_completions);

        config.include_qa = false;
        assert!(!config.include_qa);
        assert!(config.include_instructions);
        assert!(config.include_completions);
    }

    #[test]
    fn test_example_counts_serialization() {
        let counts = ExampleCounts {
            qa_pairs: 10,
            instructions: 5,
            completions: 3,
        };
        let json = serde_json::to_string(&counts).unwrap();
        assert!(json.contains("\"qa_pairs\":10"));
        assert!(json.contains("\"instructions\":5"));
        assert!(json.contains("\"completions\":3"));
    }
}
