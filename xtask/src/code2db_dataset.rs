//! Build JSON training dataset for code→DB tasks

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

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

    /// Qwen tokenizer path (tokenizer.json)
    #[arg(long, default_value = "models/qwen2.5-7b-mlx/tokenizer.json")]
    pub tokenizer: PathBuf,

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

    // Load tokenizer
    let tokenizer_path = &args.tokenizer;
    let tokenizer = adapteros_lora_worker::tokenizer::QwenTokenizer::from_file(tokenizer_path)
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
            let mut meta = std::collections::HashMap::new();
            meta.insert("file_path".into(), path.to_string_lossy().to_string());
            meta.insert("kind".into(), "migration".into());
            examples.push(adapteros_lora_worker::training::TrainingExample {
                input: input_ids,
                target: target_ids,
                metadata: meta,
            });
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
                let mut meta = std::collections::HashMap::new();
                meta.insert("file_path".into(), path.to_string_lossy().to_string());
                meta.insert("kind".into(), "rust".into());
                examples.push(adapteros_lora_worker::training::TrainingExample {
                    input: input_ids,
                    target: target_ids,
                    metadata: meta,
                });
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
        .map(|e| TrainingExampleOut {
            input: e.input,
            target: e.target,
            metadata: Some(e.metadata),
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
