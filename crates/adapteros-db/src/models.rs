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
}
