use crate::{
    models::{BundleSignature, CpPointer},
    Db,
};
use adapteros_core::{AosError, Result};
use crate::new_id;
use adapteros_id::IdPrefix;

impl Db {
    pub async fn get_cp_pointer_by_name(&self, name: &str) -> Result<Option<CpPointer>> {
        let cp = sqlx::query_as::<_, CpPointer>(
            "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at, signing_public_key FROM cp_pointers WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get CP pointer by name: {}", e)))?;
        Ok(cp)
    }

    pub async fn get_active_cp_pointer(&self, tenant_id: &str) -> Result<Option<CpPointer>> {
        let cp = sqlx::query_as::<_, CpPointer>(
            "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at, signing_public_key FROM cp_pointers WHERE tenant_id = ? AND active = 1 LIMIT 1"
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get active CP pointer: {}", e)))?;
        Ok(cp)
    }

    pub async fn deactivate_all_cp_pointers(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to deactivate CP pointers: {}", e)))?;
        Ok(())
    }

    pub async fn activate_cp_pointer(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE cp_pointers SET active = 1, activated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to activate CP pointer: {}", e)))?;
        Ok(())
    }

    pub async fn update_cp_pointer_signing_key(
        &self,
        id: &str,
        public_key_hex: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE cp_pointers SET signing_public_key = ? WHERE id = ?")
            .bind(public_key_hex)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update CP pointer signing key: {}", e))
            })?;
        Ok(())
    }

    pub async fn create_bundle_signature(
        &self,
        bundle_hash_b3: &str,
        cpid: &str,
        signature_hex: &str,
        public_key_hex: &str,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Ver);
        sqlx::query(
            "INSERT INTO bundle_signatures (id, bundle_hash_b3, cpid, signature_hex, public_key_hex) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(bundle_hash_b3)
        .bind(cpid)
        .bind(signature_hex)
        .bind(public_key_hex)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create bundle signature: {}", e)))?;
        Ok(id)
    }

    pub async fn get_bundle_signature(
        &self,
        bundle_hash_b3: &str,
    ) -> Result<Option<BundleSignature>> {
        let sig = sqlx::query_as::<_, BundleSignature>(
            "SELECT id, bundle_hash_b3, cpid, signature_hex, public_key_hex, created_at FROM bundle_signatures WHERE bundle_hash_b3 = ?"
        )
        .bind(bundle_hash_b3)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get bundle signature: {}", e)))?;
        Ok(sig)
    }

    /// Insert a new control plane pointer
    pub async fn insert_cp_pointer(
        &self,
        id: &str,
        tenant_id: &str,
        name: &str,
        plan_id: &str,
        active: bool,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO cp_pointers (id, tenant_id, name, plan_id, active) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(name)
        .bind(plan_id)
        .bind(if active { 1 } else { 0 })
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert CP pointer: {}", e)))?;
        Ok(())
    }

    /// List all CP pointers for a tenant
    pub async fn list_cp_pointers_by_tenant(&self, tenant_id: &str) -> Result<Vec<CpPointer>> {
        let rows = sqlx::query_as::<_, CpPointer>(
            "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at, signing_public_key FROM cp_pointers WHERE tenant_id = ? ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list CP pointers: {}", e)))?;
        Ok(rows)
    }
}
