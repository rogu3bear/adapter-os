//! Train from confirmed discrepancy cases
//!
//! This command exports confirmed error discrepancy cases to JSONL format
//! compatible with the training pipeline. Cases can be written to stdout,
//! a file, or appended to an existing dataset.
//!
//! # Usage
//!
//! ```bash
//! # Export to stdout
//! aosctl train-from-discrepancies
//!
//! # Export confirmed errors to a file
//! aosctl train-from-discrepancies --status confirmed_error --output training.jsonl
//!
//! # Append to existing dataset (creates new version)
//! aosctl train-from-discrepancies --dataset ds-123
//!
//! # Dry run to preview what would be exported
//! aosctl train-from-discrepancies --dry-run
//! ```

use crate::auth_store::{load_auth, warn_if_tenant_mismatch};
use crate::http_client::send_with_refresh_from_store;
use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use clap::Args;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

/// Generate training data from confirmed discrepancy cases
#[derive(Args, Debug, Clone)]
#[command(name = "train-from-discrepancies")]
#[command(about = "Generate training data from confirmed discrepancy cases")]
#[command(after_help = r#"
Examples:
  # Export confirmed errors to JSONL (stdout)
  aosctl train-from-discrepancies

  # Export to a file
  aosctl train-from-discrepancies --output discrepancies.jsonl

  # Filter by resolution status
  aosctl train-from-discrepancies --status confirmed_error

  # Append to existing dataset (creates new version)
  aosctl train-from-discrepancies --dataset ds-abc123

  # Dry run - show what would be exported
  aosctl train-from-discrepancies --dry-run
"#)]
pub struct TrainFromDiscrepanciesArgs {
    /// Resolution status to filter by
    ///
    /// Valid values: open, confirmed_error, not_an_error, model_limitation, needs_review
    #[arg(long, default_value = "confirmed_error")]
    pub status: String,

    /// Target dataset ID to append training pairs
    ///
    /// If provided, the exported discrepancies will be appended to this dataset
    /// as a new version. If not provided, output goes to --output or stdout.
    #[arg(long)]
    pub dataset: Option<String>,

    /// Output file for JSONL export
    ///
    /// If not provided and --dataset is not set, output goes to stdout.
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Dry run - show what would be done without making changes
    ///
    /// Prints a summary of discrepancy cases that would be exported
    /// without actually writing any files or creating dataset versions.
    #[arg(long)]
    pub dry_run: bool,

    /// Include cases without ground truth
    ///
    /// By default, only cases with ground_truth set are exported.
    /// Enable this to include cases that only have user_question and model_answer.
    #[arg(long)]
    pub include_incomplete: bool,
}

/// Export format matching the discrepancy API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscrepancyExportRow {
    pub id: String,
    pub inference_id: String,
    pub discrepancy_type: String,
    pub user_question: Option<String>,
    pub model_answer: Option<String>,
    pub ground_truth: Option<String>,
    pub document_id: Option<String>,
    pub chunk_hash_b3: Option<String>,
    pub confirmed_at: String,
}

/// Training pair in JSONL format compatible with training pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingPair {
    /// Input text (user question)
    pub input: String,
    /// Target text (ground truth or corrected answer)
    pub output: String,
    /// Provenance metadata as JSON string
    pub provenance: String,
}

/// Provenance metadata for training pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProvenanceMetadata {
    schema: String,
    source: String,
    discrepancy_id: String,
    inference_id: String,
    discrepancy_type: String,
    document_id: Option<String>,
    chunk_hash_b3: Option<String>,
}

/// Valid resolution statuses
const VALID_STATUSES: &[&str] = &[
    "open",
    "confirmed_error",
    "not_an_error",
    "model_limitation",
    "needs_review",
    "pending",
];

impl TrainFromDiscrepanciesArgs {
    /// Execute the train-from-discrepancies command
    pub async fn execute(&self, output: &OutputWriter) -> Result<()> {
        // Validate status
        if !VALID_STATUSES.contains(&self.status.as_str()) {
            return Err(AosError::Validation(format!(
                "Invalid status '{}'. Valid values: {}",
                self.status,
                VALID_STATUSES.join(", ")
            )));
        }

        // Load auth
        let _auth = load_auth()
            .map_err(|e| AosError::Io(format!("Failed to load auth: {e}")))?
            .ok_or_else(|| AosError::Validation("No stored auth; run `aosctl auth login`".into()))?;

        warn_if_tenant_mismatch(None, output);

        // Create HTTP client
        let client = Client::builder()
            .cookie_store(true)
            .build()
            .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;

        // Fetch discrepancy cases from API
        let discrepancies = self.fetch_discrepancies(&client).await?;

        if discrepancies.is_empty() {
            if output.is_json() {
                output.json(&serde_json::json!({
                    "status": self.status,
                    "count": 0,
                    "message": "No discrepancy cases found with this status"
                }))?;
            } else {
                output.warning(format!(
                    "No discrepancy cases found with status '{}'",
                    self.status
                ));
            }
            return Ok(());
        }

        // Filter and convert to training pairs
        let training_pairs = self.convert_to_training_pairs(&discrepancies)?;

        if training_pairs.is_empty() {
            if output.is_json() {
                output.json(&serde_json::json!({
                    "status": self.status,
                    "discrepancy_count": discrepancies.len(),
                    "training_pair_count": 0,
                    "message": "No valid training pairs (missing ground_truth or user_question)"
                }))?;
            } else {
                output.warning(format!(
                    "Found {} discrepancy cases but none have usable training data (ground_truth required)",
                    discrepancies.len()
                ));
                if !self.include_incomplete {
                    output.info("Use --include-incomplete to include cases without ground_truth");
                }
            }
            return Ok(());
        }

        // Dry run mode
        if self.dry_run {
            return self.print_dry_run(&discrepancies, &training_pairs, output);
        }

        // Execute the export
        if let Some(ref dataset_id) = self.dataset {
            self.append_to_dataset(&client, dataset_id, &training_pairs, output)
                .await
        } else {
            self.write_jsonl(&training_pairs, output)
        }
    }

    /// Fetch discrepancies from the API
    async fn fetch_discrepancies(&self, client: &Client) -> Result<Vec<DiscrepancyExportRow>> {
        // Use the export endpoint which returns JSONL
        let resp = send_with_refresh_from_store(client, |client, store| {
            let url = format!(
                "{}/v1/discrepancies?status={}&limit=1000",
                store.base_url.trim_end_matches('/'),
                self.status
            );
            client.get(url).bearer_auth(&store.token)
        })
        .await
        .map_err(|e| AosError::Io(format!("Failed to fetch discrepancies: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AosError::Io(format!(
                "API returned error {}: {}",
                status, body
            )));
        }

        // The list endpoint returns an array of discrepancy responses
        #[derive(Debug, Deserialize)]
        struct DiscrepancyResponse {
            id: String,
            inference_id: String,
            discrepancy_type: String,
            user_question: Option<String>,
            model_answer: Option<String>,
            ground_truth: Option<String>,
            document_id: Option<String>,
            chunk_hash_b3: Option<String>,
            resolved_at: Option<String>,
            updated_at: String,
        }

        let items: Vec<DiscrepancyResponse> = resp.json().await.map_err(|e| {
            AosError::Io(format!("Failed to parse discrepancies response: {e}"))
        })?;

        let exports: Vec<DiscrepancyExportRow> = items
            .into_iter()
            .map(|r| DiscrepancyExportRow {
                id: r.id,
                inference_id: r.inference_id,
                discrepancy_type: r.discrepancy_type,
                user_question: r.user_question,
                model_answer: r.model_answer,
                ground_truth: r.ground_truth,
                document_id: r.document_id,
                chunk_hash_b3: r.chunk_hash_b3,
                confirmed_at: r.resolved_at.unwrap_or(r.updated_at),
            })
            .collect();

        Ok(exports)
    }

    /// Convert discrepancy rows to training pairs
    fn convert_to_training_pairs(
        &self,
        discrepancies: &[DiscrepancyExportRow],
    ) -> Result<Vec<TrainingPair>> {
        let mut pairs = Vec::new();

        for disc in discrepancies {
            // Must have user_question
            let input = match &disc.user_question {
                Some(q) if !q.trim().is_empty() => q.clone(),
                _ => continue,
            };

            // Prefer ground_truth, fall back to model_answer if --include-incomplete
            let output_text = if let Some(ref gt) = disc.ground_truth {
                if !gt.trim().is_empty() {
                    gt.clone()
                } else if self.include_incomplete {
                    if let Some(ref ma) = disc.model_answer {
                        if !ma.trim().is_empty() {
                            ma.clone()
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else if self.include_incomplete {
                if let Some(ref ma) = disc.model_answer {
                    if !ma.trim().is_empty() {
                        ma.clone()
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let provenance = ProvenanceMetadata {
                schema: "supervised".to_string(),
                source: "discrepancy_case".to_string(),
                discrepancy_id: disc.id.clone(),
                inference_id: disc.inference_id.clone(),
                discrepancy_type: disc.discrepancy_type.clone(),
                document_id: disc.document_id.clone(),
                chunk_hash_b3: disc.chunk_hash_b3.clone(),
            };

            pairs.push(TrainingPair {
                input,
                output: output_text,
                provenance: serde_json::to_string(&provenance)
                    .map_err(|e| AosError::Serialization(e))?,
            });
        }

        Ok(pairs)
    }

    /// Print dry run summary
    fn print_dry_run(
        &self,
        discrepancies: &[DiscrepancyExportRow],
        training_pairs: &[TrainingPair],
        output: &OutputWriter,
    ) -> Result<()> {
        if output.is_json() {
            let preview: Vec<_> = training_pairs
                .iter()
                .take(5)
                .map(|p| {
                    serde_json::json!({
                        "input_preview": p.input.chars().take(100).collect::<String>(),
                        "output_preview": p.output.chars().take(100).collect::<String>(),
                    })
                })
                .collect();

            output.json(&serde_json::json!({
                "dry_run": true,
                "status": self.status,
                "discrepancy_count": discrepancies.len(),
                "training_pair_count": training_pairs.len(),
                "output_path": self.output.as_ref().map(|p| p.display().to_string()),
                "target_dataset": self.dataset,
                "preview": preview
            }))?;
        } else {
            output.section("Dry Run Summary");
            output.kv("Status filter", &self.status);
            output.kv("Discrepancy cases found", &discrepancies.len().to_string());
            output.kv("Valid training pairs", &training_pairs.len().to_string());

            if let Some(ref path) = self.output {
                output.kv("Output path", &path.display().to_string());
            } else if let Some(ref ds) = self.dataset {
                output.kv("Target dataset", ds);
            } else {
                output.kv("Output", "stdout");
            }

            if !training_pairs.is_empty() {
                output.section("Sample Training Pairs (first 3)");
                for (i, pair) in training_pairs.iter().take(3).enumerate() {
                    let input_preview: String = pair.input.chars().take(80).collect();
                    let output_preview: String = pair.output.chars().take(80).collect();
                    output.info(format!("{}. Input: {}...", i + 1, input_preview));
                    output.info(format!("   Output: {}...", output_preview));
                }
            }

            output.info("Run without --dry-run to execute");
        }

        Ok(())
    }

    /// Write training pairs as JSONL to file or stdout
    fn write_jsonl(&self, training_pairs: &[TrainingPair], output: &OutputWriter) -> Result<()> {
        let jsonl_content: String = training_pairs
            .iter()
            .filter_map(|p| serde_json::to_string(p).ok())
            .collect::<Vec<_>>()
            .join("\n");

        if let Some(ref path) = self.output {
            // Write to file
            let mut file = std::fs::File::create(path)
                .map_err(|e| AosError::Io(format!("Failed to create output file: {e}")))?;

            file.write_all(jsonl_content.as_bytes())
                .map_err(|e| AosError::Io(format!("Failed to write output: {e}")))?;

            file.write_all(b"\n")
                .map_err(|e| AosError::Io(format!("Failed to write newline: {e}")))?;

            if output.is_json() {
                output.json(&serde_json::json!({
                    "status": "success",
                    "path": path.display().to_string(),
                    "training_pair_count": training_pairs.len(),
                    "bytes_written": jsonl_content.len() + 1
                }))?;
            } else {
                output.section("Export Complete");
                output.kv("Output file", &path.display().to_string());
                output.kv("Training pairs", &training_pairs.len().to_string());
                output.kv("Bytes written", &(jsonl_content.len() + 1).to_string());
            }

            info!(
                path = %path.display(),
                count = training_pairs.len(),
                "Exported discrepancy training pairs"
            );
        } else {
            // Write to stdout
            println!("{}", jsonl_content);
        }

        Ok(())
    }

    /// Append training pairs to an existing dataset (creates new version)
    async fn append_to_dataset(
        &self,
        client: &Client,
        dataset_id: &str,
        training_pairs: &[TrainingPair],
        output: &OutputWriter,
    ) -> Result<()> {
        // Convert training pairs to JSONL bytes
        let jsonl_content: String = training_pairs
            .iter()
            .filter_map(|p| serde_json::to_string(p).ok())
            .collect::<Vec<_>>()
            .join("\n");

        let file_bytes = format!("{}\n", jsonl_content).into_bytes();

        // Upload as a new version via multipart
        let resp = send_with_refresh_from_store(client, |client, store| {
            let url = format!(
                "{}/v1/datasets/upload",
                store.base_url.trim_end_matches('/')
            );

            let part = reqwest::multipart::Part::bytes(file_bytes.clone())
                .file_name("discrepancy_training.jsonl");

            let form = reqwest::multipart::Form::new()
                .text("dataset_id", dataset_id.to_string())
                .text("format", "jsonl")
                .text("description", format!("Training data from {} discrepancy cases", training_pairs.len()))
                .part("file", part);

            client.post(url).bearer_auth(&store.token).multipart(form)
        })
        .await
        .map_err(|e| AosError::Io(format!("Failed to upload to dataset: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(AosError::Io(format!(
                "Dataset upload failed with {}: {}",
                status, body
            )));
        }

        #[derive(Deserialize)]
        struct UploadResponse {
            dataset_id: String,
            dataset_version_id: Option<String>,
        }

        let upload_resp: UploadResponse = resp
            .json()
            .await
            .map_err(|e| AosError::Io(format!("Failed to parse upload response: {e}")))?;

        if output.is_json() {
            output.json(&serde_json::json!({
                "status": "success",
                "dataset_id": upload_resp.dataset_id,
                "dataset_version_id": upload_resp.dataset_version_id,
                "training_pair_count": training_pairs.len()
            }))?;
        } else {
            output.section("Dataset Updated");
            output.kv("Dataset ID", &upload_resp.dataset_id);
            if let Some(ref vid) = upload_resp.dataset_version_id {
                output.kv("Version ID", vid);
            }
            output.kv("Training pairs added", &training_pairs.len().to_string());
        }

        info!(
            dataset_id = %upload_resp.dataset_id,
            count = training_pairs.len(),
            "Appended discrepancy training pairs to dataset"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_statuses() {
        assert!(VALID_STATUSES.contains(&"confirmed_error"));
        assert!(VALID_STATUSES.contains(&"open"));
        assert!(VALID_STATUSES.contains(&"pending"));
        assert!(!VALID_STATUSES.contains(&"invalid"));
    }

    #[test]
    fn test_convert_to_training_pairs_requires_ground_truth() {
        let args = TrainFromDiscrepanciesArgs {
            status: "confirmed_error".to_string(),
            dataset: None,
            output: None,
            dry_run: false,
            include_incomplete: false,
        };

        let discrepancies = vec![
            DiscrepancyExportRow {
                id: "1".to_string(),
                inference_id: "inf-1".to_string(),
                discrepancy_type: "incorrect_answer".to_string(),
                user_question: Some("What is 2+2?".to_string()),
                model_answer: Some("5".to_string()),
                ground_truth: Some("4".to_string()),
                document_id: None,
                chunk_hash_b3: None,
                confirmed_at: "2024-01-01".to_string(),
            },
            DiscrepancyExportRow {
                id: "2".to_string(),
                inference_id: "inf-2".to_string(),
                discrepancy_type: "hallucination".to_string(),
                user_question: Some("Who wrote Hamlet?".to_string()),
                model_answer: Some("George Washington".to_string()),
                ground_truth: None, // No ground truth
                document_id: None,
                chunk_hash_b3: None,
                confirmed_at: "2024-01-02".to_string(),
            },
        ];

        let pairs = args.convert_to_training_pairs(&discrepancies).unwrap();

        // Only the first one should be included (has ground_truth)
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].input, "What is 2+2?");
        assert_eq!(pairs[0].output, "4");
    }

    #[test]
    fn test_convert_to_training_pairs_include_incomplete() {
        let args = TrainFromDiscrepanciesArgs {
            status: "confirmed_error".to_string(),
            dataset: None,
            output: None,
            dry_run: false,
            include_incomplete: true, // Include cases without ground_truth
        };

        let discrepancies = vec![DiscrepancyExportRow {
            id: "1".to_string(),
            inference_id: "inf-1".to_string(),
            discrepancy_type: "hallucination".to_string(),
            user_question: Some("Who wrote Hamlet?".to_string()),
            model_answer: Some("Shakespeare".to_string()),
            ground_truth: None,
            document_id: None,
            chunk_hash_b3: None,
            confirmed_at: "2024-01-01".to_string(),
        }];

        let pairs = args.convert_to_training_pairs(&discrepancies).unwrap();

        // Should be included with model_answer as output
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].output, "Shakespeare");
    }

    #[test]
    fn test_provenance_serialization() {
        let prov = ProvenanceMetadata {
            schema: "supervised".to_string(),
            source: "discrepancy_case".to_string(),
            discrepancy_id: "disc-123".to_string(),
            inference_id: "inf-456".to_string(),
            discrepancy_type: "incorrect_answer".to_string(),
            document_id: Some("doc-789".to_string()),
            chunk_hash_b3: None,
        };

        let json = serde_json::to_string(&prov).unwrap();
        assert!(json.contains("\"schema\":\"supervised\""));
        assert!(json.contains("\"source\":\"discrepancy_case\""));
    }
}
