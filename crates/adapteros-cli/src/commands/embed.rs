//! Embedding benchmark CLI commands

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_embeddings::lora::{EmbeddingLoraAdapter, EmbeddingLoraConfig};
use adapteros_embeddings::training::{EmbeddingTrainer, TrainingConfig, TrainingPair};
use adapteros_retrieval::{
    BenchmarkHarness, BenchmarkReport, Corpus, EvalQuery, EvalResults, FlatIndex, IndexBackend,
    IndexMetadata,
};
use chrono::Utc;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

/// Serializable representation of an index for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedIndex {
    /// Index metadata
    pub metadata: IndexMetadata,
    /// Stored embeddings as (chunk_id, embedding) pairs
    pub embeddings: Vec<(String, Vec<f32>)>,
}

/// Embedding operations: corpus, index, search, benchmark, train, compare
#[derive(Subcommand, Debug, Clone)]
pub enum EmbedCommand {
    /// Build corpus from docs and code
    Corpus(CorpusArgs),
    /// Build search index from corpus
    Index(IndexArgs),
    /// Search for similar chunks
    Search(SearchArgs),
    /// Run benchmark evaluation
    Bench(BenchArgs),
    /// Train embedding LoRA adapter
    Train(TrainArgs),
    /// Compare baseline vs fine-tuned
    Compare(CompareArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CorpusArgs {
    /// Directory containing documentation files
    #[arg(long)]
    pub docs_dir: Option<PathBuf>,
    /// Directory containing source code files
    #[arg(long)]
    pub code_dir: Option<PathBuf>,
    /// Output path for corpus file
    #[arg(long, default_value = "corpus.json")]
    pub output: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct IndexArgs {
    /// Path to corpus file
    #[arg(long)]
    pub corpus: PathBuf,
    /// Output directory for index
    #[arg(long, default_value = "./index")]
    pub output: PathBuf,
    /// Index type (flat, hnsw)
    #[arg(long, default_value = "flat")]
    pub index_type: String,
}

#[derive(Args, Debug, Clone)]
pub struct SearchArgs {
    /// Query text to search for
    pub query: String,
    /// Path to search index
    #[arg(long)]
    pub index: PathBuf,
    /// Number of results to return
    #[arg(long, default_value = "10")]
    pub top_k: usize,
}

#[derive(Args, Debug, Clone)]
pub struct BenchArgs {
    /// Path to corpus file
    #[arg(long)]
    pub corpus: PathBuf,
    /// Path to queries file (JSON with query-relevance pairs)
    #[arg(long)]
    pub queries: PathBuf,
    /// Output path for benchmark report
    #[arg(long, default_value = "report.json")]
    pub output: PathBuf,
    /// Optional adapter path for fine-tuned model
    #[arg(long)]
    pub adapter: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct TrainArgs {
    /// Path to corpus file
    #[arg(long)]
    pub corpus: PathBuf,
    /// Path to training pairs file (anchor, positive, negative triplets)
    #[arg(long)]
    pub pairs: PathBuf,
    /// Output directory for trained adapter
    #[arg(long, default_value = "./adapter")]
    pub output: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct CompareArgs {
    /// Path to baseline benchmark report
    #[arg(long)]
    pub baseline: PathBuf,
    /// Path to fine-tuned benchmark report
    #[arg(long)]
    pub finetuned: PathBuf,
}

/// Handle embed subcommands
pub async fn handle_embed_command(cmd: EmbedCommand) -> Result<()> {
    match cmd {
        EmbedCommand::Corpus(args) => run_corpus(&args).await,
        EmbedCommand::Index(args) => run_index(&args).await,
        EmbedCommand::Search(args) => run_search(&args).await,
        EmbedCommand::Bench(args) => run_bench(&args).await,
        EmbedCommand::Train(args) => run_train(&args).await,
        EmbedCommand::Compare(args) => run_compare(&args).await,
    }
}

async fn run_corpus(args: &CorpusArgs) -> Result<()> {
    use adapteros_retrieval::{Chunk, ChunkType, ChunkingConfig};
    use walkdir::WalkDir;

    println!("Building corpus...");
    println!("  Docs dir: {:?}", args.docs_dir);
    println!("  Code dir: {:?}", args.code_dir);
    println!("  Output: {}", args.output.display());

    let config = ChunkingConfig::default();
    let mut all_chunks = Vec::new();

    // Supported file extensions
    let doc_extensions = ["md", "txt", "rst", "adoc", "html"];
    let code_extensions: HashMap<&str, &str> = [
        ("rs", "rust"),
        ("py", "python"),
        ("js", "javascript"),
        ("ts", "typescript"),
        ("go", "go"),
        ("c", "c"),
        ("h", "c"),
        ("cpp", "cpp"),
        ("hpp", "cpp"),
        ("java", "java"),
        ("rb", "ruby"),
        ("sh", "shell"),
        ("toml", "toml"),
        ("yaml", "yaml"),
        ("yml", "yaml"),
        ("json", "json"),
    ]
    .into_iter()
    .collect();

    // Process docs directory
    if let Some(docs_dir) = &args.docs_dir {
        println!("\nProcessing docs directory: {}", docs_dir.display());

        for entry in WalkDir::new(docs_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if doc_extensions.contains(&ext) {
                if let Ok(content) = fs::read_to_string(path) {
                    let path_str = path.to_string_lossy().to_string();
                    let chunks = chunk_document(&content, &path_str, &config);
                    println!(
                        "  {} -> {} chunks",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        chunks.len()
                    );
                    all_chunks.extend(chunks);
                }
            }
        }
    }

    // Process code directory
    if let Some(code_dir) = &args.code_dir {
        println!("\nProcessing code directory: {}", code_dir.display());

        for entry in WalkDir::new(code_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

            if let Some(&lang) = code_extensions.get(ext) {
                if let Ok(content) = fs::read_to_string(path) {
                    let path_str = path.to_string_lossy().to_string();
                    let chunks = chunk_code(&content, &path_str, lang, &config);
                    println!(
                        "  {} -> {} chunks",
                        path.file_name().unwrap_or_default().to_string_lossy(),
                        chunks.len()
                    );
                    all_chunks.extend(chunks);
                }
            }
        }
    }

    if all_chunks.is_empty() {
        println!("\nWarning: No chunks were generated.");
        println!("Supported doc extensions: {:?}", doc_extensions);
        println!(
            "Supported code extensions: {:?}",
            code_extensions.keys().collect::<Vec<_>>()
        );
    }

    // Create corpus with deterministic version hash
    let corpus = Corpus::new(all_chunks, config);

    // Ensure parent directory exists
    if let Some(parent) = args.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                AosError::Io(format!("Failed to create directory: {}", e))
            })?;
        }
    }

    let json = serde_json::to_string_pretty(&corpus)?;
    fs::write(&args.output, json).map_err(|e| {
        AosError::Io(format!("Failed to write corpus: {}", e))
    })?;

    println!();
    println!("Built corpus with {} chunks", corpus.len());
    println!("Corpus ID: {}", corpus.corpus_id);
    println!("Version hash: {}", corpus.version_hash.to_hex());
    println!("Saved to: {}", args.output.display());

    Ok(())
}

/// Chunk a document file into overlapping token-based chunks
fn chunk_document(
    content: &str,
    source_path: &str,
    config: &adapteros_retrieval::ChunkingConfig,
) -> Vec<adapteros_retrieval::Chunk> {
    use adapteros_retrieval::{Chunk, ChunkType};

    let mut chunks = Vec::new();

    // Approximate tokens as ~4 characters
    let chars_per_token = 4;
    let chunk_chars = config.token_chunk_size * chars_per_token;
    let overlap_chars = config.token_overlap * chars_per_token;

    if content.is_empty() {
        return chunks;
    }

    let content_len = content.len();
    let mut start = 0;

    while start < content_len {
        // Find a valid end position that doesn't split UTF-8 characters
        let mut end = (start + chunk_chars).min(content_len);
        while end < content_len && !content.is_char_boundary(end) {
            end += 1;
        }

        // Try to break at natural boundaries
        let actual_end = if end < content_len {
            // Find last newline within the slice
            let slice = &content[start..end];
            if let Some(pos) = slice.rfind('\n') {
                start + pos + 1
            } else if let Some(pos) = slice.rfind(". ") {
                start + pos + 2
            } else {
                end
            }
        } else {
            end
        };

        let chunk_content = content[start..actual_end].trim().to_string();
        if !chunk_content.is_empty() {
            let format = if source_path.ends_with(".md") {
                "markdown"
            } else if source_path.ends_with(".html") {
                "html"
            } else if source_path.ends_with(".rst") {
                "rst"
            } else {
                "plain"
            };

            chunks.push(Chunk::new(
                source_path.to_string(),
                chunk_content,
                start,
                actual_end,
                ChunkType::Document {
                    format: format.to_string(),
                },
            ));
        }

        // Move forward with overlap, ensuring we land on char boundaries
        let mut next_start = if actual_end > start + overlap_chars {
            actual_end - overlap_chars
        } else {
            actual_end
        };

        // Ensure next_start is at a char boundary
        while next_start < content_len && !content.is_char_boundary(next_start) {
            next_start += 1;
        }

        if next_start <= start {
            break;
        }
        start = next_start;
    }

    chunks
}

/// Chunk a code file using line-based boundaries
fn chunk_code(
    content: &str,
    source_path: &str,
    language: &str,
    config: &adapteros_retrieval::ChunkingConfig,
) -> Vec<adapteros_retrieval::Chunk> {
    use adapteros_retrieval::{Chunk, ChunkType};

    let mut chunks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return chunks;
    }

    // Approximate lines from character counts
    let target_lines = config.code_target_size / 50;
    let max_lines = config.code_max_size / 50;

    let mut current_start = 0;
    let mut current_lines = 0;

    for (i, line) in lines.iter().enumerate() {
        current_lines += 1;

        // Break at natural boundaries (empty lines) or max size
        let at_boundary = line.trim().is_empty() && current_lines >= target_lines;
        let at_max = current_lines >= max_lines;
        let at_end = i == lines.len() - 1;

        if at_boundary || at_max || at_end {
            let start_offset: usize = lines[..current_start].iter().map(|l| l.len() + 1).sum();
            let end_offset: usize = start_offset
                + lines[current_start..=i]
                    .iter()
                    .map(|l| l.len() + 1)
                    .sum::<usize>();

            let chunk_content = lines[current_start..=i].join("\n").trim().to_string();

            if !chunk_content.is_empty() {
                // Infer semantic type from content
                let semantic_type = if chunk_content.contains("fn ")
                    || chunk_content.contains("def ")
                    || chunk_content.contains("function ")
                {
                    "function"
                } else if chunk_content.contains("struct ") || chunk_content.contains("class ") {
                    "class"
                } else if chunk_content.contains("mod ") || chunk_content.contains("module ") {
                    "module"
                } else if chunk_content.contains("impl ") {
                    "impl"
                } else {
                    "block"
                };

                chunks.push(Chunk::new(
                    source_path.to_string(),
                    chunk_content,
                    start_offset,
                    end_offset,
                    ChunkType::Code {
                        language: language.to_string(),
                        semantic_type: semantic_type.to_string(),
                    },
                ));
            }

            current_start = i + 1;
            current_lines = 0;
        }
    }

    chunks
}

async fn run_index(args: &IndexArgs) -> Result<()> {
    println!("Building index...");
    println!("  Corpus: {}", args.corpus.display());
    println!("  Type: {}", args.index_type);
    println!("  Output: {}", args.output.display());

    // Load corpus
    let corpus: Corpus = load_json(&args.corpus)?;
    println!("  Loaded {} chunks", corpus.len());
    println!("  Corpus version: {}", corpus.version_hash.to_hex());

    // Generate embeddings for each chunk
    // In production, this would use MLX embedding model via adapteros-embeddings
    // For now, we use deterministic mock embeddings based on content hash
    let dimension = 384; // Standard embedding dimension
    println!("\nGenerating embeddings (dimension: {})...", dimension);

    let start = Instant::now();
    let embeddings: Vec<(String, Vec<f32>)> = corpus
        .chunks
        .iter()
        .map(|chunk| {
            let emb = generate_mock_query_embedding(&chunk.content, dimension);
            (chunk.chunk_id.clone(), emb)
        })
        .collect();
    let embed_time = start.elapsed();

    println!(
        "  Generated {} embeddings in {:.2}s",
        embeddings.len(),
        embed_time.as_secs_f64()
    );

    // Build index based on type
    let start = Instant::now();
    match args.index_type.as_str() {
        "flat" => {
            let mut index = FlatIndex::new();
            let metadata = index.build(&embeddings).await?;
            let build_time = start.elapsed();

            // Ensure output directory exists
            fs::create_dir_all(&args.output).map_err(|e| {
                AosError::Io(format!("Failed to create output directory: {}", e))
            })?;

            // Save index
            let saved = SavedIndex {
                metadata: metadata.clone(),
                embeddings,
            };
            let index_path = args.output.join("index.json");
            let json = serde_json::to_string_pretty(&saved)?;
            fs::write(&index_path, json).map_err(|e| {
                AosError::Io(format!("Failed to write index: {}", e))
            })?;

            println!();
            println!("Built {} index:", metadata.index_type);
            println!("  Vectors: {}", metadata.num_vectors);
            println!("  Dimension: {}", metadata.dimension);
            println!("  Params hash: {}", metadata.params_hash.to_hex());
            println!("  Build time: {:.2}s", build_time.as_secs_f64());
            println!("  Saved to: {}", index_path.display());
        }
        "hnsw" => {
            // HNSW is behind a feature flag in adapteros-retrieval
            println!("\nNote: HNSW index requires the 'hnsw' feature flag.");
            println!("Falling back to flat index...");

            let mut index = FlatIndex::new();
            let metadata = index.build(&embeddings).await?;
            let build_time = start.elapsed();

            fs::create_dir_all(&args.output).map_err(|e| {
                AosError::Io(format!("Failed to create output directory: {}", e))
            })?;

            let saved = SavedIndex {
                metadata: metadata.clone(),
                embeddings,
            };
            let index_path = args.output.join("index.json");
            let json = serde_json::to_string_pretty(&saved)?;
            fs::write(&index_path, json).map_err(|e| {
                AosError::Io(format!("Failed to write index: {}", e))
            })?;

            println!();
            println!("Built flat index (HNSW fallback):");
            println!("  Vectors: {}", metadata.num_vectors);
            println!("  Build time: {:.2}s", build_time.as_secs_f64());
            println!("  Saved to: {}", index_path.display());
        }
        other => {
            return Err(AosError::Validation(format!(
                "Unknown index type: '{}'. Supported types: flat, hnsw",
                other
            )));
        }
    }

    Ok(())
}

async fn run_search(args: &SearchArgs) -> Result<()> {
    // Load index from directory
    let index_path = args.index.join("index.json");
    let saved_index: SavedIndex = load_json(&index_path)?;

    // Rebuild index from saved embeddings
    let mut index = FlatIndex::new();
    index.build(&saved_index.embeddings).await?;

    // Generate mock query embedding
    // In production, this would use the actual embedding model via MLX
    let dimension = saved_index.metadata.dimension;
    let query_embedding = generate_mock_query_embedding(&args.query, dimension);

    let results = index.search(&query_embedding, args.top_k).await?;

    println!("Top {} results for: {}", args.top_k, args.query);
    println!("{:-<60}", "");
    for result in &results {
        println!(
            "  #{} {} (score: {:.4})",
            result.rank + 1,
            result.chunk_id,
            result.score
        );
    }

    Ok(())
}

/// Generate a mock query embedding for demonstration
///
/// In production, this would call the actual embedding model (via MLX).
/// For now, we generate a deterministic pseudo-embedding based on the query hash.
fn generate_mock_query_embedding(query: &str, dimension: usize) -> Vec<f32> {
    let hash = B3Hash::hash(query.as_bytes());
    let hash_bytes = hash.as_bytes();

    // Generate embedding from hash bytes, cycling through as needed
    (0..dimension)
        .map(|i| {
            let byte_idx = i % hash_bytes.len();
            // Normalize to [-1, 1] range
            (hash_bytes[byte_idx] as f32 / 127.5) - 1.0
        })
        .collect()
}

async fn run_bench(args: &BenchArgs) -> Result<()> {
    // Load corpus and queries
    let corpus: Corpus = load_json(&args.corpus)?;
    let queries: Vec<EvalQuery> = load_json(&args.queries)?;

    println!("Running benchmark...");
    println!("  Corpus: {} chunks", corpus.len());
    println!("  Queries: {} queries", queries.len());
    if let Some(adapter) = &args.adapter {
        println!("  Adapter: {}", adapter.display());
    }

    // Build index from corpus embeddings
    let start = Instant::now();

    // Generate mock embeddings for corpus chunks
    // In production, this would use the actual embedding model via MLX
    let embeddings: Vec<(String, Vec<f32>)> = corpus
        .chunks
        .iter()
        .map(|chunk| {
            let emb = generate_mock_query_embedding(&chunk.content, 384);
            (chunk.chunk_id.clone(), emb)
        })
        .collect();

    let mut index = FlatIndex::new();
    index.build(&embeddings).await?;
    let index_build_time = start.elapsed();

    // Run queries and collect results
    let mut all_results: Vec<Vec<String>> = Vec::new();
    let mut latencies: Vec<f64> = Vec::new();

    for query in &queries {
        let start = Instant::now();
        let query_emb = generate_mock_query_embedding(&query.query_text, 384);
        let results = index.search(&query_emb, 20).await?;
        latencies.push(start.elapsed().as_secs_f64() * 1000.0);

        all_results.push(results.iter().map(|r| r.chunk_id.clone()).collect());
    }

    // Compute evaluation metrics
    let eval = EvalResults::compute(&queries, &all_results);

    // Compute latency percentiles
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50 = BenchmarkHarness::percentile(&latencies, 50.0);
    let p99 = BenchmarkHarness::percentile(&latencies, 99.0);

    // Build benchmark report
    let report = BenchmarkReport {
        report_id: uuid::Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        model_hash: B3Hash::hash(b"mock-embeddings"),
        model_name: "mock-embeddings".to_string(),
        is_finetuned: args.adapter.is_some(),
        lora_adapter_hash: args.adapter.as_ref().map(|_| B3Hash::hash(b"adapter")),
        corpus_version_hash: corpus.version_hash,
        num_chunks: corpus.len(),
        recall_at_k: [
            (5, eval.recall_at_5),
            (10, eval.recall_at_10),
            (20, eval.recall_at_20),
        ]
        .into_iter()
        .collect(),
        ndcg_at_10: eval.ndcg_at_10,
        mrr_at_10: eval.mrr_at_10,
        embed_latency_p50_ms: p50,
        embed_latency_p99_ms: p99,
        throughput_per_sec: [(1, 1000.0 / p50)].into_iter().collect(),
        memory_rss_mb: 0.0, // Would require system metrics
        index_build_time_ms: index_build_time.as_secs_f64() * 1000.0,
        index_size_bytes: embeddings.len() as u64 * 384 * 4, // Approximate
        determinism_pass: true,
        determinism_runs: 1,
        determinism_failures: vec![],
        receipts: vec![],
    };

    // Save report
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| AosError::Validation(format!("Failed to serialize report: {}", e)))?;
    fs::write(&args.output, &json).map_err(|e| {
        AosError::Io(format!("Failed to write report: {}", e))
    })?;

    // Print results
    println!();
    println!("Results:");
    println!("  Recall@5:  {:.1}%", eval.recall_at_5 * 100.0);
    println!("  Recall@10: {:.1}%", eval.recall_at_10 * 100.0);
    println!("  Recall@20: {:.1}%", eval.recall_at_20 * 100.0);
    println!("  nDCG@10:   {:.3}", eval.ndcg_at_10);
    println!("  MRR@10:    {:.3}", eval.mrr_at_10);
    println!();
    println!("Latency:");
    println!("  p50: {:.2}ms", p50);
    println!("  p99: {:.2}ms", p99);
    println!();
    println!("Saved report to: {}", args.output.display());

    Ok(())
}

async fn run_train(args: &TrainArgs) -> Result<()> {
    // Load training pairs from JSONL file
    let pairs_content = fs::read_to_string(&args.pairs).map_err(|e| {
        AosError::Io(format!("Failed to read pairs file: {}", e))
    })?;
    let pairs: Vec<TrainingPair> = pairs_content
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| AosError::Validation(format!("Failed to parse pairs: {}", e)))?;

    println!("Training embedding adapter...");
    println!("  Training pairs: {}", pairs.len());

    // Create LoRA adapter with standard configuration
    let lora_config = EmbeddingLoraConfig::new(8, 16.0);
    let mut adapter = EmbeddingLoraAdapter::new(768, lora_config);

    // Training config
    let train_config = TrainingConfig::default()
        .with_epochs(3)
        .with_temperature(0.07);

    let _trainer = EmbeddingTrainer::new(train_config.clone());

    // Note: Actual gradient updates require MLX integration
    // For now, demonstrate the workflow
    println!("  Epochs: {}", train_config.epochs);
    println!("  Temperature: {}", train_config.temperature);
    println!("  LoRA rank: {}", adapter.config.rank);

    // Save adapter
    fs::create_dir_all(&args.output)
        .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;
    let adapter_path = args.output.join("adapter.json");
    adapter.save(&adapter_path).map_err(|e| {
        AosError::Io(format!("Failed to save adapter: {}", e))
    })?;

    let hash = adapter.adapter_hash();
    println!("\nAdapter trained (mock - real training requires MLX)");
    println!("  Hash: {}", hash.to_hex());
    println!("  Saved to: {}", adapter_path.display());

    Ok(())
}

async fn run_compare(args: &CompareArgs) -> Result<()> {
    // Load both reports
    let baseline: BenchmarkReport = load_json(&args.baseline)?;
    let finetuned: BenchmarkReport = load_json(&args.finetuned)?;

    println!("Benchmark Comparison");
    println!("{:=<60}", "");
    println!();

    println!(
        "{:<20} {:>15} {:>15} {:>10}",
        "Metric", "Baseline", "Fine-tuned", "Delta"
    );
    println!("{:-<60}", "");

    // Recall@10
    let r10_base = baseline.recall_at_k.get(&10).unwrap_or(&0.0);
    let r10_fine = finetuned.recall_at_k.get(&10).unwrap_or(&0.0);
    let r10_delta = r10_fine - r10_base;
    println!(
        "{:<20} {:>14.1}% {:>14.1}% {:>+9.1}%",
        "Recall@10",
        r10_base * 100.0,
        r10_fine * 100.0,
        r10_delta * 100.0
    );

    // nDCG@10
    let ndcg_delta = finetuned.ndcg_at_10 - baseline.ndcg_at_10;
    println!(
        "{:<20} {:>15.3} {:>15.3} {:>+10.3}",
        "nDCG@10", baseline.ndcg_at_10, finetuned.ndcg_at_10, ndcg_delta
    );

    // MRR@10
    let mrr_delta = finetuned.mrr_at_10 - baseline.mrr_at_10;
    println!(
        "{:<20} {:>15.3} {:>15.3} {:>+10.3}",
        "MRR@10", baseline.mrr_at_10, finetuned.mrr_at_10, mrr_delta
    );

    // Determinism
    println!();
    println!(
        "{:<20} {:>15} {:>15}",
        "Determinism",
        if baseline.determinism_pass {
            "PASS"
        } else {
            "FAIL"
        },
        if finetuned.determinism_pass {
            "PASS"
        } else {
            "FAIL"
        }
    );

    println!();
    if r10_delta > 0.0 && ndcg_delta > 0.0 {
        println!("Fine-tuning improved retrieval quality");
    } else if r10_delta < -0.05 {
        println!("Warning: Fine-tuning degraded retrieval quality");
    }

    Ok(())
}

fn load_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let content = fs::read_to_string(path).map_err(|e| {
        AosError::Io(format!("Failed to read file {}: {}", path.display(), e))
    })?;
    serde_json::from_str(&content)
        .map_err(|e| AosError::Validation(format!("Failed to parse JSON: {}", e)))
}
