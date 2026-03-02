//! Key metadata for lifecycle tracking

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct KeyMetadata {
    pub key_label: String,
    pub created_at: i64,
    pub source: String,
    pub key_type: String,
    pub last_checked: String,
}

impl Db {
    /// Insert or update key metadata
    pub async fn upsert_key_metadata(
        &self,
        key_label: &str,
        created_at: i64,
        source: &str,
        key_type: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO key_metadata (key_label, created_at, source, key_type, last_checked)
             VALUES (?, ?, ?, ?, datetime('now'))
             ON CONFLICT(key_label) DO UPDATE SET
                created_at = excluded.created_at,
                source = excluded.source,
                key_type = excluded.key_type,
                last_checked = datetime('now')",
        )
        .bind(key_label)
        .bind(created_at)
        .bind(source)
        .bind(key_type)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to upsert key metadata: {}", e)))?;

        Ok(())
    }

    /// Get key metadata by label
    pub async fn get_key_metadata(&self, key_label: &str) -> Result<Option<KeyMetadata>> {
        let metadata = sqlx::query_as::<_, KeyMetadata>(
            "SELECT key_label, created_at, source, key_type, last_checked
             FROM key_metadata
             WHERE key_label = ?",
        )
        .bind(key_label)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get key metadata: {}", e)))?;

        Ok(metadata)
    }

    /// List all keys
    pub async fn list_all_keys(&self) -> Result<Vec<KeyMetadata>> {
        let keys = sqlx::query_as::<_, KeyMetadata>(
            "SELECT key_label, created_at, source, key_type, last_checked
             FROM key_metadata
             ORDER BY created_at ASC",
        )
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list all keys: {}", e)))?;

        Ok(keys)
    }

    /// List keys older than a threshold (in days)
    pub async fn list_old_keys(&self, threshold_days: i64) -> Result<Vec<KeyMetadata>> {
        let threshold_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - (threshold_days * 86400);

        let keys = sqlx::query_as::<_, KeyMetadata>(
            "SELECT key_label, created_at, source, key_type, last_checked
             FROM key_metadata
             WHERE created_at < ?
             ORDER BY created_at ASC",
        )
        .bind(threshold_timestamp)
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list old keys: {}", e)))?;

        Ok(keys)
    }

    /// Get key age in days
    pub async fn get_key_age_days(&self, key_label: &str) -> Result<Option<i64>> {
        if let Some(metadata) = self.get_key_metadata(key_label).await? {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let age_seconds = now - metadata.created_at;
            let age_days = age_seconds / 86400;
            Ok(Some(age_days))
        } else {
            Ok(None)
        }
    }
}
