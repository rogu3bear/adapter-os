use crate::Db;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Manifest {
    pub id: String,
    pub tenant_id: String,
    pub hash_b3: String,
    pub body_json: String,
    pub created_at: String,
}

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
        let id = Uuid::now_v7().to_string();
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
        let model = sqlx::query_as::<_, Model>(
            "SELECT id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json, created_at FROM models WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(model)
    }

    pub async fn list_models(&self) -> Result<Vec<Model>> {
        let models = sqlx::query_as::<_, Model>(
            "SELECT id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json, created_at FROM models ORDER BY created_at DESC"
        )
        .fetch_all(self.pool())
        .await?;
        Ok(models)
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
            .bind(status)
            .bind(error_message)
            .bind(memory_usage_mb)
            .bind(&now)
            .bind(tenant_id)
            .bind(model_id)
            .execute(self.pool())
            .await?;
        } else {
            // Insert new record
            let id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO base_model_status (id, tenant_id, model_id, status, error_message, memory_usage_mb, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(model_id)
            .bind(status)
            .bind(error_message)
            .bind(memory_usage_mb)
            .bind(&now)
            .bind(&now)
            .execute(self.pool())
            .await?;
        }

        // Update loaded_at/unloaded_at timestamps based on status
        match status {
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
