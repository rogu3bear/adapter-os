use crate::{
    models::{BundleSignature, CpPointer},
    Db,
};
use anyhow::Result;
use uuid::Uuid;

impl Db {
    pub async fn get_cp_pointer_by_name(&self, name: &str) -> Result<Option<CpPointer>> {
        let cp = sqlx::query_as::<_, CpPointer>(
            "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at, signing_public_key FROM cp_pointers WHERE name = ?"
        )
        .bind(name)
        .fetch_optional(self.pool())
        .await?;
        Ok(cp)
    }

    pub async fn get_active_cp_pointer(&self, tenant_id: &str) -> Result<Option<CpPointer>> {
        let cp = sqlx::query_as::<_, CpPointer>(
            "SELECT id, tenant_id, name, plan_id, active, created_at, activated_at, signing_public_key FROM cp_pointers WHERE tenant_id = ? AND active = 1 LIMIT 1"
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(cp)
    }

    pub async fn deactivate_all_cp_pointers(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("UPDATE cp_pointers SET active = 0 WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn activate_cp_pointer(&self, id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE cp_pointers SET active = 1, activated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(self.pool())
        .await?;
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
            .await?;
        Ok(())
    }

    pub async fn create_bundle_signature(
        &self,
        bundle_hash_b3: &str,
        cpid: &str,
        signature_hex: &str,
        public_key_hex: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO bundle_signatures (id, bundle_hash_b3, cpid, signature_hex, public_key_hex) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(bundle_hash_b3)
        .bind(cpid)
        .bind(signature_hex)
        .bind(public_key_hex)
        .execute(self.pool())
        .await?;
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
        .await?;
        Ok(sig)
    }
}
