use anyhow::Result;
use clap::{Args, Subcommand};
use reqwest::Client;
use serde_json::Value;

#[derive(Debug, Subcommand, Clone)]
pub enum RepoCommand {
    /// Repository lifecycle: create/get/list/archive
    #[command(subcommand)]
    Repo(RepoOps),
    /// Version lifecycle: draft/get/list/promote/rollback/tag/resolve
    #[command(subcommand)]
    Version(VersionOps),
}

#[derive(Debug, Subcommand, Clone)]
pub enum RepoOps {
    Create(CreateRepoArgs),
    Get {
        repo_id: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    List {
        #[arg(long, default_value = "default")]
        tenant: String,
        #[arg(long)]
        base_model_id: Option<String>,
        #[arg(long, default_value_t = false)]
        archived: bool,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    Archive {
        repo_id: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
}

#[derive(Debug, Subcommand, Clone)]
pub enum VersionOps {
    Draft(CreateDraftArgs),
    Get {
        version_id: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    List {
        repo_id: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    Promote {
        repo_id: String,
        version_id: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    Rollback {
        repo_id: String,
        branch: String,
        target_version_id: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    Tag {
        version_id: String,
        tag: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
    Resolve {
        repo_id: String,
        selector: String,
        #[arg(long, default_value = "http://127.0.0.1:18080")]
        base_url: String,
    },
}

#[derive(Debug, Args, Clone)]
pub struct CreateRepoArgs {
    /// Tenant ID for the repository
    #[arg(long, default_value = "default")]
    pub tenant: String,
    /// Repository name
    #[arg(long)]
    pub name: String,
    /// Base model identifier
    #[arg(long)]
    pub base_model_id: Option<String>,
    /// Description
    #[arg(long)]
    pub description: Option<String>,
    /// Default branch (default: main)
    #[arg(long, default_value = "main")]
    pub default_branch: String,
    /// Control plane base URL (default: http://127.0.0.1:18080)
    #[arg(long, default_value = "http://127.0.0.1:18080")]
    pub base_url: String,
}

#[derive(Debug, Args, Clone)]
pub struct CreateDraftArgs {
    pub repo_id: String,
    #[arg(long, default_value = "main")]
    pub branch: String,
    #[arg(long)]
    pub parent_version_id: Option<String>,
    #[arg(long)]
    pub code_commit_sha: Option<String>,
    #[arg(long)]
    pub data_spec_hash: Option<String>,
    #[arg(long, default_value = "http://127.0.0.1:18080")]
    pub base_url: String,
}

pub async fn run_repo_command(cmd: RepoCommand, json: bool) -> Result<()> {
    let client = Client::new();
    match cmd {
        RepoCommand::Repo(op) => match op {
            RepoOps::Create(args) => create_repo(&client, args, json).await,
            RepoOps::Get { repo_id, base_url } => {
                let url = format!("{}/v1/adapter-repositories/{}", base_url, repo_id);
                render_json(client.get(url).send().await?, json).await
            }
            RepoOps::List {
                tenant,
                base_model_id,
                archived,
                base_url,
            } => {
                let mut url = format!("{}/v1/adapter-repositories?tenant_id={}", base_url, tenant);
                if let Some(model) = base_model_id {
                    url.push_str(&format!("&base_model_id={}", model));
                }
                if archived {
                    url.push_str("&archived=true");
                }
                render_json(client.get(url).send().await?, json).await
            }
            RepoOps::Archive { repo_id, base_url } => {
                let url = format!("{}/v1/adapter-repositories/{}/archive", base_url, repo_id);
                let resp = client.post(url).send().await?;
                if resp.status().is_success() {
                    println!("archived repository {}", repo_id);
                    Ok(())
                } else {
                    render_json(resp, json).await
                }
            }
        },
        RepoCommand::Version(op) => match op {
            VersionOps::Draft(args) => create_draft_version(&client, args, json).await,
            VersionOps::Get {
                version_id,
                base_url,
            } => {
                let url = format!("{}/v1/adapter-versions/{}", base_url, version_id);
                render_json(client.get(url).send().await?, json).await
            }
            VersionOps::List {
                repo_id,
                branch,
                state,
                base_url,
            } => {
                let mut url = format!("{}/v1/adapter-repositories/{}/versions", base_url, repo_id);
                if let Some(branch) = branch {
                    url.push_str(&format!("?branch={}", branch));
                }
                if let Some(state) = state {
                    let sep = if url.contains('?') { "&" } else { "?" };
                    url.push_str(&format!("{}state={}", sep, state));
                }
                render_json(client.get(url).send().await?, json).await
            }
            VersionOps::Promote {
                repo_id,
                version_id,
                base_url,
            } => {
                let url = format!("{}/v1/adapter-versions/{}/promote", base_url, version_id);
                let body = serde_json::json!({ "repo_id": repo_id });
                render_json(client.post(url).json(&body).send().await?, json).await
            }
            VersionOps::Rollback {
                repo_id,
                branch,
                target_version_id,
                base_url,
            } => {
                let url = format!(
                    "{}/v1/adapter-repositories/{}/versions/rollback",
                    base_url, repo_id
                );
                let body =
                    serde_json::json!({ "branch": branch, "target_version_id": target_version_id });
                render_json(client.post(url).json(&body).send().await?, json).await
            }
            VersionOps::Tag {
                version_id,
                tag,
                base_url,
            } => {
                let url = format!("{}/v1/adapter-versions/{}/tag", base_url, version_id);
                let body = serde_json::json!({ "tag_name": tag });
                render_json(client.post(url).json(&body).send().await?, json).await
            }
            VersionOps::Resolve {
                repo_id,
                selector,
                base_url,
            } => {
                let url = format!(
                    "{}/v1/adapter-repositories/{}/resolve-version",
                    base_url, repo_id
                );
                let body = serde_json::json!({ "selector": selector });
                render_json(client.post(url).json(&body).send().await?, json).await
            }
        },
    }
}

async fn create_repo(client: &Client, args: CreateRepoArgs, json: bool) -> Result<()> {
    let url = format!("{}/v1/adapter-repositories", args.base_url);
    let body = serde_json::json!({
        "tenant_id": args.tenant,
        "name": args.name,
        "base_model_id": args.base_model_id,
        "description": args.description,
        "default_branch": args.default_branch,
    });
    render_json(client.post(url).json(&body).send().await?, json).await
}

async fn create_draft_version(client: &Client, args: CreateDraftArgs, json: bool) -> Result<()> {
    let url = format!("{}/v1/adapter-versions/draft", args.base_url);
    let body = serde_json::json!({
        "repo_id": args.repo_id,
        "branch": args.branch,
        "parent_version_id": args.parent_version_id,
        "code_commit_sha": args.code_commit_sha,
        "data_spec_hash": args.data_spec_hash,
    });
    render_json(client.post(url).json(&body).send().await?, json).await
}

async fn render_json(resp: reqwest::Response, json: bool) -> Result<()> {
    let status = resp.status();
    let text = resp.text().await?;
    let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| Value::String(text.clone()));
    if !status.is_success() {
        eprintln!("Request failed: {}", status);
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("{}", text);
    }
    Ok(())
}
