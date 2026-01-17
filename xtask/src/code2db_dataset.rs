//! Build JSON training dataset for code→DB tasks

use adapteros_types::training::{provenance_from_map, ExampleMetadataV1};
use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Parser, Clone)]
pub struct Code2DbDatasetArgs {
    /// Repo root (defaults to current workspace root)
    #[arg(long, default_value = ".")]
    pub repo_root: PathBuf,

    /// Paths to include (comma-separated); defaults to crates/adapteros-db and migrations
    #[arg(long)]
    pub include: Option<String>,

    /// Output dataset JSON path
    #[arg(long, default_value = "data/code_to_db_training.json")]
    pub output: PathBuf,

    /// Qwen tokenizer path (auto-discovered from AOS_TOKENIZER_PATH or model directory)
    #[arg(long, env = "AOS_TOKENIZER_PATH")]
    pub tokenizer: Option<PathBuf>,

    /// Context window for chunk pairs
    #[arg(long, default_value_t = 512)]
    pub context_window: usize,

    /// Minimum examples required
    #[arg(long, default_value_t = 10)]
    pub min_examples: usize,
}

#[derive(Debug, Serialize)]
struct TrainingExampleOut {
    input: Vec<u32>,
    target: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Serialize)]
struct TrainingDataOut {
    examples: Vec<TrainingExampleOut>,
}

/// Build dataset by pairing prompts→targets for adapteros-db patterns.
pub async fn run(args: Code2DbDatasetArgs) -> Result<()> {
    // Resolve include paths
    let include_paths: Vec<PathBuf> = if let Some(inc) = &args.include {
        inc.split(',')
            .map(|s| args.repo_root.join(s.trim()))
            .collect()
    } else {
        vec![
            args.repo_root.join("crates/adapteros-db"),
            args.repo_root.join("migrations"),
        ]
    };

    // Collect candidate files (Rust and SQL) for simple pairing heuristics
    let mut sources = Vec::new();
    for base in include_paths {
        if base.is_dir() {
            for entry in walkdir::WalkDir::new(&base)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.into_path();
                if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if matches!(ext, "rs" | "sql" | "md") {
                            sources.push(p);
                        }
                    }
                }
            }
        }
    }

    // Resolve tokenizer via canonical discovery (CLI arg > AOS_TOKENIZER_PATH > AOS_MODEL_PATH/tokenizer.json)
    let tokenizer_path = adapteros_config::resolve_tokenizer_path(args.tokenizer.as_ref())
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let tokenizer = adapteros_lora_worker::tokenizer::QwenTokenizer::from_file(&tokenizer_path)
        .with_context(|| format!("Failed to load tokenizer at {}", tokenizer_path.display()))?;

    // Simple heuristics: build (input,target) from comments/instructions to code or migration content
    let re_heading = Regex::new(r"(?m)^\s*//+\s*(.+)$").unwrap();
    let re_sql_create = Regex::new(r"(?i)\bcreate\s+table\b").unwrap();

    let mut examples: Vec<adapteros_lora_worker::training::TrainingExample> = Vec::new();
    for path in sources {
        let content = fs::read_to_string(&path).unwrap_or_default();

        // If SQL migration, use filename as prompt and SQL as target
        if path.extension().and_then(|e| e.to_str()) == Some("sql")
            && re_sql_create.is_match(&content)
        {
            let prompt = format!(
                "Create migration for {}\n{}",
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("migration.sql"),
                path.display()
            );
            let input_ids = tokenizer.encode(&prompt)?;
            let target_ids = tokenizer.encode(&content)?;
            let mut prov = BTreeMap::new();
            prov.insert(
                "file_path".to_string(),
                serde_json::Value::String(path.to_string_lossy().to_string()),
            );
            prov.insert(
                "kind".to_string(),
                serde_json::Value::String("migration".to_string()),
            );
            let created_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let metadata = ExampleMetadataV1::new(
                path.to_string_lossy().to_string(),
                examples.len() as u64,
                "", // source_hash - empty for synthetic data
                provenance_from_map(&prov).unwrap_or_default(),
                created_at,
            );
            let attention_mask = vec![1u8; input_ids.len()];
            examples.push(adapteros_lora_worker::training::TrainingExample::new(
                input_ids,
                target_ids,
                attention_mask,
                metadata,
            ));
            continue;
        }

        // For Rust code in adapteros-db, pair doc/comment lines as prompt with the function impl as target
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let mut last_prompt: Option<String> = None;
            for cap in re_heading.captures_iter(&content) {
                let heading = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                if !heading.is_empty() {
                    last_prompt = Some(heading.to_string());
                }
            }

            if let Some(prompt) = last_prompt {
                let input_ids = tokenizer.encode(&prompt)?;
                // Use tail of file as target (simplified)
                let tail = content
                    .lines()
                    .rev()
                    .take(120)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                let target_ids = tokenizer.encode(&tail)?;
                let mut prov = BTreeMap::new();
                prov.insert(
                    "file_path".to_string(),
                    serde_json::Value::String(path.to_string_lossy().to_string()),
                );
                prov.insert(
                    "kind".to_string(),
                    serde_json::Value::String("rust".to_string()),
                );
                let created_at = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let metadata = ExampleMetadataV1::new(
                    path.to_string_lossy().to_string(),
                    examples.len() as u64,
                    "", // source_hash - empty for synthetic data
                    provenance_from_map(&prov).unwrap_or_default(),
                    created_at,
                );
                let attention_mask = vec![1u8; input_ids.len()];
                examples.push(adapteros_lora_worker::training::TrainingExample::new(
                    input_ids,
                    target_ids,
                    attention_mask,
                    metadata,
                ));
            }
        }
    }

    // Chunk using DatasetGenerator for consistent windowing/validation
    let gen = adapteros_lora_worker::training::DatasetGenerator::new(
        args.context_window,
        args.min_examples,
    );
    gen.validate_examples(&examples)
        .context("Dataset validation failed")?;

    // Write JSON { "examples": [...] }
    let out_dir = args
        .output
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    if !out_dir.exists() {
        fs::create_dir_all(&out_dir).with_context(|| format!("creating {}", out_dir.display()))?;
    }
    let examples_out: Vec<TrainingExampleOut> = examples
        .into_iter()
        .map(|e| {
            let mut map = std::collections::HashMap::new();
            map.insert("dataset_id".to_string(), e.metadata.dataset_id.clone());
            map.insert("row_id".to_string(), e.metadata.row_id.to_string());
            map.insert("provenance".to_string(), e.metadata.provenance);
            map.insert(
                "created_at_unix_ms".to_string(),
                e.metadata.created_at_unix_ms.to_string(),
            );
            TrainingExampleOut {
                input: e.input_tokens,
                target: e.target_tokens,
                metadata: Some(map),
            }
        })
        .collect();
    let td = TrainingDataOut {
        examples: examples_out,
    };
    let json = serde_json::to_string_pretty(&td)?;
    fs::write(&args.output, json).with_context(|| format!("writing {}", args.output.display()))?;

    println!("✓ Wrote dataset: {}", args.output.display());
    Ok(())
}
