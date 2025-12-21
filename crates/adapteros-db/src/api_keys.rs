use crate::{Db, StorageMode};
use adapteros_core::{AosError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::users::Role;

fn pool_or_err(db: &Db) -> Result<&sqlx::SqlitePool> {
    db.pool_opt().ok_or_else(|| {
        AosError::Database("SQL pool not available for current storage mode".to_string())
    })
}

fn time_now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

/// Stored API key record (hash is persisted, token is never stored)
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApiKeyRecord {
    pub id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub name: String,
    pub scopes: String,
    pub hash: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

impl ApiKeyRecord {
    pub fn parsed_scopes(&self) -> Result<Vec<Role>> {
        let roles: Vec<Role> = serde_json::from_str(&self.scopes)
            .map_err(|e| AosError::Parse(format!("invalid scopes: {e}")))?;
        Ok(roles)
    }
}

impl Db {
    /// Create a new API key (hash must already be computed)
    pub async fn create_api_key(
        &self,
        tenant_id: &str,
        user_id: &str,
        name: &str,
        scopes: &[Role],
        hash: &str,
    ) -> Result<String> {
        if !self.storage_mode().write_to_sql() {
            return Err(AosError::Database(
                "SQL backend required for API keys".to_string(),
            ));
        }

        let pool = pool_or_err(self)?;
        let scopes_json =
            serde_json::to_string(scopes).map_err(|e| AosError::Parse(e.to_string()))?;
        let id = uuid::Uuid::now_v7().to_string();
        let created_at = time_now_rfc3339();

        sqlx::query(
            "INSERT INTO api_keys (id, tenant_id, user_id, name, scopes, hash, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(name)
        .bind(&scopes_json)
        .bind(hash)
        .bind(&created_at)
        .execute(pool)
        .await?;

        Ok(id)
    }

    /// List API keys for a tenant (including revoked)
    pub async fn list_api_keys(&self, tenant_id: &str) -> Result<Vec<ApiKeyRecord>> {
        if !self.storage_mode().read_from_sql() {
            return Err(AosError::Database(
                "SQL backend required for API keys".to_string(),
            ));
        }

        let pool = pool_or_err(self)?;
        let rows = sqlx::query_as::<_, ApiKeyRecord>(
            "SELECT id, tenant_id, user_id, name, scopes, hash, created_at, revoked_at
             FROM api_keys
             WHERE tenant_id = ?
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        Ok(rows)
    }

    /// Revoke an API key (idempotent)
    pub async fn revoke_api_key(&self, tenant_id: &str, id: &str) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Err(AosError::Database(
                "SQL backend required for API keys".to_string(),
            ));
        }

        let pool = pool_or_err(self)?;
        let revoked_at = time_now_rfc3339();

        sqlx::query(
            "UPDATE api_keys
             SET revoked_at = COALESCE(revoked_at, ?)
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(&revoked_at)
        .bind(id)
        .bind(tenant_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Lookup an API key by hash (returns only active keys when include_revoked is false)
    pub async fn get_api_key_by_hash(
        &self,
        hash: &str,
        include_revoked: bool,
    ) -> Result<Option<ApiKeyRecord>> {
        if !self.storage_mode().read_from_sql() {
            return Err(AosError::Database(
                "SQL backend required for API keys".to_string(),
            ));
        }

        let pool = pool_or_err(self)?;
        // Join to users to hard-enforce tenant consistency at lookup time
        let row = if include_revoked {
            sqlx::query_as::<_, ApiKeyRecord>(
                "SELECT k.id, k.tenant_id, k.user_id, k.name, k.scopes, k.hash, k.created_at, k.revoked_at
                 FROM api_keys k
                 INNER JOIN users u ON u.id = k.user_id AND u.tenant_id = k.tenant_id
                 WHERE k.hash = ?
                 LIMIT 1",
            )
            .bind(hash)
            .fetch_optional(pool)
            .await?
        } else {
            sqlx::query_as::<_, ApiKeyRecord>(
                "SELECT k.id, k.tenant_id, k.user_id, k.name, k.scopes, k.hash, k.created_at, k.revoked_at
                 FROM api_keys k
                 INNER JOIN users u ON u.id = k.user_id AND u.tenant_id = k.tenant_id
                 WHERE k.hash = ? AND k.revoked_at IS NULL
                 LIMIT 1",
            )
            .bind(hash)
            .fetch_optional(pool)
            .await?
        };

        Ok(row)
    }
}
