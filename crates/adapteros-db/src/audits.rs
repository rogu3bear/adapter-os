use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Audit {
    pub id: String,
    pub tenant_id: String,
    pub cpid: Option<String>,
    pub suite_name: String,
    pub bundle_id: Option<String>,
    pub result_json: String,
    pub status: String,
    pub created_at: String,
}

impl Db {
    pub async fn create_audit(
        &self,
        tenant_id: &str,
        cpid: &str,
        suite_name: &str,
        bundle_id: Option<&str>,
        result_json: &str,
        status: &str,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Aud);
        sqlx::query(
            "INSERT INTO audits (id, tenant_id, cpid, suite_name, bundle_id, result_json, status) VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(cpid)
        .bind(suite_name)
        .bind(bundle_id)
        .bind(result_json)
        .bind(status)
        .execute(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(id)
    }

    pub async fn list_all_audits(&self) -> Result<Vec<Audit>> {
        let audits = sqlx::query_as::<_, Audit>(
            "SELECT id, tenant_id, cpid, suite_name, bundle_id, result_json, status, created_at FROM audits ORDER BY created_at DESC"
        )
        .fetch_all(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(audits)
    }

    pub async fn get_audit(&self, id: &str) -> Result<Option<Audit>> {
        let audit = sqlx::query_as::<_, Audit>(
            "SELECT id, tenant_id, cpid, suite_name, bundle_id, result_json, status, created_at FROM audits WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(self.pool_result()?)
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
        Ok(audit)
    }
}
