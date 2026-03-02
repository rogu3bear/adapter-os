use crate::{new_id, Db, Result};
use adapteros_core::AosError;
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CoremlFusionPair {
    pub id: String,
    pub tenant_id: String,
    pub base_model_id: String,
    pub adapter_id: String,
    pub fused_manifest_hash: String,
    pub coreml_package_hash: String,
    pub adapter_hash_b3: Option<String>,
    pub base_model_hash_b3: Option<String>,
    pub metadata_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateCoremlFusionPairParams {
    pub tenant_id: String,
    pub base_model_id: String,
    pub adapter_id: String,
    pub fused_manifest_hash: String,
    pub coreml_package_hash: String,
    pub adapter_hash_b3: Option<String>,
    pub base_model_hash_b3: Option<String>,
    pub metadata_path: Option<String>,
}

impl Db {
    /// Create or update a CoreML fusion pairing record.
    pub async fn upsert_coreml_fusion_pair(
        &self,
        params: CreateCoremlFusionPairParams,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Mdl);

        sqlx::query(
            r#"
            INSERT INTO coreml_fusion_pairs (
                id, tenant_id, base_model_id, adapter_id,
                fused_manifest_hash, coreml_package_hash,
                adapter_hash_b3, base_model_hash_b3, metadata_path, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(tenant_id, base_model_id, adapter_id, coreml_package_hash)
            DO UPDATE SET
                fused_manifest_hash = excluded.fused_manifest_hash,
                adapter_hash_b3 = excluded.adapter_hash_b3,
                base_model_hash_b3 = excluded.base_model_hash_b3,
                metadata_path = excluded.metadata_path
            "#,
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.base_model_id)
        .bind(&params.adapter_id)
        .bind(&params.fused_manifest_hash)
        .bind(&params.coreml_package_hash)
        .bind(&params.adapter_hash_b3)
        .bind(&params.base_model_hash_b3)
        .bind(&params.metadata_path)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to upsert fusion pair: {}", e)))?;

        Ok(id)
    }

    /// Fetch the most recent fusion pairing for a base model + adapter.
    pub async fn get_coreml_fusion_pair(
        &self,
        tenant_id: &str,
        base_model_id: &str,
        adapter_id: &str,
    ) -> Result<Option<CoremlFusionPair>> {
        let row = sqlx::query_as::<_, CoremlFusionPair>(
            r#"
            SELECT id, tenant_id, base_model_id, adapter_id,
                   fused_manifest_hash, coreml_package_hash,
                   adapter_hash_b3, base_model_hash_b3, metadata_path, created_at
            FROM coreml_fusion_pairs
            WHERE tenant_id = ? AND base_model_id = ? AND adapter_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(base_model_id)
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch fusion pair: {}", e)))?;

        Ok(row)
    }
}
