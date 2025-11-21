use crate::Db;
use adapteros_core::{AosError, Result};

impl Db {
    pub async fn create_artifact(
        &self,
        hash_b3: &str,
        kind: &str,
        signature_b64: &str,
        sbom_hash_b3: Option<&str>,
        size_bytes: i64,
        stored_path: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO artifacts (hash_b3, kind, signature_b64, sbom_hash_b3, size_bytes, stored_path) VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(hash_b3)
        .bind(kind)
        .bind(signature_b64)
        .bind(sbom_hash_b3)
        .bind(size_bytes)
        .bind(stored_path)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create artifact: {}", e)))?;
        Ok(())
    }
}
