use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub created_at: String,
}

impl Db {
    pub async fn create_tenant(&self, name: &str, itar_flag: bool) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, ?)")
            .bind(&id)
            .bind(name)
            .bind(itar_flag)
            .execute(self.pool())
            .await?;
        Ok(id)
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<Tenant>> {
        let tenant = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at FROM tenants WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(tenant)
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at FROM tenants ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(tenants)
    }

    /// Rename a tenant
    pub async fn rename_tenant(&self, id: &str, new_name: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET name = ?, created_at = created_at WHERE id = ?")
            .bind(new_name)
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
