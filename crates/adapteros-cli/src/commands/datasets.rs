//! Dataset management commands backed by the control-plane APIs.
//! Implements create, ingest, list, version inspection, manifest summaries,
//! validation triggers, and trust_state visibility for scripting and
//! interactive use.

use crate::auth_store::{load_auth, warn_if_tenant_mismatch};
use crate::http_client::send_with_refresh_from_store;
use crate::output::{OutputWriter, Table};
use adapteros_api_types::dataset_domain::DatasetManifest;
use adapteros_api_types::training::{
    DatasetResponse, UploadDatasetResponse, ValidateDatasetResponse,
};
use adapteros_core::{AosError, Result};
use adapteros_db::training_datasets::TrainingDatasetVersion;
use adapteros_db::Db;
use adapteros_lora_worker::training::{
    DatasetBuilder, DatasetSource, GitAuth,
};
use clap::{Args, Subcommand};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL};
use reqwest::{multipart, Client};
use serde::Serialize;
use std::path::PathBuf;

/// Dataset management command
pub type DatasetCommand = DatasetSubcommand;

/// Dataset subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum DatasetSubcommand {
    /// Create a dataset identity from documents/collections (no local files)
    #[command(after_help = r#"Examples:
  # Create from a single document
  aosctl dataset create --document-id doc-123 --name reviews

  # Create from multiple documents
  aosctl dataset create --document-ids doc-1 doc-2 --name combined

  # Create from a collection
  aosctl dataset create --collection-id coll-9 --name coll_ds
"#)]
    Create(CreateArgs),

    /// Ingest local files into a new dataset version
    #[command(after_help = r#"Examples:
  # Ingest a single file
  aosctl dataset ingest ./data/train.jsonl

  # Ingest multiple files with format hint
  aosctl dataset ingest ./data/*.jsonl --format jsonl

  # Ingest into an existing dataset
  aosctl dataset ingest ./data/new.jsonl --dataset-id ds-123

  # JSON output
  aosctl dataset ingest ./data/train.jsonl --json
"#)]
    Ingest(IngestArgs),

    /// List datasets with validation/trainability state
    #[command(after_help = r#"Examples:
  aosctl dataset list
  aosctl dataset list --trust-state allowed
  aosctl dataset list --name contains \"reviews\" --json
"#)]
    List(ListArgs),

    /// List dataset versions for a dataset
    #[command(after_help = r#"Examples:
  aosctl dataset versions ds-123
  aosctl dataset versions ds-123 --json
"#)]
    Versions(VersionsArgs),

    /// Show manifest/validation/trust for a dataset version
    #[command(after_help = r#"Examples:
  aosctl dataset show dsv-abc123
  aosctl dataset show dsv-abc123 --json
"#)]
    Show(ShowArgs),

    /// Trigger validation for a dataset (creates/uses latest version)
    #[command(after_help = r#"Examples:
  aosctl dataset validate ds-123
  aosctl dataset validate --dataset-version-id dsv-abc123
"#)]
    Validate(ValidateArgs),

    /// Build a canonical dataset from raw sources (local operation)
    #[command(after_help = r#"Examples:
  # Build from JSONL
  aosctl dataset build ./data/train.jsonl --tokenizer ./models/Qwen/tokenizer.json

  # Build from git repo (public)
  aosctl dataset build --git https://github.com/user/data.git --path data/

  # Build from git repo (private with token)
  aosctl dataset build --git https://github.com/user/private.git --git-auth token

  # Build from archive
  aosctl dataset build ./data.tar.gz --output ./dataset-out

  # Dry run (validate without writing)
  aosctl dataset build ./data.jsonl --tokenizer ./tokenizer.json --dry-run
"#)]
    Build(BuildArgs),
}

#[derive(Debug, Args, Clone)]
pub struct CreateArgs {
    /// Dataset name
    #[arg(long)]
    pub name: Option<String>,
    /// Dataset type (freeform, e.g. training, evaluation)
    #[arg(long)]
    pub dataset_type: Option<String>,
    /// Purpose (e.g. chat-finetune, safety-eval)
    #[arg(long)]
    pub purpose: Option<String>,
    /// Source location hint (e.g. bucket path or URL)
    #[arg(long)]
    pub source_location: Option<String>,
    /// Optional tags
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    /// Single document id
    #[arg(long)]
    pub document_id: Option<String>,
    /// Multiple document ids
    #[arg(long)]
    pub document_ids: Vec<String>,
    /// Collection id
    #[arg(long)]
    pub collection_id: Option<String>,
    /// Optional description
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct IngestArgs {
    /// Files to upload (JSONL only)
    #[arg(required = true)]
    pub files: Vec<PathBuf>,
    /// Existing dataset id (omit to create new)
    #[arg(long)]
    pub dataset_id: Option<String>,
    /// Format hint passed to backend (jsonl only)
    #[arg(long)]
    pub format: Option<String>,
    /// Optional dataset name when creating
    #[arg(long)]
    pub name: Option<String>,
    /// Optional description
    #[arg(long)]
    pub description: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct ListArgs {
    /// Filter by trust_state (allowed, allowed_with_warning, blocked, needs_approval)
    #[arg(long)]
    pub trust_state: Option<String>,
    /// Filter by dataset name substring (case-insensitive)
    #[arg(long)]
    pub name: Option<String>,
    /// Limit results
    #[arg(long, default_value = "50")]
    pub limit: u32,
}

#[derive(Debug, Args, Clone)]
pub struct VersionsArgs {
    /// Dataset id
    pub dataset_id: String,
}

#[derive(Debug, Args, Clone)]
pub struct ShowArgs {
    /// Dataset version id
    pub dataset_version_id: String,
}

#[derive(Debug, Args, Clone)]
pub struct ValidateArgs {
    /// Dataset id (validated via API)
    #[arg(long, conflicts_with = "dataset_version_id")]
    pub dataset_id: Option<String>,
    /// Dataset version id (resolved to dataset id)
    #[arg(long)]
    pub dataset_version_id: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct BuildArgs {
    /// Source path (file, directory, or archive)
    #[arg(required_unless_present = "git")]
    pub source: Option<PathBuf>,

    /// Git repository URL
    #[arg(long)]
    pub git: Option<String>,

    /// Path within git repo
    #[arg(long)]
    pub path: Option<String>,

    /// Git branch/tag
    #[arg(long)]
    pub branch: Option<String>,

    /// Git auth: none, ssh, token, credential-helper
    #[arg(long, default_value = "none")]
    pub git_auth: String,

    /// Git auth token (for --git-auth token), can also use AOS_GIT_TOKEN env var
    #[arg(long, env = "AOS_GIT_TOKEN")]
    pub git_token: Option<String>,

    /// SSH key path (for --git-auth ssh)
    #[arg(long)]
    pub ssh_key: Option<PathBuf>,

    /// Output directory
    #[arg(long, short, default_value = "./dataset-build")]
    pub output: PathBuf,

    /// Tokenizer path (required)
    #[arg(long, env = "AOS_TOKENIZER_PATH")]
    pub tokenizer: PathBuf,

    /// Format hint (jsonl only)
    #[arg(long)]
    pub format: Option<String>,

    /// Text parsing strategy (unsupported; JSONL only)
    #[arg(long, default_value = "paragraph-pairs")]
    pub text_strategy: String,

    /// Input column name (unsupported; JSONL only)
    #[arg(long)]
    pub input_col: Option<String>,

    /// Target column name (unsupported; JSONL only)
    #[arg(long)]
    pub target_col: Option<String>,

    /// Dataset name
    #[arg(long)]
    pub name: Option<String>,

    /// Dry run (validate without writing)
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Serialize)]
struct IngestResult {
    dataset_id: String,
    dataset_version_id: String,
    file_count: i32,
    total_size_bytes: i64,
    validation_status: String,
    trust_state: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct DatasetListRow {
    dataset_id: String,
    name: String,
    validation_status: String,
    trust_state: Option<String>,
    file_count: i32,
    total_size_bytes: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct VersionRow {
    dataset_version_id: String,
    dataset_id: String,
    version_number: i64,
    validation_status: String,
    trust_state: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct ShowResult {
    dataset_version_id: String,
    dataset_id: String,
    trust_state: String,
    validation_status: String,
    hash_b3: String,
    manifest: Option<DatasetManifest>,
}

/// Execute dataset commands
pub async fn run(cmd: DatasetCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DatasetSubcommand::Create(args) => create_dataset(args, output).await,
        DatasetSubcommand::Ingest(args) => ingest_dataset(args, output).await,
        DatasetSubcommand::List(args) => list_datasets(args, output).await,
        DatasetSubcommand::Versions(args) => list_versions(args, output).await,
        DatasetSubcommand::Show(args) => show_version(args, output).await,
        DatasetSubcommand::Validate(args) => validate_dataset(args, output).await,
        DatasetSubcommand::Build(args) => build_dataset(args, output).await,
    }
}

async fn create_dataset(args: CreateArgs, output: &OutputWriter) -> Result<()> {
    let mut _auth = load_auth()
        .map_err(|e| AosError::Io(format!("Failed to load auth: {e}")))?
        .ok_or_else(|| AosError::Validation("No stored auth; run `aosctl auth login`".into()))?;

    warn_if_tenant_mismatch(None, output);

    let client = Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;

    let request_body = serde_json::json!({
        "document_id": args.document_id,
        "document_ids": if args.document_ids.is_empty() { None::<Vec<String>> } else { Some(args.document_ids.clone()) },
        "collection_id": args.collection_id,
        "name": args.name,
        "description": args.description,
        "dataset_type": args.dataset_type,
        "purpose": args.purpose,
        "source_location": args.source_location,
        "tags": if args.tags.is_empty() { None::<Vec<String>> } else { Some(args.tags.clone()) },
    });

    let resp = send_with_refresh_from_store(&client, |client, store| {
        let url = format!(
            "{}/v1/datasets/from-documents",
            store.base_url.trim_end_matches('/')
        );
        client
            .post(url)
            .bearer_auth(&store.token)
            .json(&request_body)
    })
    .await
    .map_err(|e| AosError::Io(format!("Create request failed: {e}")))?;

    let dataset: DatasetResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Io(format!("Failed to parse dataset response: {e}")))?;

    let db = Db::connect_env().await?;
    let version_id = db
        .ensure_dataset_version_exists(&dataset.dataset_id)
        .await?;
    let trust_state = db
        .get_effective_trust_state(&version_id)
        .await?
        .unwrap_or_else(|| "unknown".to_string());

    if output.is_json() {
        output.json(&serde_json::json!({
            "dataset_id": dataset.dataset_id,
            "dataset_version_id": version_id,
            "trust_state": trust_state,
            "validation_status": dataset.validation_status,
        }))?;
    } else {
        output.section("Dataset created");
        output.kv("Dataset", &dataset.dataset_id);
        output.kv("Version", &version_id);
        output.kv("Validation", &format!("{:?}", dataset.validation_status));
        output.kv("Trust", &trust_state);
    }

    Ok(())
}

async fn ingest_dataset(args: IngestArgs, output: &OutputWriter) -> Result<()> {
    let mut _auth = load_auth()
        .map_err(|e| AosError::Io(format!("Failed to load auth: {e}")))?
        .ok_or_else(|| AosError::Validation("No stored auth; run `aosctl auth login`".into()))?;

    warn_if_tenant_mismatch(None, output);

    if let Some(ref fmt) = args.format {
        if fmt.to_ascii_lowercase() != "jsonl" {
            return Err(AosError::Validation(
                "Only jsonl format is supported by PLAN_4".to_string(),
            ));
        }
    }

    let client = Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;

    // Pre-read files so we can rebuild the multipart body if refresh is needed.
    let mut file_payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for path in &args.files {
        if !path.exists() {
            return Err(AosError::Io(format!("File not found: {}", path.display())));
        }
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read {}: {e}", path.display())))?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| AosError::Validation("Invalid file name".into()))?
            .to_string();
        file_payloads.push((name, data));
    }

    let format_hint = args.format.clone();
    let name_hint = args.name.clone();
    let desc_hint = args.description.clone();

    let resp = send_with_refresh_from_store(&client, |client, store| {
        let url = format!(
            "{}/v1/datasets/upload",
            store.base_url.trim_end_matches('/')
        );
        let mut form = multipart::Form::new();
        if let Some(ref fmt) = format_hint {
            form = form.text("format", fmt.clone());
        }
        if let Some(ref n) = name_hint {
            form = form.text("name", n.clone());
        }
        if let Some(ref d) = desc_hint {
            form = form.text("description", d.clone());
        }
        for (fname, data) in &file_payloads {
            let part = multipart::Part::bytes(data.clone()).file_name(fname.clone());
            form = form.part("file", part);
        }
        client.post(url).bearer_auth(&store.token).multipart(form)
    })
    .await
    .map_err(|e| AosError::Io(format!("Upload request failed: {e}")))?;

    let upload: UploadDatasetResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Io(format!("Failed to parse upload response: {e}")))?;

    if let Some(ref expected) = args.dataset_id {
        if expected != &upload.dataset_id {
            output.warning(format!(
                "Requested dataset {} but backend created {}",
                expected, upload.dataset_id
            ));
        }
    }

    let db = Db::connect_env().await?;
    let dataset_id = upload.dataset_id.clone();
    let version_id = db.ensure_dataset_version_exists(&dataset_id).await?;
    let trust_state = db
        .get_effective_trust_state(&version_id)
        .await?
        .unwrap_or_else(|| "unknown".to_string());

    let result = IngestResult {
        dataset_id,
        dataset_version_id: version_id,
        file_count: upload.file_count,
        total_size_bytes: upload.total_size_bytes,
        validation_status: "pending".to_string(),
        trust_state,
        created_at: upload.created_at,
    };

    if output.is_json() {
        output.json(&result)?;
    } else {
        output.section("Ingest completed");
        output.kv("Dataset", &result.dataset_id);
        output.kv("Version", &result.dataset_version_id);
        output.kv("Files", &result.file_count.to_string());
        output.kv("Size (bytes)", &result.total_size_bytes.to_string());
        output.kv("Validation", &result.validation_status);
        output.kv("Trust", &result.trust_state);
    }

    Ok(())
}

async fn list_datasets(args: ListArgs, output: &OutputWriter) -> Result<()> {
    let mut _auth = load_auth()
        .map_err(|e| AosError::Io(format!("Failed to load auth: {e}")))?
        .ok_or_else(|| AosError::Validation("No stored auth; run `aosctl auth login`".into()))?;

    warn_if_tenant_mismatch(None, output);

    let client = Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;

    let resp = send_with_refresh_from_store(&client, |client, store| {
        let url = format!(
            "{}/v1/datasets?limit={}",
            store.base_url.trim_end_matches('/'),
            args.limit
        );
        client.get(url).bearer_auth(&store.token)
    })
    .await
    .map_err(|e| AosError::Io(format!("List request failed: {e}")))?;

    let datasets: Vec<DatasetResponse> = resp
        .json()
        .await
        .map_err(|e| AosError::Io(format!("Failed to parse list response: {e}")))?;

    let db = Db::connect_env().await?;
    let mut rows = Vec::new();
    for ds in datasets {
        let version_id = db.ensure_dataset_version_exists(&ds.dataset_id).await?;
        let trust_state = db
            .get_effective_trust_state(&version_id)
            .await?
            .unwrap_or_else(|| "unknown".to_string());

        if let Some(ref filter) = args.trust_state {
            if !trust_state.eq_ignore_ascii_case(filter) {
                continue;
            }
        }
        if let Some(ref name_filter) = args.name {
            if !ds.name.to_lowercase().contains(&name_filter.to_lowercase()) {
                continue;
            }
        }

        rows.push(DatasetListRow {
            dataset_id: ds.dataset_id,
            name: ds.name,
            validation_status: format!("{:?}", ds.validation_status),
            trust_state: Some(trust_state),
            file_count: ds.file_count,
            total_size_bytes: ds.total_size_bytes,
            created_at: ds.created_at,
            updated_at: ds.updated_at,
        });
    }

    if output.is_json() {
        output.json(&rows)?;
        return Ok(());
    }

    let mut table = Table::new();
    table
        .set_header(vec![
            "dataset_id",
            "trust_state",
            "validation",
            "files",
            "size_bytes",
            "updated_at",
        ])
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    for row in &rows {
        table.add_row(vec![
            row.dataset_id.clone(),
            row.trust_state.clone().unwrap_or_else(|| "unknown".into()),
            row.validation_status.clone(),
            row.file_count.to_string(),
            row.total_size_bytes.to_string(),
            row.updated_at.clone(),
        ]);
    }

    if !output.is_quiet() {
        println!("{table}");
    }

    Ok(())
}

async fn list_versions(args: VersionsArgs, output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;
    let versions = db.list_all_dataset_versions().await?;
    let mut filtered: Vec<TrainingDatasetVersion> = versions
        .into_iter()
        .filter(|v| v.dataset_id == args.dataset_id)
        .collect();
    filtered.sort_by_key(|v| v.version_number);

    let rows: Vec<VersionRow> = filtered
        .iter()
        .map(|v| VersionRow {
            dataset_version_id: v.id.clone(),
            dataset_id: v.dataset_id.clone(),
            version_number: v.version_number,
            validation_status: v.validation_status.clone(),
            trust_state: v.trust_state.clone(),
            created_at: v.created_at.clone(),
        })
        .collect();

    if output.is_json() {
        output.json(&rows)?;
        return Ok(());
    }

    let mut table = Table::new();
    table
        .set_header(vec![
            "version_id",
            "version",
            "validation",
            "trust_state",
            "created_at",
        ])
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    for row in &rows {
        table.add_row(vec![
            row.dataset_version_id.clone(),
            row.version_number.to_string(),
            row.validation_status.clone(),
            row.trust_state.clone(),
            row.created_at.clone(),
        ]);
    }

    if !output.is_quiet() {
        println!("{table}");
    }

    Ok(())
}

async fn show_version(args: ShowArgs, output: &OutputWriter) -> Result<()> {
    let mut _auth = load_auth()
        .map_err(|e| AosError::Io(format!("Failed to load auth: {e}")))?
        .ok_or_else(|| AosError::Validation("No stored auth; run `aosctl auth login`".into()))?;

    let client = Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;

    let manifest_resp = send_with_refresh_from_store(&client, |client, store| {
        let url = format!(
            "{}/v1/training/dataset_versions/{}/manifest",
            store.base_url.trim_end_matches('/'),
            args.dataset_version_id
        );
        client.get(url).bearer_auth(&store.token)
    })
    .await;

    let manifest: Option<DatasetManifest> = match manifest_resp {
        Ok(resp) => resp.json().await.ok(),
        Err(_) => None,
    };

    let db = Db::connect_env().await?;
    let version = db
        .get_training_dataset_version(&args.dataset_version_id)
        .await?
        .ok_or_else(|| AosError::NotFound("Dataset version not found".into()))?;
    let trust_state = db
        .get_effective_trust_state(&version.id)
        .await?
        .unwrap_or_else(|| version.trust_state.clone());

    let result = ShowResult {
        dataset_version_id: version.id.clone(),
        dataset_id: version.dataset_id.clone(),
        trust_state,
        validation_status: version.validation_status.clone(),
        hash_b3: version.hash_b3.clone(),
        manifest,
    };

    if output.is_json() {
        output.json(&result)?;
    } else {
        output.section("Dataset version");
        output.kv("Dataset", &result.dataset_id);
        output.kv("Version", &result.dataset_version_id);
        output.kv("Trust", &result.trust_state);
        output.kv("Validation", &result.validation_status);
        output.kv("Hash", &result.hash_b3);
        if let Some(m) = &result.manifest {
            output.kv("Rows", &m.total_rows.to_string());
            output.kv(
                "Splits",
                &m.splits.keys().cloned().collect::<Vec<_>>().join(","),
            );
        }
    }

    Ok(())
}

async fn validate_dataset(args: ValidateArgs, output: &OutputWriter) -> Result<()> {
    let mut _auth = load_auth()
        .map_err(|e| AosError::Io(format!("Failed to load auth: {e}")))?
        .ok_or_else(|| AosError::Validation("No stored auth; run `aosctl auth login`".into()))?;

    let db = Db::connect_env().await?;

    let (dataset_id, version_id) = if let Some(ref vid) = args.dataset_version_id {
        let v = db
            .get_training_dataset_version(vid)
            .await?
            .ok_or_else(|| AosError::NotFound("Dataset version not found".into()))?;
        (v.dataset_id.clone(), v.id.clone())
    } else if let Some(ref did) = args.dataset_id {
        let vid = db.ensure_dataset_version_exists(did).await?;
        (did.clone(), vid)
    } else {
        return Err(AosError::Validation(
            "Provide either --dataset-id or --dataset-version-id".into(),
        ));
    };

    let client = Client::builder()
        .cookie_store(true)
        .build()
        .map_err(|e| AosError::Io(format!("HTTP client build failed: {e}")))?;

    let resp = send_with_refresh_from_store(&client, |client, store| {
        let url = format!(
            "{}/v1/datasets/{}/validate",
            store.base_url.trim_end_matches('/'),
            dataset_id
        );
        client
            .post(url)
            .bearer_auth(&store.token)
            .json(&serde_json::json!({"check_format": true}))
    })
    .await
    .map_err(|e| AosError::Io(format!("Validate request failed: {e}")))?;

    let validation: ValidateDatasetResponse = resp
        .json()
        .await
        .map_err(|e| AosError::Io(format!("Failed to parse validation response: {e}")))?;

    // Refresh trust state after validation
    let trust_state = db
        .get_effective_trust_state(&version_id)
        .await?
        .unwrap_or_else(|| "unknown".to_string());

    if output.is_json() {
        output.json(&serde_json::json!({
            "dataset_id": dataset_id,
            "dataset_version_id": version_id,
            "is_valid": validation.is_valid,
            "validation_status": validation.validation_status,
            "trust_state": trust_state
        }))?;
    } else {
        output.section("Validation");
        output.kv("Dataset", &dataset_id);
        output.kv("Version", &version_id);
        output.kv("Valid", &validation.is_valid.to_string());
        output.kv("Validation", &format!("{:?}", validation.validation_status));
        output.kv("Trust", &trust_state);
    }

    Ok(())
}

async fn build_dataset(args: BuildArgs, output: &OutputWriter) -> Result<()> {
    // Validate tokenizer path exists
    if !args.tokenizer.exists() {
        return Err(AosError::Io(format!(
            "Tokenizer not found: {}",
            args.tokenizer.display()
        )));
    }

    // Enforce PLAN_4 JSONL-only contract.
    if let Some(ref fmt) = args.format {
        if fmt.to_ascii_lowercase() != "jsonl" {
            return Err(AosError::Validation(
                "Only jsonl format is supported by PLAN_4".to_string(),
            ));
        }
    }
    if args.input_col.is_some() || args.target_col.is_some() {
        return Err(AosError::Validation(
            "CSV column mapping is not supported by PLAN_4 (JSONL only)".to_string(),
        ));
    }
    if args.text_strategy.to_ascii_lowercase() != "paragraph-pairs" {
        return Err(AosError::Validation(
            "Text parsing strategies are not supported by PLAN_4 (JSONL only)".to_string(),
        ));
    }

    // Build git auth
    let git_auth = match args.git_auth.to_lowercase().as_str() {
        "none" => GitAuth::None,
        "ssh" => GitAuth::SshKey(args.ssh_key.clone()),
        "token" => {
            let token = args.git_token.clone().ok_or_else(|| {
                AosError::Validation(
                    "Git token required for --git-auth token. Use --git-token or AOS_GIT_TOKEN env var".into()
                )
            })?;
            GitAuth::HttpsToken(token)
        }
        "credential-helper" => GitAuth::CredentialHelper,
        other => {
            return Err(AosError::Validation(format!(
                "Unknown git auth type: {}. Use: none, ssh, token, credential-helper",
                other
            )))
        }
    };

    // Determine source
    let source = if let Some(ref url) = args.git {
        DatasetSource::Git {
            url: url.clone(),
            branch: args.branch.clone(),
            path: args.path.clone(),
            auth: git_auth,
        }
    } else if let Some(ref path) = args.source {
        // Check if it's an archive
        let path_str = path.display().to_string().to_lowercase();
        if path_str.ends_with(".zip")
            || path_str.ends_with(".tar.gz")
            || path_str.ends_with(".tgz")
            || path_str.ends_with(".tar")
        {
            DatasetSource::Archive(path.clone())
        } else {
            DatasetSource::Filesystem(path.clone())
        }
    } else {
        return Err(AosError::Validation(
            "Either source path or --git URL required".into(),
        ));
    };

    // Build the dataset builder
    let mut builder = DatasetBuilder::new(args.tokenizer.clone(), args.output.clone());
    if let Some(ref name) = args.name {
        builder = builder.with_name(name.clone());
    }

    // Dry run or actual build
    if args.dry_run {
        output.section("Dry run validation");
        let count = builder.validate(&source)?;
        output.kv("Sample count", &count.to_string());
        output.kv("Status", "Valid");
        if output.is_json() {
            output.json(&serde_json::json!({
                "sample_count": count,
                "valid": true,
                "dry_run": true,
            }))?;
        }
    } else {
        let result = builder.build(&source)?;
        if output.is_json() {
            output.json(&serde_json::json!({
                "manifest_path": result.manifest_path.display().to_string(),
                "examples_path": result.examples_path.display().to_string(),
                "example_count": result.example_count,
                "tokenizer_hash": result.tokenizer_hash,
                "dataset_hash": result.dataset_hash,
            }))?;
        } else {
            output.section("Dataset built");
            output.kv("Manifest", &result.manifest_path.display().to_string());
            output.kv("Examples", &result.examples_path.display().to_string());
            output.kv("Count", &result.example_count.to_string());
            output.kv("Tokenizer hash", &result.tokenizer_hash);
            output.kv("Dataset hash", &result.dataset_hash);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_args_allow_multiple_files() {
        let args = IngestArgs {
            files: vec![PathBuf::from("a.jsonl"), PathBuf::from("b.jsonl")],
            dataset_id: None,
            format: Some("jsonl".into()),
            name: Some("ds".into()),
            description: None,
        };
        assert_eq!(args.files.len(), 2);
        assert_eq!(args.format.as_deref(), Some("jsonl"));
    }

    #[test]
    fn build_args_defaults() {
        let args = BuildArgs {
            source: Some(PathBuf::from("data.jsonl")),
            git: None,
            path: None,
            branch: None,
            git_auth: "none".to_string(),
            git_token: None,
            ssh_key: None,
            output: PathBuf::from("./dataset-build"),
            tokenizer: PathBuf::from("./tokenizer.json"),
            format: None,
            text_strategy: "paragraph-pairs".to_string(),
            input_col: None,
            target_col: None,
            name: None,
            dry_run: false,
        };
        assert_eq!(args.text_strategy, "paragraph-pairs");
        assert!(!args.dry_run);
    }
}
