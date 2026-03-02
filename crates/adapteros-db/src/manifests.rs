use crate::{models::Manifest, new_id, Db};
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;

impl Db {
    pub async fn create_manifest(
        &self,
        tenant_id: &str,
        hash_b3: &str,
        body_json: &str,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Ver);
        sqlx::query(
            "INSERT INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(hash_b3)
        .bind(body_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(id)
    }

    pub async fn get_manifest_by_hash(&self, hash_b3: &str) -> Result<Option<Manifest>> {
        let manifest = sqlx::query_as::<_, Manifest>(
            "SELECT id, tenant_id, hash_b3, body_json, created_at FROM manifests WHERE hash_b3 = ?",
        )
        .bind(hash_b3)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(manifest)
    }
}
