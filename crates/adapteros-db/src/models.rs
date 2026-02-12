use crate::constants::MODEL_COLUMNS;
use crate::new_id;
use crate::Db;
use adapteros_id::IdPrefix;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// The shared/system tenant ID that is visible to all tenants.
/// Models with this tenant_id are considered base models and accessible by any tenant.
const SYSTEM_TENANT_ID: &str = "system";

/// Registration identity hashes for a model directory.
///
/// Computes BLAKE3 hashes for model identity verification during registration:
/// - **main hash**: config.json + first .safetensors/.bin file (identity fingerprint)
/// - **config hash**: config.json alone
/// - **tokenizer hash**: tokenizer.json alone
/// - **tokenizer_config hash**: tokenizer_config.json alone
///
/// This is the *registration identity* hash chain, used to detect whether a model
/// has changed since it was registered. It intentionally hashes only config + first
/// shard for speed. For full *weight integrity* verification (all shards, sorted),
/// see `compute_model_directory_hash()` in `adapteros-lora-worker::backend_factory::model_io`.
struct ModelHashes {
    hash_b3: String,
    config_hash_b3: String,
    tokenizer_hash_b3: String,
    tokenizer_cfg_hash_b3: String,
}

fn compute_model_hashes(path: &std::path::Path, fallback_id: &str) -> ModelHashes {
    use adapteros_core::B3Hash;

    if path.exists() {
        if path.is_dir() {
            let config_path = path.join("config.json");
            let tokenizer_path = path.join("tokenizer.json");
            let tokenizer_cfg_path = path.join("tokenizer_config.json");

            // Main hash: config + first weights shard
            let mut main_hasher = blake3::Hasher::new();
            if let Ok(config_bytes) = std::fs::read(&config_path) {
                main_hasher.update(&config_bytes);
            }

            if let Some(weights_file) = walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .find(|e| {
                    e.path()
                        .extension()
                        .and_then(|s| s.to_str())
                        .map(|s| s == "safetensors" || s == "bin")
                        .unwrap_or(false)
                })
            {
                if let Ok(weights_bytes) = std::fs::read(weights_file.path()) {
                    main_hasher.update(&weights_bytes);
                }
            }

            let main_hash = B3Hash::from_bytes(*main_hasher.finalize().as_bytes());

            let config_hash = if config_path.exists() {
                B3Hash::hash_file(&config_path)
                    .unwrap_or_else(|_| B3Hash::hash(config_path.to_string_lossy().as_bytes()))
            } else {
                B3Hash::hash(b"missing-config")
            };

            let tokenizer_hash = if tokenizer_path.exists() {
                B3Hash::hash_file(&tokenizer_path)
                    .unwrap_or_else(|_| B3Hash::hash(tokenizer_path.to_string_lossy().as_bytes()))
            } else {
                B3Hash::hash(b"missing-tokenizer")
            };

            let tokenizer_cfg_hash = if tokenizer_cfg_path.exists() {
                B3Hash::hash_file(&tokenizer_cfg_path).unwrap_or_else(|_| {
                    B3Hash::hash(tokenizer_cfg_path.to_string_lossy().as_bytes())
                })
            } else {
                B3Hash::hash(b"missing-tokenizer-config")
            };

            ModelHashes {
                hash_b3: main_hash.to_hex(),
                config_hash_b3: config_hash.to_hex(),
                tokenizer_hash_b3: tokenizer_hash.to_hex(),
                tokenizer_cfg_hash_b3: tokenizer_cfg_hash.to_hex(),
            }
        } else {
            // Single file
            let file_hash = B3Hash::hash_file(path)
                .unwrap_or_else(|_| B3Hash::hash(path.to_string_lossy().as_bytes()));
            let hex = file_hash.to_hex();
            ModelHashes {
                hash_b3: hex.clone(),
                config_hash_b3: hex.clone(),
                tokenizer_hash_b3: hex.clone(),
                tokenizer_cfg_hash_b3: hex,
            }
        }
    } else {
        ModelHashes {
            hash_b3: format!("missing_hash_{}", fallback_id),
            config_hash_b3: format!("missing_config_{}", fallback_id),
            tokenizer_hash_b3: format!("missing_tokenizer_{}", fallback_id),
            tokenizer_cfg_hash_b3: format!("missing_tokenizer_cfg_{}", fallback_id),
        }
    }
}

/// Normalize a model path for consistent storage.
///
/// Strips leading `./` prefix to ensure paths are stored consistently.
/// Does not canonicalize to absolute paths as that would be machine-specific.
fn normalize_model_path(path: &str) -> String {
    path.strip_prefix("./").unwrap_or(path).to_string()
}

/// Builder for creating model registration parameters
#[derive(Debug, Default)]
pub struct ModelRegistrationBuilder {
    name: Option<String>,
    hash_b3: Option<String>,
    config_hash_b3: Option<String>,
    tokenizer_hash_b3: Option<String>,
    tokenizer_cfg_hash_b3: Option<String>,
    license_hash_b3: Option<String>,
    metadata_json: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_populates_required_fields() {
        let params = ModelRegistrationBuilder::new()
            .name("model")
            .hash_b3("hash")
            .config_hash_b3("config-hash")
            .tokenizer_hash_b3("tokenizer-hash")
            .tokenizer_cfg_hash_b3("tokenizer-cfg-hash")
            .license_hash_b3(Some("license-hash"))
            .metadata_json(Some(r#"{"size": "7b"}"#))
            .build()
            .expect("builder succeeds");

        assert_eq!(params.name, "model");
        assert_eq!(params.hash_b3, "hash");
        assert_eq!(params.config_hash_b3, "config-hash");
        assert_eq!(params.tokenizer_hash_b3, "tokenizer-hash");
        assert_eq!(params.tokenizer_cfg_hash_b3, "tokenizer-cfg-hash");
        assert_eq!(params.license_hash_b3.as_deref(), Some("license-hash"));
        assert_eq!(params.metadata_json.as_deref(), Some(r#"{"size": "7b"}"#));
    }

    #[test]
    fn builder_requires_name() {
        let err = ModelRegistrationBuilder::new()
            .hash_b3("hash")
            .config_hash_b3("config")
            .tokenizer_hash_b3("tokenizer")
            .tokenizer_cfg_hash_b3("tokenizer-cfg")
            .build()
            .expect_err("missing name should error");

        assert!(err.to_string().contains("name is required"));
    }
}

/// Parameters for model registration
#[derive(Debug, Clone)]
pub struct ModelRegistrationParams {
    pub name: String,
    pub hash_b3: String,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    pub tokenizer_cfg_hash_b3: String,
    pub license_hash_b3: Option<String>,
    pub metadata_json: Option<String>,
}

impl ModelRegistrationBuilder {
    /// Create a new model registration builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the model name (required)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the model B3 hash (required)
    pub fn hash_b3(mut self, hash_b3: impl Into<String>) -> Self {
        self.hash_b3 = Some(hash_b3.into());
        self
    }

    /// Set the config B3 hash (required)
    pub fn config_hash_b3(mut self, config_hash_b3: impl Into<String>) -> Self {
        self.config_hash_b3 = Some(config_hash_b3.into());
        self
    }

    /// Set the tokenizer B3 hash (required)
    pub fn tokenizer_hash_b3(mut self, tokenizer_hash_b3: impl Into<String>) -> Self {
        self.tokenizer_hash_b3 = Some(tokenizer_hash_b3.into());
        self
    }

    /// Set the tokenizer config B3 hash (required)
    pub fn tokenizer_cfg_hash_b3(mut self, tokenizer_cfg_hash_b3: impl Into<String>) -> Self {
        self.tokenizer_cfg_hash_b3 = Some(tokenizer_cfg_hash_b3.into());
        self
    }

    /// Set the license B3 hash (optional)
    pub fn license_hash_b3(mut self, license_hash_b3: Option<impl Into<String>>) -> Self {
        self.license_hash_b3 = license_hash_b3.map(|s| s.into());
        self
    }

    /// Set the metadata JSON (optional)
    pub fn metadata_json(mut self, metadata_json: Option<impl Into<String>>) -> Self {
        self.metadata_json = metadata_json.map(|s| s.into());
        self
    }

    /// Build the model registration parameters
    pub fn build(self) -> Result<ModelRegistrationParams> {
        Ok(ModelRegistrationParams {
            name: self.name.ok_or_else(|| anyhow!("name is required"))?,
            hash_b3: self.hash_b3.ok_or_else(|| anyhow!("hash_b3 is required"))?,
            config_hash_b3: self
                .config_hash_b3
                .ok_or_else(|| anyhow!("config_hash_b3 is required"))?,
            tokenizer_hash_b3: self
                .tokenizer_hash_b3
                .ok_or_else(|| anyhow!("tokenizer_hash_b3 is required"))?,
            tokenizer_cfg_hash_b3: self
                .tokenizer_cfg_hash_b3
                .ok_or_else(|| anyhow!("tokenizer_cfg_hash_b3 is required"))?,
            license_hash_b3: self.license_hash_b3,
            metadata_json: self.metadata_json,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub license_hash_b3: Option<String>,
    pub config_hash_b3: String,
    pub tokenizer_hash_b3: String,
    pub tokenizer_cfg_hash_b3: String,
    pub metadata_json: Option<String>,
    pub created_at: String,
    #[sqlx(default)]
    pub model_type: Option<String>,
    #[sqlx(default)]
    pub model_path: Option<String>,
    #[sqlx(default)]
    pub config: Option<String>,
    #[sqlx(default)]
    pub routing_bias: Option<f64>,
    #[sqlx(default)]
    pub status: Option<String>,
    #[sqlx(default)]
    pub tenant_id: Option<String>,
    #[sqlx(default)]
    pub updated_at: Option<String>,
    #[sqlx(default)]
    pub adapter_path: Option<String>,
    #[sqlx(default)]
    pub backend: Option<String>,
    #[sqlx(default)]
    pub quantization: Option<String>,
    #[sqlx(default)]
    pub last_error: Option<String>,
    #[sqlx(default)]
    pub size_bytes: Option<i64>,
    #[sqlx(default)]
    pub format: Option<String>,
    #[sqlx(default)]
    pub capabilities: Option<String>,
    #[sqlx(default)]
    pub import_status: Option<String>,
    #[sqlx(default)]
    pub import_error: Option<String>,
    #[sqlx(default)]
    pub imported_at: Option<String>,
    #[sqlx(default)]
    pub imported_by: Option<String>,
}

impl Model {
    /// Check if this model is visible to the specified tenant.
    ///
    /// A model is visible to a tenant if:
    /// - The model has no tenant_id (global/NULL) - legacy shared models
    /// - The model belongs to the 'system' tenant - base models shared across tenants
    /// - The model belongs to the requesting tenant - tenant-specific models
    pub fn is_visible_to_tenant(&self, tenant_id: &str) -> bool {
        match self.tenant_id.as_deref() {
            None => true,                                    // Global models (NULL) visible to all
            Some(SYSTEM_TENANT_ID) => true,                  // System models visible to all
            Some(model_tenant) => model_tenant == tenant_id, // Tenant-scoped models
        }
    }
}

pub type Manifest = adapteros_types::Manifest;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Plan {
    pub id: String,
    pub tenant_id: String,
    pub plan_id_b3: String,
    pub manifest_hash_b3: String,
    pub kernel_hashes_json: String,
    pub metallib_hash_b3: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CpPointer {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub plan_id: String,
    pub active: i32,
    pub created_at: String,
    pub activated_at: Option<String>,
    pub signing_public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BundleSignature {
    pub id: String,
    pub bundle_hash_b3: String,
    pub cpid: String,
    pub signature_hex: String,
    pub public_key_hex: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BaseModelStatus {
    pub id: String,
    pub tenant_id: String,
    pub model_id: String,
    pub status: String,
    pub loaded_at: Option<String>,
    pub unloaded_at: Option<String>,
    pub error_message: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelWithStats {
    #[serde(flatten)]
    pub model: Model,
    pub adapter_count: i64,
    pub training_job_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Worker {
    pub id: String,
    pub tenant_id: String,
    pub node_id: String,
    pub plan_id: String,
    pub uds_path: String,
    pub pid: Option<i32>,
    pub status: String,
    pub started_at: String,
    pub last_seen_at: Option<String>,
    pub manifest_hash_b3: Option<String>,
    pub backend: Option<String>,
    pub model_hash_b3: Option<String>,
    pub capabilities_json: Option<String>,
    #[sqlx(default)]
    pub tokenizer_hash_b3: Option<String>,
    #[sqlx(default)]
    pub tokenizer_vocab_size: Option<i64>,
}

impl Db {
    /// Register a new model
    ///
    /// Use [`ModelRegistrationBuilder`] to construct model parameters:
    /// ```no_run
    /// use adapteros_db::models::ModelRegistrationBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = ModelRegistrationBuilder::new()
    ///     .name("my-model")
    ///     .hash_b3("model-hash-123")
    ///     .config_hash_b3("config-hash-456")
    ///     .tokenizer_hash_b3("tokenizer-hash-789")
    ///     .tokenizer_cfg_hash_b3("tokenizer-cfg-hash-101")
    ///     .license_hash_b3(Some("license-hash-202"))
    ///     .metadata_json(Some(r#"{"architecture": "transformer"}"#))
    ///     .build()
    ///     .expect("required fields");
    /// db.register_model(params).await.expect("registration succeeds");
    /// # }
    /// ```
    pub async fn register_model(&self, params: ModelRegistrationParams) -> Result<String> {
        let id = new_id(IdPrefix::Mdl);
        sqlx::query(
            "INSERT INTO models (id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(&params.name)
        .bind(&params.hash_b3)
        .bind(&params.license_hash_b3)
        .bind(&params.config_hash_b3)
        .bind(&params.tokenizer_hash_b3)
        .bind(&params.tokenizer_cfg_hash_b3)
        .bind(&params.metadata_json)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_model(&self, id: &str) -> Result<Option<Model>> {
        let query = format!("SELECT {} FROM models WHERE id = ?", MODEL_COLUMNS);
        let model = sqlx::query_as::<_, Model>(&query)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(model)
    }

    /// PRD-ART-01: Get model by name for base model compatibility checking during import
    ///
    /// Looks up a model by its name (e.g., "qwen2.5-7b") to validate that
    /// an imported adapter is compatible with an available base model.
    pub async fn get_model_by_name(&self, name: &str) -> Result<Option<Model>> {
        let query = format!(
            "SELECT {} FROM models WHERE name = ? AND import_status = 'available'",
            MODEL_COLUMNS
        );
        let model = sqlx::query_as::<_, Model>(&query)
            .bind(name)
            .fetch_optional(self.pool())
            .await?;
        Ok(model)
    }

    /// Get model by its hash (model_hash_b3). Used to lookup model from worker's model_hash.
    pub async fn get_model_by_hash(&self, hash_b3: &str) -> Result<Option<Model>> {
        let query = format!("SELECT {} FROM models WHERE hash_b3 = ?", MODEL_COLUMNS);
        let model = sqlx::query_as::<_, Model>(&query)
            .bind(hash_b3)
            .fetch_optional(self.pool())
            .await?;
        Ok(model)
    }

    /// Get a model scoped to a tenant.
    ///
    /// Returns the model if it's visible to the tenant (NULL, 'system', or matching tenant_id).
    /// Returns None if the model exists but belongs to a different tenant.
    pub async fn get_model_for_tenant(&self, tenant_id: &str, id: &str) -> Result<Option<Model>> {
        let model = self.get_model(id).await?;
        Ok(match model {
            Some(m) if m.is_visible_to_tenant(tenant_id) => Some(m),
            _ => None,
        })
    }

    /// Get a model by name scoped to a tenant.
    ///
    /// Returns the model if it's visible to the tenant (NULL, 'system', or matching tenant_id).
    /// Returns None if the model exists but belongs to a different tenant.
    pub async fn get_model_by_name_for_tenant(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<Model>> {
        let model = self.get_model_by_name(name).await?;
        Ok(match model {
            Some(m) if m.is_visible_to_tenant(tenant_id) => Some(m),
            _ => None,
        })
    }

    pub async fn list_models(&self, tenant_id: &str) -> Result<Vec<Model>> {
        // Include models from: user's tenant, NULL tenant (legacy), or 'system' tenant (shared)
        let query = format!(
            "SELECT {} FROM models \
             WHERE tenant_id = ? OR tenant_id IS NULL OR tenant_id = 'system' \
             ORDER BY created_at DESC",
            MODEL_COLUMNS
        );
        let models = sqlx::query_as::<_, Model>(&query)
            .bind(tenant_id)
            .fetch_all(self.pool())
            .await?;
        Ok(models)
    }

    /// Import a model from a path on disk
    pub async fn import_model_from_path(
        &self,
        name: &str,
        model_path: &str,
        format: &str,
        backend: &str,
        tenant_id: &str,
        imported_by: &str,
        status: adapteros_core::ModelImportStatus,
    ) -> Result<String> {
        use std::path::Path;

        let id = new_id(IdPrefix::Mdl);
        let now = chrono::Utc::now().to_rfc3339();
        // Normalize path for consistent storage
        let normalized_path = normalize_model_path(model_path);
        let path = Path::new(&normalized_path);

        // Compute size based on whether path is a file or directory
        let size_bytes = if path.exists() {
            if path.is_dir() {
                // Sum all file sizes in directory
                walkdir::WalkDir::new(path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len() as i64)
                    .sum::<i64>()
                    .into()
            } else {
                // Single file
                std::fs::metadata(path).ok().map(|m| m.len() as i64)
            }
        } else {
            None
        };

        let hashes = compute_model_hashes(path, &id);

        sqlx::query(
            "INSERT INTO models
             (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
              model_path, format, backend, tenant_id, import_status, imported_at, imported_by,
              size_bytes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&hashes.hash_b3)
        .bind(&hashes.config_hash_b3)
        .bind(&hashes.tokenizer_hash_b3)
        .bind(&hashes.tokenizer_cfg_hash_b3)
        .bind(&normalized_path)
        .bind(format)
        .bind(backend)
        .bind(tenant_id)
        .bind(status.as_str())
        .bind(&now)
        .bind(imported_by)
        .bind(size_bytes)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Import or update a model from a path on disk (upsert pattern).
    ///
    /// If a model with the same name already exists, updates it instead of failing.
    /// Returns the model ID (existing or new).
    pub async fn upsert_model_from_path(
        &self,
        name: &str,
        model_path: &str,
        format: &str,
        backend: &str,
        tenant_id: &str,
        imported_by: &str,
        status: adapteros_core::ModelImportStatus,
    ) -> Result<String> {
        use std::path::Path;

        // Check if model with this name already exists
        let existing = self.get_model_by_name(name).await?;
        let id = existing
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| new_id(IdPrefix::Mdl));

        let now = chrono::Utc::now().to_rfc3339();
        // Normalize path for consistent storage
        let normalized_path = normalize_model_path(model_path);
        let path = Path::new(&normalized_path);

        // Compute size based on whether path is a file or directory
        let size_bytes = if path.exists() {
            if path.is_dir() {
                walkdir::WalkDir::new(path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len() as i64)
                    .sum::<i64>()
                    .into()
            } else {
                std::fs::metadata(path).ok().map(|m| m.len() as i64)
            }
        } else {
            None
        };

        let hashes = compute_model_hashes(path, &id);

        // Use ON CONFLICT to handle both insert and update cases
        sqlx::query(
            "INSERT INTO models
             (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
              model_path, format, backend, tenant_id, import_status, imported_at, imported_by,
              size_bytes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
                hash_b3 = excluded.hash_b3,
                config_hash_b3 = excluded.config_hash_b3,
                tokenizer_hash_b3 = excluded.tokenizer_hash_b3,
                tokenizer_cfg_hash_b3 = excluded.tokenizer_cfg_hash_b3,
                model_path = excluded.model_path,
                format = excluded.format,
                backend = excluded.backend,
                import_status = excluded.import_status,
                imported_at = excluded.imported_at,
                imported_by = excluded.imported_by,
                size_bytes = excluded.size_bytes,
                updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(name)
        .bind(&hashes.hash_b3)
        .bind(&hashes.config_hash_b3)
        .bind(&hashes.tokenizer_hash_b3)
        .bind(&hashes.tokenizer_cfg_hash_b3)
        .bind(&normalized_path)
        .bind(format)
        .bind(backend)
        .bind(tenant_id)
        .bind(status.as_str())
        .bind(&now)
        .bind(imported_by)
        .bind(size_bytes)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;

        Ok(id)
    }

    /// Update model import status
    pub async fn update_model_import_status(
        &self,
        model_id: &str,
        status: adapteros_core::ModelImportStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "UPDATE models
             SET import_status = ?, import_error = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(status.as_str())
        .bind(error_message)
        .bind(&now)
        .bind(model_id)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// Update model path after download
    ///
    /// Updates the file path for a model after it has been downloaded.
    /// This is used by the download handler to set the correct path.
    pub async fn update_model_path(&self, model_id: &str, path: &str) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        let result = sqlx::query(
            "UPDATE models
             SET model_path = ?, updated_at = ?
             WHERE id = ? OR name = ?",
        )
        .bind(path)
        .bind(&now)
        .bind(model_id)
        .bind(model_id)
        .execute(self.pool())
        .await?;

        if result.rows_affected() == 0 {
            return Err(anyhow::anyhow!("Model not found: {}", model_id));
        }

        Ok(())
    }

    /// Get count of adapters using a model
    ///
    /// Note: Current schema doesn't have a direct base_model_id foreign key in adapters table.
    /// This method searches by model path/name in adapter metadata fields.
    /// For accurate tracking, consider adding a base_model_id column to adapters table.
    pub async fn count_adapters_for_model(&self, model_id: &str) -> Result<i64> {
        // First, get the model to find its path/name
        let model = self.get_model(model_id).await?;

        if let Some(model) = model {
            // Search for adapters that might reference this model
            // Check in metadata_json, adapter_path, or other fields that might contain model reference
            let count = if let Some(model_path) = &model.model_path {
                // Try to find adapters with matching path patterns
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM adapters
                     WHERE (aos_file_path LIKE ? OR aos_file_path LIKE ?)
                        OR (metadata_json LIKE ? OR metadata_json LIKE ?)",
                )
                .bind(format!("%{}%", model_path))
                .bind(format!("%{}%", model.name))
                .bind(format!("%{}%", model_path))
                .bind(format!("%{}%", model.name))
                .fetch_one(self.pool())
                .await?
            } else {
                // Search by model name in metadata
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM adapters
                     WHERE metadata_json LIKE ?",
                )
                .bind(format!("%{}%", model.name))
                .fetch_one(self.pool())
                .await?
            };

            Ok(count)
        } else {
            Ok(0)
        }
    }

    /// Get count of training jobs for a model
    ///
    /// Note: Current schema doesn't have a direct base_model_id foreign key in training tables.
    /// This method searches by model name/path in training configuration JSON.
    /// For accurate tracking, consider adding a base_model_id column to training tables.
    pub async fn count_training_jobs_for_model(&self, model_id: &str) -> Result<i64> {
        // First, get the model to find its path/name
        let model = self.get_model(model_id).await?;

        if let Some(model) = model {
            // Search for training jobs that reference this model in their config
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM repository_training_jobs
                 WHERE training_config_json LIKE ? OR training_config_json LIKE ?",
            )
            .bind(format!("%{}%", model.name))
            .bind(if let Some(path) = &model.model_path {
                format!("%{}%", path)
            } else {
                String::new()
            })
            .fetch_one(self.pool())
            .await?;

            Ok(count)
        } else {
            Ok(0)
        }
    }

    /// List models with statistics
    pub async fn list_models_with_stats(&self, tenant_id: &str) -> Result<Vec<ModelWithStats>> {
        let models = self.list_models(tenant_id).await?;
        let mut result = Vec::new();

        for model in models {
            let adapter_count = self.count_adapters_for_model(&model.id).await?;
            let training_job_count = self.count_training_jobs_for_model(&model.id).await?;

            result.push(ModelWithStats {
                model,
                adapter_count,
                training_job_count,
            });
        }

        Ok(result)
    }

    /// Update base model status
    pub async fn update_base_model_status(
        &self,
        tenant_id: &str,
        model_id: &str,
        status: &str,
        error_message: Option<&str>,
        memory_usage_mb: Option<i32>,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();

        // Normalize legacy statuses into canonical model load states (aligned with DB CHECK)
        let normalized_status = match status {
            "loaded" | "ready" => "loaded",
            "unloaded" | "no-model" | "none" => "unloaded",
            "loading" => "loading",
            "unloading" => "unloading",
            "checking" => "loading",
            "error" => "error",
            other => {
                tracing::warn!(
                    status = %other,
                    "Unknown base model status; coercing to unloaded"
                );
                "unloaded"
            }
        };

        // Check if status record exists
        let existing = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM base_model_status WHERE tenant_id = ? AND model_id = ?",
        )
        .bind(tenant_id)
        .bind(model_id)
        .fetch_one(self.pool())
        .await?;

        if existing > 0 {
            // Update existing record
            sqlx::query(
                "UPDATE base_model_status SET status = ?, error_message = ?, memory_usage_mb = ?, updated_at = ? WHERE tenant_id = ? AND model_id = ?"
            )
            .bind(normalized_status)
            .bind(error_message)
            .bind(memory_usage_mb)
            .bind(&now)
            .bind(tenant_id)
            .bind(model_id)
            .execute(self.pool())
            .await?;
        } else {
            // Insert new record
            let id = new_id(IdPrefix::Mdl);
            sqlx::query(
                "INSERT INTO base_model_status (id, tenant_id, model_id, status, error_message, memory_usage_mb, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(model_id)
            .bind(normalized_status)
            .bind(error_message)
            .bind(memory_usage_mb)
            .bind(&now)
            .bind(&now)
            .execute(self.pool())
            .await?;
        }

        // Update loaded_at/unloaded_at timestamps based on normalized status
        match normalized_status {
            "loaded" => {
                sqlx::query(
                    "UPDATE base_model_status SET loaded_at = ? WHERE tenant_id = ? AND model_id = ?"
                )
                .bind(&now)
                .bind(tenant_id)
                .bind(model_id)
                .execute(self.pool())
                .await?;
            }
            "unloaded" => {
                sqlx::query(
                    "UPDATE base_model_status SET unloaded_at = ? WHERE tenant_id = ? AND model_id = ?"
                )
                .bind(&now)
                .bind(tenant_id)
                .bind(model_id)
                .execute(self.pool())
                .await?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Get base model status for tenant
    pub async fn get_base_model_status(&self, tenant_id: &str) -> Result<Option<BaseModelStatus>> {
        let status = sqlx::query_as::<_, BaseModelStatus>(
            "SELECT id, tenant_id, model_id, status, loaded_at, unloaded_at, error_message, memory_usage_mb, created_at, updated_at FROM base_model_status WHERE tenant_id = ? ORDER BY updated_at DESC LIMIT 1"
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(status)
    }

    /// Get base model status for a specific model (tenant-scoped).
    pub async fn get_base_model_status_for_model(
        &self,
        tenant_id: &str,
        model_id: &str,
    ) -> Result<Option<BaseModelStatus>> {
        let status = sqlx::query_as::<_, BaseModelStatus>(
            "SELECT id, tenant_id, model_id, status, loaded_at, unloaded_at, error_message, memory_usage_mb, created_at, updated_at FROM base_model_status WHERE tenant_id = ? AND model_id = ? ORDER BY updated_at DESC LIMIT 1"
        )
        .bind(tenant_id)
        .bind(model_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(status)
    }

    /// List all base model statuses
    pub async fn list_base_model_statuses(&self) -> Result<Vec<BaseModelStatus>> {
        let statuses = sqlx::query_as::<_, BaseModelStatus>(
            "SELECT id, tenant_id, model_id, status, loaded_at, unloaded_at, error_message, memory_usage_mb, created_at, updated_at FROM base_model_status ORDER BY updated_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(statuses)
    }
}
