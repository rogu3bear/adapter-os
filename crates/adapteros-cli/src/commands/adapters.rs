//! Adapters group commands

use crate::output::OutputWriter;
use adapteros_api_types::adapters::RegisterAdapterRequest;
use adapteros_core::B3Hash;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser, Clone)]
#[command(name = "adapters")]
pub struct AdaptersArgs {
    #[command(subcommand)]
    pub cmd: AdaptersCmd,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AdaptersCmd {
    /// Register a packaged adapter by path (dir or weights file)
    Register(RegisterArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct RegisterArgs {
    /// Path to packaged adapter dir or weights.safetensors
    #[arg(long)]
    pub path: PathBuf,

    /// Adapter ID (defaults to directory name)
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Name to display (defaults to adapter_id)
    #[arg(long)]
    pub name: Option<String>,

    /// Rank (defaults from manifest if present; else 8)
    #[arg(long)]
    pub rank: Option<i32>,

    /// Tier (ephemeral=0, persistent=1) default ephemeral
    #[arg(long)]
    pub tier: Option<i32>,

    /// Control plane base URL
    #[arg(long, default_value = "http://127.0.0.1:8080/api")]
    pub base_url: String,
}

pub async fn run(args: AdaptersArgs, output: &OutputWriter) -> Result<()> {
    match args.cmd {
        AdaptersCmd::Register(reg) => register(reg, output).await,
    }
}

async fn register(args: RegisterArgs, output: &OutputWriter) -> Result<()> {
    let (weights_path, manifest_path, adapter_id_default) = resolve_paths(&args.path)?;

    // Compute B3 hash of weights
    let hash = B3Hash::hash_file(&weights_path)?;
    let hash_b3 = format!("b3:{}", hash.to_hex());

    // Load rank and defaults from manifest if present
    let (rank_default, _name_default) = if manifest_path.exists() {
        let data = std::fs::read_to_string(&manifest_path)?;
        let v: serde_json::Value = serde_json::from_str(&data)?;
        (
            v.get("rank")
                .and_then(|r| r.as_i64())
                .map(|r| r as i32)
                .unwrap_or(8),
            v.get("metadata")
                .and_then(|m| m.get("description"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string()),
        )
    } else {
        (8, None)
    };

    let adapter_id = args
        .adapter_id
        .clone()
        .unwrap_or_else(|| adapter_id_default.clone());
    let name = args.name.clone().unwrap_or_else(|| adapter_id.clone());
    let rank = args.rank.unwrap_or(rank_default);
    let tier = args.tier.unwrap_or(0); // ephemeral by default

    output.info("Registering adapter via HTTP API");
    output.kv("Adapter ID", &adapter_id);
    output.kv("Hash", &hash_b3);
    output.kv("Rank", &rank.to_string());
    output.kv("Tier", &tier.to_string());

    let body = RegisterAdapterRequest {
        adapter_id,
        name,
        hash_b3,
        rank,
        tier,
        languages: vec![],
        framework: None,
    };

    let url = format!(
        "{}/v1/adapters/register",
        args.base_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client.post(&url).json(&body).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Register failed: {} {}", status, text);
    }
    let value: serde_json::Value = resp.json().await?;
    if output.is_json() {
        output.json(&value)?;
    } else if let Some(id) = value.get("adapter_id").and_then(|v| v.as_str()) {
        output.success(format!("Adapter registered: {}", id));
    } else {
        output.success("Adapter registered");
    }
    Ok(())
}

fn resolve_paths(path: &Path) -> Result<(PathBuf, PathBuf, String)> {
    if path.is_dir() {
        let weights = path.join("weights.safetensors");
        let manifest = path.join("manifest.json");
        let adapter_id = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("adapter")
            .to_string();
        Ok((weights, manifest, adapter_id))
    } else {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let manifest = parent.join("manifest.json");
        let adapter_id = parent
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("adapter")
            .to_string();
        Ok((path.to_path_buf(), manifest, adapter_id))
    }
}
