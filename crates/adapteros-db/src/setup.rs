//! Shared setup operations for first-run workflows.

use crate::training_datasets::{
    build_training_rows_from_jsonl_bytes, CreateDatasetHashInputsParams, CreateDatasetParams,
    CreateTrainingDatasetRowParams, SampleRole,
};
use crate::Db;
use anyhow::Result;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::warn;
use walkdir::WalkDir;

/// Discovered model candidate for setup flows.
#[derive(Debug, Clone)]
pub struct SetupDiscoveredModel {
    pub name: String,
    pub path: PathBuf,
    pub format: String,
    pub backend: String,
}

/// Seed options for setup model registration.
#[derive(Debug, Clone)]
pub struct SetupSeedOptions<'a> {
    pub force: bool,
    pub tenant_id: &'a str,
    pub imported_by: &'a str,
}

/// Per-model seed status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupSeedStatus {
    Seeded,
    Skipped,
    Failed,
}

/// Per-model setup seed result.
#[derive(Debug, Clone)]
pub struct SetupSeedItem {
    pub name: String,
    pub path: PathBuf,
    pub status: SetupSeedStatus,
    pub model_id: Option<String>,
    pub message: Option<String>,
}

/// Aggregate result for setup model seeding.
#[derive(Debug, Clone, Default)]
pub struct SetupSeedResult {
    pub total: usize,
    pub seeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub items: Vec<SetupSeedItem>,
}

/// Seed options for repo-backed training datasets under `training/datasets`.
#[derive(Debug, Clone)]
pub struct SetupSeedRepoDatasetsOptions<'a> {
    pub force: bool,
    pub tenant_id: &'a str,
    pub imported_by: &'a str,
}

/// Per-dataset repo seed result.
#[derive(Debug, Clone)]
pub struct SetupSeedRepoDatasetItem {
    pub name: String,
    pub manifest_path: PathBuf,
    pub status: SetupSeedStatus,
    pub dataset_id: Option<String>,
    pub message: Option<String>,
}

/// Aggregate result for repo dataset seeding.
#[derive(Debug, Clone, Default)]
pub struct SetupSeedRepoDatasetsResult {
    pub total: usize,
    pub seeded: usize,
    pub skipped: usize,
    pub failed: usize,
    pub items: Vec<SetupSeedRepoDatasetItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct RepoSeedManifest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_repo_manifest_version")]
    version: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    tier: Option<String>,
    #[serde(default)]
    entries: Vec<RepoSeedEntry>,
    #[serde(default)]
    files: Vec<RepoSeedFileSpec>,
    #[serde(default)]
    target_modules: Vec<String>,
    #[serde(default)]
    evaluation_gates: Vec<String>,
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    provenance: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct RepoSeedEntry {
    path: String,
    #[serde(default = "default_repo_entry_format")]
    format: String,
    #[serde(default = "default_repo_entry_weight")]
    weight: f32,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum RepoSeedFileSpec {
    Path(String),
    Object {
        path: String,
        #[serde(default = "default_repo_entry_format")]
        format: String,
        #[serde(default = "default_repo_entry_weight")]
        weight: f32,
        #[serde(default)]
        role: Option<String>,
    },
}

#[derive(Debug)]
struct PreparedSeedDataset {
    manifest: RepoSeedManifest,
    manifest_json: String,
    manifest_path: PathBuf,
    dataset_dir: PathBuf,
    dataset_hash: String,
    total_rows: i64,
    positive_rows: i64,
    negative_rows: i64,
    parse_errors: usize,
    dropped_rows: usize,
    files: Vec<PreparedSeedFile>,
    rows: Vec<CreateTrainingDatasetRowParams>,
}

#[derive(Debug)]
struct PreparedSeedFile {
    relative_path: String,
    absolute_path: PathBuf,
    size_bytes: i64,
    hash_b3: String,
}

fn default_repo_manifest_version() -> String {
    "1.0.0".to_string()
}

fn default_repo_entry_format() -> String {
    "jsonl".to_string()
}

fn default_repo_entry_weight() -> f32 {
    1.0
}

fn role_from_manifest_entry(role: Option<&str>, weight: f32) -> Option<SampleRole> {
    match role.map(|value| value.trim().to_ascii_lowercase()) {
        Some(role) if role == "negative" => Some(SampleRole::Negative),
        Some(role) if role == "positive" => Some(SampleRole::Positive),
        Some(role) if role == "mixed" || role == "supervised" => None,
        Some(_) => None,
        None if weight.is_sign_negative() => Some(SampleRole::Negative),
        None => None,
    }
}

fn is_jsonl_entry(path: &str, format: &str) -> bool {
    format.eq_ignore_ascii_case("jsonl") || path.to_ascii_lowercase().ends_with(".jsonl")
}

fn discover_repo_manifest_paths(root: &Path) -> Vec<PathBuf> {
    if !root.exists() {
        return Vec::new();
    }

    let mut manifests: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == "manifest.json")
        .map(|entry| entry.into_path())
        .collect();
    manifests.sort();
    manifests
}

fn manifest_entries(manifest: &RepoSeedManifest, dataset_dir: &Path) -> Vec<RepoSeedEntry> {
    if !manifest.entries.is_empty() {
        return manifest.entries.clone();
    }

    if !manifest.files.is_empty() {
        return manifest
            .files
            .iter()
            .map(|file| match file {
                RepoSeedFileSpec::Path(path) => RepoSeedEntry {
                    path: path.clone(),
                    format: default_repo_entry_format(),
                    weight: default_repo_entry_weight(),
                    role: None,
                },
                RepoSeedFileSpec::Object {
                    path,
                    format,
                    weight,
                    role,
                } => RepoSeedEntry {
                    path: path.clone(),
                    format: format.clone(),
                    weight: *weight,
                    role: role.clone(),
                },
            })
            .collect();
    }

    let mut discovered = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dataset_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if name.eq_ignore_ascii_case("manifest.json")
                    || !name.to_ascii_lowercase().ends_with(".jsonl")
                {
                    continue;
                }
                discovered.push(RepoSeedEntry {
                    path: name.to_string(),
                    format: default_repo_entry_format(),
                    weight: default_repo_entry_weight(),
                    role: None,
                });
            }
        }
    }

    discovered.sort_by(|a, b| a.path.cmp(&b.path));
    discovered
}

async fn prepare_repo_seed_dataset(
    manifest_path: &Path,
    tenant_id: &str,
) -> Result<PreparedSeedDataset> {
    let manifest_bytes = fs::read(manifest_path).await?;
    let manifest_json = String::from_utf8(manifest_bytes.clone())?;
    let mut manifest: RepoSeedManifest = serde_json::from_slice(&manifest_bytes)?;
    let dataset_dir = manifest_path
        .parent()
        .ok_or_else(|| {
            anyhow::anyhow!("manifest path missing parent: {}", manifest_path.display())
        })?
        .to_path_buf();
    if manifest.category.is_none() {
        manifest.category = dataset_dir
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(|name| name.to_string());
    }

    let entries = manifest_entries(&manifest, &dataset_dir);
    if entries.is_empty() {
        anyhow::bail!("no JSONL entries found");
    }

    let mut files = Vec::new();
    let mut rows = Vec::new();
    let mut total_rows = 0i64;
    let mut positive_rows = 0i64;
    let mut negative_rows = 0i64;
    let mut parse_errors_total = 0usize;
    let mut dropped_rows_total = 0usize;
    let mut hasher = blake3::Hasher::new();
    hasher.update(manifest_path.to_string_lossy().as_bytes());
    hasher.update(&manifest_bytes);

    for entry in entries {
        if !is_jsonl_entry(&entry.path, &entry.format) {
            continue;
        }

        let absolute_path = dataset_dir.join(&entry.path);
        let bytes = fs::read(&absolute_path).await?;
        let file_hash = blake3::hash(&bytes).to_hex().to_string();

        hasher.update(entry.path.as_bytes());
        hasher.update(&bytes);

        let (mut file_rows, parse_errors, dropped) = build_training_rows_from_jsonl_bytes(
            &entry.path,
            &bytes,
            "seed-pending",
            "seed-pending",
            Some(tenant_id),
            None,
            Some("repo_seed"),
        );
        parse_errors_total += parse_errors;
        dropped_rows_total += dropped;
        if file_rows.is_empty() {
            anyhow::bail!(
                "no usable JSONL rows in {} (parse_errors={}, dropped={})",
                absolute_path.display(),
                parse_errors,
                dropped
            );
        }

        let manifest_role = role_from_manifest_entry(entry.role.as_deref(), entry.weight);
        for row in &mut file_rows {
            row.weight *= f64::from(entry.weight);
            if let Some(role) = manifest_role {
                row.sample_role = role;
            }
            match row.sample_role {
                SampleRole::Positive => positive_rows += 1,
                SampleRole::Negative => negative_rows += 1,
            }
        }
        total_rows += i64::try_from(file_rows.len()).unwrap_or(0);
        rows.extend(file_rows);

        let size_bytes = i64::try_from(bytes.len())?;
        files.push(PreparedSeedFile {
            relative_path: entry.path,
            absolute_path,
            size_bytes,
            hash_b3: file_hash,
        });
    }

    if rows.is_empty() {
        anyhow::bail!("no seedable JSONL rows found");
    }

    Ok(PreparedSeedDataset {
        manifest,
        manifest_json,
        manifest_path: manifest_path.to_path_buf(),
        dataset_dir,
        dataset_hash: hasher.finalize().to_hex().to_string(),
        total_rows,
        positive_rows,
        negative_rows,
        parse_errors: parse_errors_total,
        dropped_rows: dropped_rows_total,
        files,
        rows,
    })
}

impl Db {
    /// Run signed migrations via the setup service wrapper.
    pub async fn setup_run_migrations(&self) -> adapteros_core::Result<()> {
        self.migrate().await
    }

    /// Discover model directories under a setup root.
    pub fn setup_discover_models(root: &Path) -> Vec<SetupDiscoveredModel> {
        let mut discovered: Vec<SetupDiscoveredModel> = adapteros_core::discover_model_dirs(root)
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "model".to_string());
                let format = adapteros_core::ModelFormat::detect_from_dir(&path);
                let backend = format.default_backend();
                SetupDiscoveredModel {
                    name,
                    path,
                    format: format.as_str().to_string(),
                    backend: backend.as_str().to_string(),
                }
            })
            .collect();

        discovered.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
        discovered
    }

    /// Seed selected models into the database.
    pub async fn setup_seed_models(
        &self,
        selected_paths: &[PathBuf],
        options: SetupSeedOptions<'_>,
    ) -> Result<SetupSeedResult> {
        if self.pool_opt().is_none() {
            anyhow::bail!("SQL pool not available for setup model seeding");
        }

        let mut summary = SetupSeedResult {
            total: selected_paths.len(),
            ..Default::default()
        };

        let mut seen = HashSet::new();

        for model_path in selected_paths {
            if !seen.insert(model_path.clone()) {
                continue;
            }

            let name = model_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "model".to_string());

            if !model_path.exists() {
                summary.failed += 1;
                summary.items.push(SetupSeedItem {
                    name,
                    path: model_path.clone(),
                    status: SetupSeedStatus::Failed,
                    model_id: None,
                    message: Some("model path does not exist".to_string()),
                });
                continue;
            }

            let Some(path_str) = model_path.to_str() else {
                warn!(path = ?model_path, "Skipping model dir with non-UTF8 path");
                summary.failed += 1;
                summary.items.push(SetupSeedItem {
                    name,
                    path: model_path.clone(),
                    status: SetupSeedStatus::Failed,
                    model_id: None,
                    message: Some("model path is not valid UTF-8".to_string()),
                });
                continue;
            };

            if !options.force {
                let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE name = ?")
                    .bind(&name)
                    .fetch_one(self.pool_result()?)
                    .await?;
                if exists > 0 {
                    summary.skipped += 1;
                    summary.items.push(SetupSeedItem {
                        name,
                        path: model_path.clone(),
                        status: SetupSeedStatus::Skipped,
                        model_id: None,
                        message: Some("already exists".to_string()),
                    });
                    continue;
                }
            }

            let format = adapteros_core::ModelFormat::detect_from_dir(model_path);
            let backend = format.default_backend();

            let seed_result = if options.force {
                self.upsert_model_from_path(
                    &name,
                    path_str,
                    format.as_str(),
                    backend.as_str(),
                    options.tenant_id,
                    options.imported_by,
                    adapteros_core::ModelImportStatus::Available,
                )
                .await
            } else {
                self.import_model_from_path(
                    &name,
                    path_str,
                    format.as_str(),
                    backend.as_str(),
                    options.tenant_id,
                    options.imported_by,
                    adapteros_core::ModelImportStatus::Available,
                )
                .await
            };

            match seed_result {
                Ok(model_id) => {
                    summary.seeded += 1;
                    summary.items.push(SetupSeedItem {
                        name,
                        path: model_path.clone(),
                        status: SetupSeedStatus::Seeded,
                        model_id: Some(model_id),
                        message: None,
                    });
                }
                Err(error) => {
                    warn!(model = %name, error = %error, "Failed to seed model");
                    summary.failed += 1;
                    summary.items.push(SetupSeedItem {
                        name,
                        path: model_path.clone(),
                        status: SetupSeedStatus::Failed,
                        model_id: None,
                        message: Some(error.to_string()),
                    });
                }
            }
        }

        Ok(summary)
    }

    /// Seed repo-backed training datasets from `training/datasets` into the active SQL/KV store.
    ///
    /// This is intentionally idempotent: existing datasets are identified by tenant + source path.
    pub async fn setup_seed_repo_datasets(
        &self,
        root: &Path,
        options: SetupSeedRepoDatasetsOptions<'_>,
    ) -> Result<SetupSeedRepoDatasetsResult> {
        if self.pool_opt().is_none() {
            anyhow::bail!("SQL pool not available for repo dataset seeding");
        }
        if self.get_tenant(options.tenant_id).await?.is_none() {
            anyhow::bail!("target tenant '{}' does not exist", options.tenant_id);
        }

        let manifest_paths = discover_repo_manifest_paths(root);
        let mut summary = SetupSeedRepoDatasetsResult {
            total: manifest_paths.len(),
            ..Default::default()
        };

        for manifest_path in manifest_paths {
            let name = manifest_path
                .parent()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "dataset".to_string());

            let source_location = manifest_path
                .parent()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|| manifest_path.to_string_lossy().to_string());

            if !options.force {
                let existing: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM training_datasets WHERE tenant_id = ? AND source_location = ? LIMIT 1",
                )
                .bind(options.tenant_id)
                .bind(&source_location)
                .fetch_optional(self.pool_result()?)
                .await?;

                if existing.is_some() {
                    summary.skipped += 1;
                    summary.items.push(SetupSeedRepoDatasetItem {
                        name,
                        manifest_path: manifest_path.clone(),
                        status: SetupSeedStatus::Skipped,
                        dataset_id: existing,
                        message: Some("already seeded".to_string()),
                    });
                    continue;
                }
            }

            match prepare_repo_seed_dataset(&manifest_path, options.tenant_id).await {
                Ok(prepared) => match self
                    .seed_prepared_repo_dataset(&prepared, &source_location, &options)
                    .await
                {
                    Ok(dataset_id) => {
                        summary.seeded += 1;
                        summary.items.push(SetupSeedRepoDatasetItem {
                            name: prepared.manifest.name.clone(),
                            manifest_path: manifest_path.clone(),
                            status: SetupSeedStatus::Seeded,
                            dataset_id: Some(dataset_id),
                            message: None,
                        });
                    }
                    Err(error) => {
                        warn!(
                            manifest_path = %manifest_path.display(),
                            error = %error,
                            "Failed to seed repo dataset"
                        );
                        summary.failed += 1;
                        summary.items.push(SetupSeedRepoDatasetItem {
                            name,
                            manifest_path: manifest_path.clone(),
                            status: SetupSeedStatus::Failed,
                            dataset_id: None,
                            message: Some(error.to_string()),
                        });
                    }
                },
                Err(error) => {
                    warn!(
                        manifest_path = %manifest_path.display(),
                        error = %error,
                        "Failed to prepare repo dataset seed"
                    );
                    summary.failed += 1;
                    summary.items.push(SetupSeedRepoDatasetItem {
                        name,
                        manifest_path: manifest_path.clone(),
                        status: SetupSeedStatus::Failed,
                        dataset_id: None,
                        message: Some(error.to_string()),
                    });
                }
            }
        }

        Ok(summary)
    }

    async fn seed_prepared_repo_dataset(
        &self,
        prepared: &PreparedSeedDataset,
        source_location: &str,
        options: &SetupSeedRepoDatasetsOptions<'_>,
    ) -> Result<String> {
        let storage_path = prepared.dataset_dir.to_string_lossy().to_string();
        let manifest_path = prepared.manifest_path.to_string_lossy().to_string();
        let metadata_json = serde_json::to_string(&json!({
            "seed_source": "repo_training_datasets",
            "imported_by": options.imported_by,
            "manifest_path": manifest_path,
            "manifest_version": prepared.manifest.version,
            "category": prepared.manifest.category,
            "scope": prepared.manifest.scope,
            "tier": prepared.manifest.tier,
            "parse_errors": prepared.parse_errors,
            "dropped_rows": prepared.dropped_rows,
            "target_modules": prepared.manifest.target_modules,
            "evaluation_gates": prepared.manifest.evaluation_gates,
            "provenance": prepared.manifest.provenance,
        }))?;

        let params = CreateDatasetParams::builder()
            .name(&prepared.manifest.name)
            .description(prepared.manifest.description.as_str())
            .format("jsonl")
            .hash_b3(&prepared.dataset_hash)
            .dataset_hash_b3(&prepared.dataset_hash)
            .storage_path(&storage_path)
            .status("uploaded")
            .tenant_id(options.tenant_id)
            .dataset_type("training")
            .purpose(
                prepared
                    .manifest
                    .intent
                    .as_deref()
                    .unwrap_or("repo training dataset"),
            )
            .source_location(source_location)
            .collection_method("pipeline")
            .ownership("system")
            .metadata_json(metadata_json)
            .build()?;

        let version_label = Some(prepared.manifest.version.as_str());
        let (dataset_id, dataset_version_id) = self
            .create_training_dataset_from_params_with_version(
                &params,
                version_label,
                &storage_path,
                &prepared.dataset_hash,
                Some(&manifest_path),
                Some(&prepared.manifest_json),
            )
            .await?;

        for file in &prepared.files {
            let mime_type = if file.relative_path.to_ascii_lowercase().ends_with(".jsonl") {
                Some("application/jsonl")
            } else {
                Some("application/octet-stream")
            };
            self.add_dataset_file(
                &dataset_id,
                &file.relative_path,
                &file.absolute_path.to_string_lossy(),
                file.size_bytes,
                &file.hash_b3,
                mime_type,
            )
            .await?;
        }

        let rows: Vec<CreateTrainingDatasetRowParams> = prepared
            .rows
            .iter()
            .map(|row| {
                let mut cloned = row.clone();
                cloned.dataset_id = dataset_id.clone();
                cloned.dataset_version_id = Some(dataset_version_id.clone());
                cloned
            })
            .collect();
        self.bulk_insert_training_dataset_rows(&rows).await?;

        self.update_dataset_validation(&dataset_id, "valid", None, None)
            .await?;
        self.update_dataset_version_structural_validation(&dataset_version_id, "valid", None)
            .await?;

        let mut hash_inputs = CreateDatasetHashInputsParams::new(
            prepared.dataset_hash.clone(),
            prepared.total_rows,
            prepared.positive_rows,
            prepared.negative_rows,
        );
        hash_inputs.dataset_id = Some(dataset_id.clone());
        hash_inputs.tenant_id = Some(options.tenant_id.to_string());
        hash_inputs.ingestion_mode = "repo_training_datasets".to_string();
        hash_inputs.generator = "setup_seed_repo_datasets".to_string();
        hash_inputs.additional_inputs_json = Some(serde_json::to_string(&json!({
            "manifest_path": manifest_path,
            "source_location": source_location,
        }))?);
        self.record_dataset_hash_inputs(&hash_inputs).await?;

        Ok(dataset_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn setup_seed_repo_datasets_imports_repo_training_manifest_idempotently() {
        let db = Db::new_in_memory().await.unwrap();
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at) VALUES ('default', 'default', datetime('now'))",
        )
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();
        let root = tempdir().unwrap();
        let dataset_dir = root.path().join("docs").join("adapteros_qa");
        std::fs::create_dir_all(&dataset_dir).unwrap();
        std::fs::write(
            dataset_dir.join("manifest.json"),
            r#"{
  "name": "adapteros_qa_v1",
  "description": "AdapterOS QA seed dataset",
  "version": "1.0.0",
  "category": "docs",
  "intent": "Teach adapterOS behavior",
  "files": ["adapteros-qa.jsonl"]
}"#,
        )
        .unwrap();
        std::fs::write(
            dataset_dir.join("adapteros-qa.jsonl"),
            "{\"prompt\":\"What is adapterOS?\",\"response\":\"A modular adapter runtime.\"}\n",
        )
        .unwrap();

        let summary = db
            .setup_seed_repo_datasets(
                root.path(),
                SetupSeedRepoDatasetsOptions {
                    force: false,
                    tenant_id: "default",
                    imported_by: "system",
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.total, 1);
        assert_eq!(summary.seeded, 1);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 0);

        let datasets = db
            .list_training_datasets_for_tenant("default", 10)
            .await
            .unwrap();
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].name, "adapteros_qa_v1");
        assert_eq!(datasets[0].validation_status, "valid");

        let row_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM training_dataset_rows WHERE dataset_id = ?")
                .bind(&datasets[0].id)
                .fetch_one(db.pool_result().unwrap())
                .await
                .unwrap();
        assert_eq!(row_count, 1);

        let rerun = db
            .setup_seed_repo_datasets(
                root.path(),
                SetupSeedRepoDatasetsOptions {
                    force: false,
                    tenant_id: "default",
                    imported_by: "system",
                },
            )
            .await
            .unwrap();
        assert_eq!(rerun.seeded, 0);
        assert_eq!(rerun.skipped, 1);
    }
}
