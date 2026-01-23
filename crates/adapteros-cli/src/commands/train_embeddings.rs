//! Train embedding model using contrastive learning
//!
//! Trains custom embedding models for RAG/semantic search using:
//! - Triplet loss: anchor, positive, negative
//! - InfoNCE loss: in-batch negatives
//! - Contrastive loss: pairs with similarity labels

use crate::commands::training_common::CommonTrainingArgs;
use adapteros_api_types::training::{
    EmbeddingExample, EmbeddingTrainingConfig, EmbeddingTrainingMode, PoolingStrategy,
};
use adapteros_core::{AosError, Result};
use adapteros_lora_worker::training::{
    batch_triplet_loss, info_nce_loss, l2_normalize, ProjectionLayer,
};
use clap::Args;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Train a custom embedding model using contrastive learning
#[derive(Args, Debug, Clone)]
pub struct TrainEmbeddingsArgs {
    /// Path to training data (JSONL with triplets or pairs)
    ///
    /// Each line should be one of:
    /// - Triplet: {"anchor": "text", "positive": "similar", "negative": "different"}
    /// - Pair: {"text_a": "text", "text_b": "other", "score": 0.8}
    #[arg(long)]
    data: PathBuf,

    /// Output directory for trained embedding model
    #[arg(long, default_value = "var/embeddings")]
    output: PathBuf,

    /// Embedding dimension (output size)
    ///
    /// Common values:
    /// - 384: SBERT-style small embeddings
    /// - 768: BERT-style standard embeddings
    /// - 1024: Large embeddings
    #[arg(long, default_value = "384")]
    dim: usize,

    /// Training mode: triplet, contrastive, or info-nce
    ///
    /// - triplet: Uses explicit positive/negative examples
    /// - contrastive: Uses pairs with similarity scores
    /// - info-nce: Uses in-batch negatives (efficient for large batches)
    #[arg(long, default_value = "info-nce")]
    mode: String,

    /// Pooling strategy: mean, cls, max, or last
    ///
    /// - mean: Average all token embeddings (recommended)
    /// - cls: Use [CLS] token only
    /// - max: Max pooling
    /// - last: Use last token
    #[arg(long, default_value = "mean")]
    pooling: String,

    /// Triplet margin (only for triplet mode)
    #[arg(long, default_value = "0.5")]
    margin: f32,

    /// Temperature for contrastive/info-nce loss
    #[arg(long, default_value = "0.07")]
    temperature: f32,

    /// L2 normalize output embeddings
    #[arg(long, default_value = "true")]
    normalize: bool,

    /// Maximum sequence length
    #[arg(long, default_value = "512")]
    max_seq_length: usize,

    /// Number of warmup steps
    #[arg(long, default_value = "100")]
    warmup_steps: u32,

    /// Dry run - show what would be done without executing
    #[arg(long)]
    dry_run: bool,

    /// Model name/identifier for the output
    #[arg(long, default_value = "custom-embeddings")]
    model_name: String,

    /// Common training hyperparameters
    #[command(flatten)]
    common: CommonTrainingArgs,
}

/// Loaded training data
#[derive(Debug)]
enum TrainingData {
    Triplets(Vec<(String, String, String)>),
    Pairs(Vec<(String, String, f32)>),
}

impl TrainingData {
    fn len(&self) -> usize {
        match self {
            TrainingData::Triplets(t) => t.len(),
            TrainingData::Pairs(p) => p.len(),
        }
    }
}

/// Result of embedding training
#[derive(Debug)]
struct TrainingResult {
    final_loss: f32,
    best_loss: f32,
    epochs_completed: usize,
    examples_processed: usize,
    training_time_secs: f64,
}

impl TrainEmbeddingsArgs {
    /// Execute the embedding training command
    pub async fn execute(&self) -> Result<()> {
        info!("Starting embedding model training");

        // Validate arguments
        self.common.validate()?;
        self.validate_embedding_args()?;

        // Parse mode and pooling
        let mode = self.parse_mode()?;
        let pooling = self.parse_pooling()?;

        // Load training data
        let data = self.load_training_data()?;

        // Build config
        let config = EmbeddingTrainingConfig {
            mode,
            embedding_dim: self.dim,
            pooling,
            normalize: self.normalize,
            learning_rate: self.common.learning_rate,
            batch_size: self.common.batch_size,
            epochs: self.common.epochs,
            warmup_steps: self.warmup_steps,
            max_seq_length: self.max_seq_length,
        };

        if self.dry_run {
            self.print_dry_run_summary(&config, &data)?;
            return Ok(());
        }

        // Create output directory
        fs::create_dir_all(&self.output)?;

        // Initialize projection layer
        info!(
            embedding_dim = self.dim,
            hidden_dim = self.common.hidden_dim,
            "Initializing projection layer"
        );

        let mut projection = ProjectionLayer::new(self.common.hidden_dim, self.dim, true);

        // Train based on data type
        let start_time = Instant::now();
        let result = match data {
            TrainingData::Triplets(triplets) => {
                info!(
                    num_triplets = triplets.len(),
                    epochs = self.common.epochs,
                    "Training with triplet data"
                );
                self.train_triplets(&mut projection, &triplets)?
            }
            TrainingData::Pairs(pairs) => {
                info!(
                    num_pairs = pairs.len(),
                    epochs = self.common.epochs,
                    "Training with pair data (using info-nce)"
                );
                self.train_pairs(&mut projection, &pairs)?
            }
        };

        let training_time = start_time.elapsed().as_secs_f64();

        // Save projection weights
        let model_path = self.output.join("projection.bin");
        self.save_projection(&projection, &model_path)?;

        // Save config
        let config_path = self.output.join("config.json");
        let config_json = serde_json::json!({
            "model_name": self.model_name,
            "embedding_dim": self.dim,
            "hidden_dim": self.common.hidden_dim,
            "pooling": self.pooling,
            "normalize": self.normalize,
            "training": {
                "mode": self.mode,
                "learning_rate": self.common.learning_rate,
                "batch_size": self.common.batch_size,
                "epochs": self.common.epochs,
                "final_loss": result.final_loss,
                "best_loss": result.best_loss,
                "training_time_secs": training_time,
                "examples_processed": result.examples_processed,
            }
        });
        fs::write(&config_path, serde_json::to_string_pretty(&config_json)?)?;

        info!(
            output_dir = %self.output.display(),
            final_loss = result.final_loss,
            training_time_secs = training_time,
            "Embedding model training complete"
        );

        println!("\nTraining complete!");
        println!("  Model saved to: {}", model_path.display());
        println!("  Config saved to: {}", config_path.display());
        println!("  Final loss: {:.6}", result.final_loss);
        println!("  Best loss: {:.6}", result.best_loss);
        println!("  Examples processed: {}", result.examples_processed);
        println!("  Training time: {:.2}s", training_time);

        Ok(())
    }

    fn validate_embedding_args(&self) -> Result<()> {
        if !self.data.exists() {
            return Err(AosError::Validation(format!(
                "Training data file not found: {}",
                self.data.display()
            )));
        }

        if self.dim == 0 {
            return Err(AosError::Validation(
                "Embedding dimension must be greater than zero".to_string(),
            ));
        }

        if self.dim > 4096 {
            return Err(AosError::Validation(
                "Embedding dimension too large (>4096)".to_string(),
            ));
        }

        if self.temperature <= 0.0 {
            return Err(AosError::Validation(
                "Temperature must be positive".to_string(),
            ));
        }

        if self.margin < 0.0 {
            return Err(AosError::Validation(
                "Triplet margin must be non-negative".to_string(),
            ));
        }

        Ok(())
    }

    fn parse_mode(&self) -> Result<EmbeddingTrainingMode> {
        match self.mode.to_lowercase().as_str() {
            "triplet" => Ok(EmbeddingTrainingMode::Triplet {
                margin: self.margin,
            }),
            "contrastive" => Ok(EmbeddingTrainingMode::Contrastive {
                temperature: self.temperature,
            }),
            "info-nce" | "infonce" | "nce" => Ok(EmbeddingTrainingMode::InfoNce {
                temperature: self.temperature,
            }),
            other => Err(AosError::Validation(format!(
                "Unknown training mode '{}'. Use: triplet, contrastive, or info-nce",
                other
            ))),
        }
    }

    fn parse_pooling(&self) -> Result<PoolingStrategy> {
        match self.pooling.to_lowercase().as_str() {
            "mean" => Ok(PoolingStrategy::Mean),
            "cls" => Ok(PoolingStrategy::Cls),
            "max" => Ok(PoolingStrategy::Max),
            "last" => Ok(PoolingStrategy::Last),
            other => Err(AosError::Validation(format!(
                "Unknown pooling strategy '{}'. Use: mean, cls, max, or last",
                other
            ))),
        }
    }

    fn load_training_data(&self) -> Result<TrainingData> {
        let content = fs::read_to_string(&self.data).map_err(|e| {
            AosError::Io(format!(
                "Failed to read training data from {}: {}",
                self.data.display(),
                e
            ))
        })?;

        let mut triplets = Vec::new();
        let mut pairs = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Try parsing as EmbeddingExample
            match serde_json::from_str::<EmbeddingExample>(line) {
                Ok(EmbeddingExample::Triplet {
                    anchor,
                    positive,
                    negative,
                }) => {
                    triplets.push((anchor, positive, negative));
                }
                Ok(EmbeddingExample::Pair {
                    text_a,
                    text_b,
                    score,
                }) => {
                    pairs.push((text_a, text_b, score));
                }
                Err(e) => {
                    // Try parsing as raw JSON object
                    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
                        if let (Some(anchor), Some(positive), Some(negative)) = (
                            obj.get("anchor").and_then(|v| v.as_str()),
                            obj.get("positive").and_then(|v| v.as_str()),
                            obj.get("negative").and_then(|v| v.as_str()),
                        ) {
                            triplets.push((
                                anchor.to_string(),
                                positive.to_string(),
                                negative.to_string(),
                            ));
                            continue;
                        }

                        if let (Some(text_a), Some(text_b)) = (
                            obj.get("text_a").and_then(|v| v.as_str()),
                            obj.get("text_b").and_then(|v| v.as_str()),
                        ) {
                            let score =
                                obj.get("score").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
                            pairs.push((text_a.to_string(), text_b.to_string(), score));
                            continue;
                        }
                    }

                    warn!(
                        line = line_num + 1,
                        error = %e,
                        "Skipping invalid line in training data"
                    );
                }
            }
        }

        // Determine which format to use
        if !triplets.is_empty() && !pairs.is_empty() {
            warn!(
                "Mixed triplet and pair data found. Using {} triplets (ignoring {} pairs)",
                triplets.len(),
                pairs.len()
            );
            Ok(TrainingData::Triplets(triplets))
        } else if !triplets.is_empty() {
            info!(
                num_examples = triplets.len(),
                "Loaded triplet training data"
            );
            Ok(TrainingData::Triplets(triplets))
        } else if !pairs.is_empty() {
            info!(num_examples = pairs.len(), "Loaded pair training data");
            Ok(TrainingData::Pairs(pairs))
        } else {
            Err(AosError::Validation(
                "No valid training examples found in data file".to_string(),
            ))
        }
    }

    fn train_triplets(
        &self,
        projection: &mut ProjectionLayer,
        triplets: &[(String, String, String)],
    ) -> Result<TrainingResult> {
        let margin = self.margin;
        let num_epochs = self.common.epochs;
        let batch_size = self.common.batch_size;

        let mut best_loss = f32::MAX;
        let mut total_loss = 0.0;
        let mut examples_processed = 0;

        for epoch in 0..num_epochs {
            let mut epoch_loss = 0.0;
            let mut batches = 0;

            for chunk in triplets.chunks(batch_size) {
                // Convert text to embeddings using simple hash encoding
                let anchors: Vec<Vec<f32>> = chunk
                    .iter()
                    .map(|(a, _, _)| self.simple_encode(a, projection.hidden_dim))
                    .collect();
                let positives: Vec<Vec<f32>> = chunk
                    .iter()
                    .map(|(_, p, _)| self.simple_encode(p, projection.hidden_dim))
                    .collect();
                let negatives: Vec<Vec<f32>> = chunk
                    .iter()
                    .map(|(_, _, n)| self.simple_encode(n, projection.hidden_dim))
                    .collect();

                // Project to embedding space
                let anchor_embs: Vec<Vec<f32>> = anchors
                    .iter()
                    .map(|a| {
                        let mut emb = projection.forward(a);
                        if self.normalize {
                            l2_normalize(&mut emb);
                        }
                        emb
                    })
                    .collect();
                let positive_embs: Vec<Vec<f32>> = positives
                    .iter()
                    .map(|p| {
                        let mut emb = projection.forward(p);
                        if self.normalize {
                            l2_normalize(&mut emb);
                        }
                        emb
                    })
                    .collect();
                let negative_embs: Vec<Vec<f32>> = negatives
                    .iter()
                    .map(|n| {
                        let mut emb = projection.forward(n);
                        if self.normalize {
                            l2_normalize(&mut emb);
                        }
                        emb
                    })
                    .collect();

                // Compute batch loss
                let batch_loss =
                    batch_triplet_loss(&anchor_embs, &positive_embs, &negative_embs, margin);
                epoch_loss += batch_loss;
                batches += 1;
                examples_processed += chunk.len();

                // Update projection (simplified gradient update)
                if batch_loss > 0.0 {
                    let grad_scale = self.common.learning_rate / chunk.len() as f32;
                    for (i, anchor) in anchors.iter().enumerate() {
                        let (wg, bg) = projection.backward(anchor, &anchor_embs[i]);
                        projection.update(&wg, bg.as_deref(), grad_scale * batch_loss);
                    }
                }
            }

            let avg_loss = if batches > 0 {
                epoch_loss / batches as f32
            } else {
                0.0
            };
            total_loss = avg_loss;

            if avg_loss < best_loss {
                best_loss = avg_loss;
            }

            info!(
                epoch = epoch + 1,
                loss = avg_loss,
                best = best_loss,
                "Epoch complete"
            );
        }

        Ok(TrainingResult {
            final_loss: total_loss,
            best_loss,
            epochs_completed: num_epochs,
            examples_processed,
            training_time_secs: 0.0, // Will be filled in by caller
        })
    }

    fn train_pairs(
        &self,
        projection: &mut ProjectionLayer,
        pairs: &[(String, String, f32)],
    ) -> Result<TrainingResult> {
        let temperature = self.temperature;
        let num_epochs = self.common.epochs;
        let batch_size = self.common.batch_size;

        let mut best_loss = f32::MAX;
        let mut total_loss = 0.0;
        let mut examples_processed = 0;

        for epoch in 0..num_epochs {
            let mut epoch_loss = 0.0;
            let mut batches = 0;

            for chunk in pairs.chunks(batch_size) {
                // Convert text to embeddings
                let queries: Vec<Vec<f32>> = chunk
                    .iter()
                    .map(|(a, _, _)| self.simple_encode(a, projection.hidden_dim))
                    .collect();
                let positives: Vec<Vec<f32>> = chunk
                    .iter()
                    .map(|(_, b, _)| self.simple_encode(b, projection.hidden_dim))
                    .collect();

                // Project to embedding space
                let query_embs: Vec<Vec<f32>> = queries
                    .iter()
                    .map(|q| {
                        let mut emb = projection.forward(q);
                        if self.normalize {
                            l2_normalize(&mut emb);
                        }
                        emb
                    })
                    .collect();
                let positive_embs: Vec<Vec<f32>> = positives
                    .iter()
                    .map(|p| {
                        let mut emb = projection.forward(p);
                        if self.normalize {
                            l2_normalize(&mut emb);
                        }
                        emb
                    })
                    .collect();

                // Compute InfoNCE loss
                let batch_loss = info_nce_loss(&query_embs, &positive_embs, temperature);
                epoch_loss += batch_loss;
                batches += 1;
                examples_processed += chunk.len();

                // Update projection
                let grad_scale = self.common.learning_rate / chunk.len() as f32;
                for (i, query) in queries.iter().enumerate() {
                    let (wg, bg) = projection.backward(query, &query_embs[i]);
                    projection.update(&wg, bg.as_deref(), grad_scale * batch_loss);
                }
            }

            let avg_loss = if batches > 0 {
                epoch_loss / batches as f32
            } else {
                0.0
            };
            total_loss = avg_loss;

            if avg_loss < best_loss {
                best_loss = avg_loss;
            }

            info!(
                epoch = epoch + 1,
                loss = avg_loss,
                best = best_loss,
                "Epoch complete"
            );
        }

        Ok(TrainingResult {
            final_loss: total_loss,
            best_loss,
            epochs_completed: num_epochs,
            examples_processed,
            training_time_secs: 0.0,
        })
    }

    /// Simple text encoding (placeholder - in production would use tokenizer)
    fn simple_encode(&self, text: &str, hidden_dim: usize) -> Vec<f32> {
        // Hash-based encoding for consistent dimensions
        let mut embedding = vec![0.0f32; hidden_dim];

        for (i, c) in text.chars().enumerate() {
            let idx = (c as usize + i * 31) % hidden_dim;
            embedding[idx] += 1.0;
        }

        // Normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 1e-8 {
            for x in &mut embedding {
                *x /= norm;
            }
        }

        embedding
    }

    fn save_projection(&self, projection: &ProjectionLayer, path: &std::path::Path) -> Result<()> {
        // Convert weights to binary
        let weight_data: Vec<u8> = projection
            .weights
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        fs::write(path, &weight_data)
            .map_err(|e| AosError::Io(format!("Failed to save projection weights: {}", e)))?;

        // Save bias if present
        if let Some(ref bias) = projection.bias {
            let bias_path = path.with_extension("bias.bin");
            let bias_data: Vec<u8> = bias.iter().flat_map(|f| f.to_le_bytes()).collect();
            fs::write(&bias_path, &bias_data)
                .map_err(|e| AosError::Io(format!("Failed to save projection bias: {}", e)))?;
        }

        Ok(())
    }

    fn print_dry_run_summary(
        &self,
        _config: &EmbeddingTrainingConfig,
        data: &TrainingData,
    ) -> Result<()> {
        println!("\n=== Embedding Training (Dry Run) ===\n");

        println!("Input:");
        println!("  Data file: {}", self.data.display());
        match data {
            TrainingData::Triplets(t) => println!("  Examples: {} triplets", t.len()),
            TrainingData::Pairs(p) => println!("  Examples: {} pairs", p.len()),
        }

        println!("\nModel Configuration:");
        println!("  Embedding dimension: {}", self.dim);
        println!("  Hidden dimension: {}", self.common.hidden_dim);
        println!("  Pooling: {}", self.pooling);
        println!("  Normalize: {}", self.normalize);

        println!("\nTraining Configuration:");
        println!("  Mode: {}", self.mode);
        println!("  Learning rate: {}", self.common.learning_rate);
        println!("  Batch size: {}", self.common.batch_size);
        println!("  Epochs: {}", self.common.epochs);
        println!("  Warmup steps: {}", self.warmup_steps);
        println!("  Max sequence length: {}", self.max_seq_length);

        if self.mode == "triplet" {
            println!("  Triplet margin: {}", self.margin);
        } else {
            println!("  Temperature: {}", self.temperature);
        }

        println!("\nOutput:");
        println!("  Directory: {}", self.output.display());
        println!("  Model: {}/projection.bin", self.output.display());
        println!("  Config: {}/config.json", self.output.display());

        println!("\nTo execute training, remove the --dry-run flag.");

        Ok(())
    }
}

/// Handle the train-embeddings command
pub async fn run(args: TrainEmbeddingsArgs) -> Result<()> {
    args.execute().await
}
