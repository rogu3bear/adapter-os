//! Model hash verification and collision detection
//!
//! This module implements model registration with hash verification:
//! - Model registration with multiple component hashes
//! - Hash collision detection across different models
//! - Hash verification for integrity checking

use crate::Db;
use adapteros_core::{AosError, B3Hash, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Trait for model hash verification operations
#[async_trait]
pub trait ModelHashVerifier {
    /// Register a model with hash collision detection
    async fn register_model(&self, model: ModelRecord) -> Result<String>;

    /// Verify all model component hashes match registered values
    async fn verify_model_hashes(&self, name: &str, hashes: &ModelHashes) -> Result<bool>;

    /// Get a model by name
    async fn get_model(&self, name: &str) -> Result<Option<ModelRecord>>;

    /// List all registered models
    async fn list_models(&self) -> Result<Vec<ModelRecord>>;
}

#[async_trait]
impl ModelHashVerifier for Db {
    async fn register_model(&self, model: ModelRecord) -> Result<String> {
        register_model(self, model).await
    }

    async fn verify_model_hashes(&self, name: &str, hashes: &ModelHashes) -> Result<bool> {
        verify_model_hashes(self, name, hashes).await
    }

    async fn get_model(&self, name: &str) -> Result<Option<ModelRecord>> {
        get_model(self, name).await
    }

    async fn list_models(&self) -> Result<Vec<ModelRecord>> {
        list_models(self).await
    }
}

/// Model registration record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecord {
    pub name: String,
    pub config_hash: B3Hash,
    pub tokenizer_hash: B3Hash,
    pub tokenizer_cfg_hash: B3Hash,
    pub weights_hash: B3Hash,
    pub license_hash: B3Hash,
    pub license_text: String,
    pub model_card_hash: Option<B3Hash>,
    pub created_at: i64,
}

/// Parameters required to construct a model record
#[derive(Debug, Clone)]
pub struct ModelRecordInput {
    pub name: String,
    pub config_hash: B3Hash,
    pub tokenizer_hash: B3Hash,
    pub tokenizer_cfg_hash: B3Hash,
    pub weights_hash: B3Hash,
    pub license_hash: B3Hash,
    pub license_text: String,
    pub model_card_hash: Option<B3Hash>,
}

/// Model hashes for verification
#[derive(Debug, Clone)]
pub struct ModelHashes {
    pub config_hash: B3Hash,
    pub tokenizer_hash: B3Hash,
    pub tokenizer_cfg_hash: B3Hash,
    pub weights_hash: B3Hash,
    pub license_hash: B3Hash,
}

impl ModelRecord {
    /// Create from input parameters
    pub fn from_input(input: ModelRecordInput) -> Self {
        Self {
            name: input.name,
            config_hash: input.config_hash,
            tokenizer_hash: input.tokenizer_hash,
            tokenizer_cfg_hash: input.tokenizer_cfg_hash,
            weights_hash: input.weights_hash,
            license_hash: input.license_hash,
            license_text: input.license_text,
            model_card_hash: input.model_card_hash,
            created_at: chrono::Utc::now().timestamp(),
        }
    }
}

/// Register a new model with hash collision detection
pub async fn register_model(db: &Db, model: ModelRecord) -> Result<String> {
    // Check for hash collisions
    check_hash_collisions(db, &model).await?;

    // Insert model
    sqlx::query(
        r#"
        INSERT INTO base_models (
            name, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
            weights_hash_b3, license_hash_b3, license_text, model_card_hash_b3, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(&model.name)
    .bind(model.config_hash.to_hex())
    .bind(model.tokenizer_hash.to_hex())
    .bind(model.tokenizer_cfg_hash.to_hex())
    .bind(model.weights_hash.to_hex())
    .bind(model.license_hash.to_hex())
    .bind(&model.license_text)
    .bind(model.model_card_hash.as_ref().map(|h| h.to_hex()))
    .bind(model.created_at)
    .execute(db.pool_result()?)
    .await
    .map_err(|e| AosError::Registry(format!("Failed to register model: {}", e)))?;

    Ok(model.name)
}

/// Get model by name
pub async fn get_model(db: &Db, name: &str) -> Result<Option<ModelRecord>> {
    let row = sqlx::query_as::<_, ModelRow>(
        r#"
        SELECT name, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
               weights_hash_b3, license_hash_b3, license_text, model_card_hash_b3, created_at
        FROM base_models WHERE name = ?1
        "#,
    )
    .bind(name)
    .fetch_optional(db.pool_result()?)
    .await
    .map_err(|e| AosError::Registry(format!("Failed to get model: {}", e)))?;

    row.map(ModelRecord::try_from).transpose()
}

/// List all registered models
pub async fn list_models(db: &Db) -> Result<Vec<ModelRecord>> {
    let rows = sqlx::query_as::<_, ModelRow>(
        r#"
        SELECT name, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
               weights_hash_b3, license_hash_b3, license_text, model_card_hash_b3, created_at
        FROM base_models ORDER BY created_at DESC
        "#,
    )
    .fetch_all(db.pool_result()?)
    .await
    .map_err(|e| AosError::Registry(format!("Failed to list models: {}", e)))?;

    rows.into_iter().map(ModelRecord::try_from).collect()
}

/// Verify model hashes match registered values
pub async fn verify_model_hashes(db: &Db, name: &str, hashes: &ModelHashes) -> Result<bool> {
    let model = match get_model(db, name).await? {
        Some(m) => m,
        None => return Ok(false),
    };

    Ok(model.config_hash == hashes.config_hash
        && model.tokenizer_hash == hashes.tokenizer_hash
        && model.tokenizer_cfg_hash == hashes.tokenizer_cfg_hash
        && model.weights_hash == hashes.weights_hash
        && model.license_hash == hashes.license_hash)
}

/// Check for hash collisions with existing models
async fn check_hash_collisions(db: &Db, model: &ModelRecord) -> Result<()> {
    let hash_checks = vec![
        ("config", model.config_hash.to_hex()),
        ("tokenizer", model.tokenizer_hash.to_hex()),
        ("tokenizer_cfg", model.tokenizer_cfg_hash.to_hex()),
        ("weights", model.weights_hash.to_hex()),
        ("license", model.license_hash.to_hex()),
    ];

    for (hash_type, hash_value) in hash_checks {
        let column = format!("{}_hash_b3", hash_type);
        let query = format!(
            "SELECT name FROM base_models WHERE {} = ?1 AND name != ?2",
            column
        );

        let existing: Option<String> = sqlx::query_scalar(&query)
            .bind(&hash_value)
            .bind(&model.name)
            .fetch_optional(db.pool_result()?)
            .await
            .map_err(|e| AosError::Registry(format!("Hash collision check failed: {}", e)))?;

        if let Some(existing_name) = existing {
            return Err(AosError::Registry(format!(
                "Hash collision: {} hash {} already used by model '{}'",
                hash_type, hash_value, existing_name
            )));
        }
    }

    Ok(())
}

/// Compute file hash for verification
pub fn compute_file_hash<P: AsRef<Path>>(path: P) -> Result<B3Hash> {
    let path = path.as_ref();
    let contents = std::fs::read(path).map_err(|e| {
        AosError::Registry(format!("Failed to read file {}: {}", path.display(), e))
    })?;
    Ok(B3Hash::hash(&contents))
}

/// Read license text from file
pub fn read_license_text<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|e| {
        AosError::Registry(format!(
            "Failed to read license file {}: {}",
            path.display(),
            e
        ))
    })
}

#[derive(sqlx::FromRow)]
struct ModelRow {
    name: String,
    config_hash_b3: Option<String>,
    tokenizer_hash_b3: Option<String>,
    tokenizer_cfg_hash_b3: Option<String>,
    weights_hash_b3: Option<String>,
    license_hash_b3: Option<String>,
    license_text: Option<String>,
    model_card_hash_b3: Option<String>,
    created_at: Option<i64>,
}

impl TryFrom<ModelRow> for ModelRecord {
    type Error = AosError;

    fn try_from(row: ModelRow) -> Result<Self> {
        let parse_hash = |s: Option<String>, name: &str| -> Result<B3Hash> {
            let s = s.ok_or_else(|| AosError::Registry(format!("Missing {} hash", name)))?;
            B3Hash::from_hex(&s)
                .map_err(|e| AosError::Registry(format!("Invalid {} hash: {}", name, e)))
        };

        Ok(Self {
            name: row.name,
            config_hash: parse_hash(row.config_hash_b3, "config")?,
            tokenizer_hash: parse_hash(row.tokenizer_hash_b3, "tokenizer")?,
            tokenizer_cfg_hash: parse_hash(row.tokenizer_cfg_hash_b3, "tokenizer_cfg")?,
            weights_hash: parse_hash(row.weights_hash_b3, "weights")?,
            license_hash: parse_hash(row.license_hash_b3, "license")?,
            license_text: row.license_text.unwrap_or_default(),
            model_card_hash: row
                .model_card_hash_b3
                .map(|s| B3Hash::from_hex(&s))
                .transpose()
                .map_err(|e| AosError::Registry(format!("Invalid model_card hash: {}", e)))?,
            created_at: row.created_at.unwrap_or(0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_model_lifecycle() {
        let db = Db::new_in_memory().await.unwrap();

        let model = ModelRecord::from_input(ModelRecordInput {
            name: "test-model".to_string(),
            config_hash: B3Hash::hash(b"config"),
            tokenizer_hash: B3Hash::hash(b"tokenizer"),
            tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
            weights_hash: B3Hash::hash(b"weights"),
            license_hash: B3Hash::hash(b"license"),
            license_text: "MIT License".to_string(),
            model_card_hash: Some(B3Hash::hash(b"model_card")),
        });

        // Register
        register_model(&db, model.clone()).await.unwrap();

        // Get
        let retrieved = get_model(&db, "test-model").await.unwrap().unwrap();
        assert_eq!(retrieved.name, "test-model");
        assert_eq!(retrieved.config_hash, model.config_hash);

        // Verify hashes
        let hashes = ModelHashes {
            config_hash: model.config_hash,
            tokenizer_hash: model.tokenizer_hash,
            tokenizer_cfg_hash: model.tokenizer_cfg_hash,
            weights_hash: model.weights_hash,
            license_hash: model.license_hash,
        };
        let verified = verify_model_hashes(&db, "test-model", &hashes)
            .await
            .unwrap();
        assert!(verified);

        // List
        let models = list_models(&db).await.unwrap();
        assert_eq!(models.len(), 1);
    }
}
