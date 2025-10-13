use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    pub async fn register_model(
        &self,
        name: &str,
        hash_b3: &str,
        config_hash_b3: &str,
        tokenizer_hash_b3: &str,
        tokenizer_cfg_hash_b3: &str,
        license_hash_b3: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO models (id, name, hash_b3, license_hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, metadata_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(name)
        .bind(hash_b3)
        .bind(license_hash_b3)
        .bind(config_hash_b3)
        .bind(tokenizer_hash_b3)
        .bind(tokenizer_cfg_hash_b3)
        .bind(metadata_json)
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
