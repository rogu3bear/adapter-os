//! Model registry operations

use adapteros_core::{AosError, B3Hash, Result};
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use std::path::Path;

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

impl ModelRecord {
    /// Create from individual components
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

    /// Convert from database row
    fn from_row(row: &Row) -> Result<Self> {
        let name: String = row.get(0)?;
        let config_hash_str: String = row.get(1)?;
        let tokenizer_hash_str: String = row.get(2)?;
        let tokenizer_cfg_hash_str: String = row.get(3)?;
        let weights_hash_str: String = row.get(4)?;
        let license_hash_str: String = row.get(5)?;
        let license_text: String = row.get(6)?;
        let model_card_hash_str: Option<String> = row.get(7)?;
        let created_at: i64 = row.get(8)?;

        let config_hash = B3Hash::from_hex(&config_hash_str)
            .map_err(|e| AosError::Registry(format!("Invalid config hash: {}", e)))?;
        let tokenizer_hash = B3Hash::from_hex(&tokenizer_hash_str)
            .map_err(|e| AosError::Registry(format!("Invalid tokenizer hash: {}", e)))?;
        let tokenizer_cfg_hash = B3Hash::from_hex(&tokenizer_cfg_hash_str)
            .map_err(|e| AosError::Registry(format!("Invalid tokenizer_cfg hash: {}", e)))?;
        let weights_hash = B3Hash::from_hex(&weights_hash_str)
            .map_err(|e| AosError::Registry(format!("Invalid weights hash: {}", e)))?;
        let license_hash = B3Hash::from_hex(&license_hash_str)
            .map_err(|e| AosError::Registry(format!("Invalid license hash: {}", e)))?;

        let model_card_hash = match model_card_hash_str {
            Some(s) => Some(
                B3Hash::from_hex(&s)
                    .map_err(|e| AosError::Registry(format!("Invalid model_card hash: {}", e)))?,
            ),
            None => None,
        };

        Ok(Self {
            name,
            config_hash,
            tokenizer_hash,
            tokenizer_cfg_hash,
            weights_hash,
            license_hash,
            license_text,
            model_card_hash,
            created_at,
        })
    }
}

/// Model registry operations
pub struct ModelRegistry {
    conn: Connection,
}

impl ModelRegistry {
    /// Create new model registry
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Register a new model
    pub fn register_model(&self, model: ModelRecord) -> Result<()> {
        // Check for hash collisions with different models
        self.check_hash_collisions(&model)?;

        let sql = r#"
            INSERT INTO models (
                name, config_hash, tokenizer_hash, tokenizer_cfg_hash,
                weights_hash, license_hash, license_text, model_card_hash, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        self.conn.execute(
            sql,
            params![
                model.name,
                model.config_hash.to_string(),
                model.tokenizer_hash.to_string(),
                model.tokenizer_cfg_hash.to_string(),
                model.weights_hash.to_string(),
                model.license_hash.to_string(),
                model.license_text,
                model.model_card_hash.map(|h| h.to_string()),
                model.created_at,
            ],
        )?;

        Ok(())
    }

    /// Get model by name
    pub fn get_model(&self, name: &str) -> Result<Option<ModelRecord>> {
        let sql = "SELECT * FROM models WHERE name = ?";

        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query(params![name])?;

        match rows.next()? {
            Some(row) => Ok(Some(ModelRecord::from_row(row)?)),
            None => Ok(None),
        }
    }

    /// Verify model hashes match registered values
    pub fn verify_model_hashes(
        &self,
        name: &str,
        config_hash: &B3Hash,
        tokenizer_hash: &B3Hash,
        tokenizer_cfg_hash: &B3Hash,
        weights_hash: &B3Hash,
        license_hash: &B3Hash,
    ) -> Result<bool> {
        let model = match self.get_model(name)? {
            Some(m) => m,
            None => return Ok(false),
        };

        Ok(model.config_hash == *config_hash
            && model.tokenizer_hash == *tokenizer_hash
            && model.tokenizer_cfg_hash == *tokenizer_cfg_hash
            && model.weights_hash == *weights_hash
            && model.license_hash == *license_hash)
    }

    /// List all registered models
    pub fn list_models(&self) -> Result<Vec<ModelRecord>> {
        let sql = "SELECT * FROM models ORDER BY created_at DESC";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![], |row| {
            ModelRecord::from_row(row)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        })?;

        let mut models = Vec::new();
        for row_result in rows {
            models.push(row_result?);
        }

        Ok(models)
    }

    /// Check for hash collisions with existing models
    fn check_hash_collisions(&self, model: &ModelRecord) -> Result<()> {
        // Check each hash against existing models
        let hashes_to_check = vec![
            ("config", &model.config_hash),
            ("tokenizer", &model.tokenizer_hash),
            ("tokenizer_cfg", &model.tokenizer_cfg_hash),
            ("weights", &model.weights_hash),
            ("license", &model.license_hash),
        ];

        for (hash_type, hash) in hashes_to_check {
            let sql = format!(
                "SELECT name FROM models WHERE {}_hash = ? AND name != ?",
                hash_type
            );

            let mut stmt = self.conn.prepare(&sql)?;
            let mut rows = stmt.query(params![hash.to_string(), &model.name])?;

            if let Some(row) = rows.next()? {
                let existing_name: String = row.get(0)?;
                return Err(AosError::Registry(format!(
                    "Hash collision: {} hash {} already used by model '{}'",
                    hash_type, hash, existing_name
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_model_registration() {
        let temp_dir = tempdir().expect("Test temp directory creation should succeed");
        let db_path = temp_dir.path().join("test.db");
        let mut conn = Connection::open(&db_path).expect("Database connection should succeed");

        // Run migrations
        crate::registry::migrations::run_migrations(&mut conn)
            .expect("Database migrations should succeed");

        let registry = ModelRegistry::new(conn);

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

        // Register model
        registry
            .register_model(model.clone())
            .expect("Model registration should succeed");

        // Retrieve model
        let retrieved = registry
            .get_model("test-model")
            .expect("Getting model should succeed")
            .expect("Model should exist");
        assert_eq!(retrieved.name, model.name);
        assert_eq!(retrieved.config_hash, model.config_hash);
        assert_eq!(retrieved.license_text, model.license_text);

        // Verify hashes
        let verified = registry
            .verify_model_hashes(
                "test-model",
                &model.config_hash,
                &model.tokenizer_hash,
                &model.tokenizer_cfg_hash,
                &model.weights_hash,
                &model.license_hash,
            )
            .expect("Hash verification should succeed");
        assert!(verified);
    }

    #[test]
    fn test_hash_collision_detection() {
        let temp_dir = tempdir().expect("Test temp directory creation should succeed");
        let db_path = temp_dir.path().join("test.db");
        let mut conn = Connection::open(&db_path).expect("Database connection should succeed");

        crate::registry::migrations::run_migrations(&mut conn)
            .expect("Database migrations should succeed");
        let registry = ModelRegistry::new(conn);

        let model1 = ModelRecord::from_input(ModelRecordInput {
            name: "model1".to_string(),
            config_hash: B3Hash::hash(b"config"),
            tokenizer_hash: B3Hash::hash(b"tokenizer1"),
            tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg1"),
            weights_hash: B3Hash::hash(b"weights1"),
            license_hash: B3Hash::hash(b"license1"),
            license_text: "MIT".to_string(),
            model_card_hash: None,
        });

        let model2 = ModelRecord::from_input(ModelRecordInput {
            name: "model2".to_string(),
            config_hash: B3Hash::hash(b"config"), // Same config hash
            tokenizer_hash: B3Hash::hash(b"tokenizer2"),
            tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg2"),
            weights_hash: B3Hash::hash(b"weights2"),
            license_hash: B3Hash::hash(b"license2"),
            license_text: "Apache".to_string(),
            model_card_hash: None,
        });

        // Register first model
        registry
            .register_model(model1)
            .expect("First model registration should succeed");

        // Second model should fail due to hash collision
        let result = registry.register_model(model2);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Hash collision"));
    }
}
