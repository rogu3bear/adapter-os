//! Shared setup operations for first-run workflows.

use crate::Db;
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::warn;

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
}
