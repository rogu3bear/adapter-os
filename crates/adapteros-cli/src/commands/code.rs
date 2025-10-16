///! Code intelligence CLI commands
///!
///! Handles repository registration, scanning, and status queries

use crate::output::OutputWriter;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

/// Code repository information
#[derive(Debug, Serialize, Deserialize)]
pub struct CodeRepository {
    pub repo_id: String,
    pub path: String,
    pub languages: Vec<String>,
    pub default_branch: String,
    pub latest_scan_commit: Option<String>,
    pub latest_scan_at: Option<String>,
    pub status: String,
}

/// Scan job status
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanJobStatus {
    pub job_id: String,
    pub status: String,
    pub progress_pct: i32,
    pub current_stage: Option<String>,
}

/// Initialize a code repository for scanning
pub async fn code_init(
    repo_path: &PathBuf,
    tenant_id: &str,
    output: &OutputWriter,
) -> Result<()> {
    output.info(&format!("Initializing repository at {:?}", repo_path));

    // Detect languages (simplified)
    let languages = detect_languages(repo_path)?;

    // Determine repo_id from path
    let repo_id = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Detect default branch (simplified - would use git2)
    let default_branch = "main".to_string();

    // Call API to register repository
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:8080/api/v1/code/register-repo")
        .json(&json!({
            "tenant_id": tenant_id,
            "repo_id": repo_id,
            "path": repo_path.to_string_lossy(),
            "languages": languages,
            "default_branch": default_branch,
        }))
        .send()
        .await?;

    if response.status().is_success() {
        output.success(&format!("Repository {} registered successfully", repo_id));
        output.json(&json!({
            "status": "registered",
            "repo_id": repo_id,
            "path": repo_path.to_string_lossy(),
        }))?;
    } else {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to register repository: {}", error_text);
    }

    Ok(())
}

/// Update repository scan
pub async fn code_update(
    repo_id: &str,
    tenant_id: &str,
    commit: Option<&str>,
    output: &OutputWriter,
) -> Result<()> {
    output.info(&format!("Triggering scan for repository {}", repo_id));

    // Get current commit if not provided (would use git2)
    let commit_sha = commit.unwrap_or("HEAD").to_string();

    // Call API to trigger scan
    let client = reqwest::Client::new();
    let response = client
        .post("http://localhost:8080/api/v1/code/scan")
        .json(&json!({
            "tenant_id": tenant_id,
            "repo_id": repo_id,
            "commit": commit_sha,
            "full_scan": true,
        }))
        .send()
        .await?;

    if response.status().is_success() {
        let result: serde_json::Value = response.json().await?;
        let job_id = result["job_id"].as_str().unwrap_or("unknown");

        output.success(&format!("Scan job created: {}", job_id));
        output.json(&result)?;

        // Poll for job completion
        if !output.is_json() {
            output.info("Waiting for scan to complete...");
            poll_scan_job(job_id, output).await?;
        }
    } else {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to trigger scan: {}", error_text);
    }

    Ok(())
}

/// List registered repositories
pub async fn code_list(tenant_id: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:8080/api/v1/code/repositories")
        .query(&[("tenant_id", tenant_id)])
        .send()
        .await?;

    if response.status().is_success() {
        let result: serde_json::Value = response.json().await?;

        if output.is_json() {
            output.json(&result)?;
        } else {
            let empty_vec = vec![];
            let repos = result["repos"].as_array().unwrap_or(&empty_vec);

            if repos.is_empty() {
                output.info("No repositories registered");
            } else {
                output.info(&format!("Registered repositories ({}):", repos.len()));
                for repo in repos {
                    let repo_id = repo["repo_id"].as_str().unwrap_or("unknown");
                    let status = repo["status"].as_str().unwrap_or("unknown");
                    let scan_commit = repo["latest_scan_commit"]
                        .as_str()
                        .unwrap_or("not scanned");

                    println!("  {} ({}): {}", repo_id, status, scan_commit);
                }
            }
        }
    } else {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to list repositories: {}", error_text);
    }

    Ok(())
}

/// Get repository status
pub async fn code_status(repo_id: &str, tenant_id: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .get(&format!(
            "http://localhost:8080/api/v1/code/repositories/{}",
            repo_id
        ))
        .query(&[("tenant_id", tenant_id)])
        .send()
        .await?;

    if response.status().is_success() {
        let result: serde_json::Value = response.json().await?;

        if output.is_json() {
            output.json(&result)?;
        } else {
            let status = result["status"].as_str().unwrap_or("unknown");
            let path = result["path"].as_str().unwrap_or("unknown");
            let languages = result["languages"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            output.info(&format!("Repository: {}", repo_id));
            println!("  Status: {}", status);
            println!("  Path: {}", path);
            println!("  Languages: {}", languages);

            if let Some(commit) = result["latest_scan_commit"].as_str() {
                println!("  Latest scan: {}", commit);
            }
            if let Some(scan_at) = result["latest_scan_at"].as_str() {
                println!("  Scanned at: {}", scan_at);
            }
        }
    } else {
        let error_text = response.text().await?;
        anyhow::bail!("Failed to get repository status: {}", error_text);
    }

    Ok(())
}

/// Poll scan job until completion
async fn poll_scan_job(job_id: &str, output: &OutputWriter) -> Result<()> {
    let client = reqwest::Client::new();
    let mut last_progress = 0;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        let response = client
            .get(&format!("http://localhost:8080/api/v1/code/scan/{}", job_id))
            .send()
            .await?;

        if !response.status().is_success() {
            break;
        }

        let result: serde_json::Value = response.json().await?;
        let status = result["status"].as_str().unwrap_or("unknown");
        let progress = result["progress"]["percentage"].as_i64().unwrap_or(0) as i32;
        let stage = result["progress"]["current_stage"]
            .as_str()
            .unwrap_or("unknown");

        if progress > last_progress {
            output.info(&format!("Progress: {}% ({})", progress, stage));
            last_progress = progress;
        }

        match status {
            "completed" => {
                output.success("Scan completed successfully");
                if let Some(result_obj) = result["result"].as_object() {
                    let symbol_count = result_obj["symbol_count"].as_i64().unwrap_or(0);
                    let file_count = result_obj["file_count"].as_i64().unwrap_or(0);
                    println!("  Files: {}, Symbols: {}", file_count, symbol_count);
                }
                break;
            }
            "failed" => {
                let error = result["error_message"].as_str().unwrap_or("Unknown error");
                anyhow::bail!("Scan failed: {}", error);
            }
            _ => {
                // Continue polling
            }
        }
    }

    Ok(())
}

/// Detect languages in repository (simplified)
fn detect_languages(repo_path: &PathBuf) -> Result<Vec<String>> {
    let mut languages = std::collections::HashSet::new();

    // Walk directory and detect languages by extension
    if repo_path.is_dir() {
        for entry in walkdir::WalkDir::new(repo_path)
            .max_depth(3)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if let Some(ext) = entry.path().extension() {
                let lang = match ext.to_str() {
                    Some("rs") => Some("Rust"),
                    Some("py") => Some("Python"),
                    Some("js") => Some("JavaScript"),
                    Some("ts") => Some("TypeScript"),
                    Some("go") => Some("Go"),
                    Some("java") => Some("Java"),
                    _ => None,
                };

                if let Some(lang) = lang {
                    languages.insert(lang.to_string());
                }
            }
        }
    }

    Ok(languages.into_iter().collect())
}

