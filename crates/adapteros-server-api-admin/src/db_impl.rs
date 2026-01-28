//! AdminDb trait implementation for adapteros-db::Db
//!
//! This module bridges the Db type from adapteros-db to the AdminDb trait
//! required by admin handlers.

use crate::state::AdminDb;
use adapteros_core::Result;
use adapteros_db::models::Worker;
use adapteros_db::tenants::Tenant;
use adapteros_db::users::User;
use adapteros_db::Db;

impl AdminDb for Db {
    async fn list_users(
        &self,
        page: Option<i64>,
        page_size: Option<i64>,
        role: Option<&str>,
        tenant_id: Option<&str>,
    ) -> Result<(Vec<User>, i64)> {
        self.list_users(page.unwrap_or(1), page_size.unwrap_or(20), role, tenant_id)
            .await
    }

    async fn list_active_workers(&self) -> Result<Vec<Worker>> {
        self.list_active_workers().await
    }

    async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        self.list_tenants().await
    }
}
