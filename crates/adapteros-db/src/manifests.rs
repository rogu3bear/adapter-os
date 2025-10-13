use crate::{models::Manifest, Db};
use anyhow::Result;
use uuid::Uuid;

impl Db {
    pub async fn create_manifest(
        &self,
        tenant_id: &str,
        hash_b3: &str,
        body_json: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(hash_b3)
        .bind(body_json)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_manifest_by_hash(&self, hash_b3: &str) -> Result<Option<Manifest>> {
        let manifest = sqlx::query_as::<_, Manifest>(
            "SELECT id, tenant_id, hash_b3, body_json, created_at FROM manifests WHERE hash_b3 = ?",
        )
        .bind(hash_b3)
        .fetch_optional(self.pool())
        .await?;
        Ok(manifest)
    }
}
