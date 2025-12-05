use crate::users_kv::{Role as KvRole, UserKvOps, UserKvRepository};
use crate::{Db, StorageMode};
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "operator")]
    Operator,
    #[serde(rename = "sre")]
    SRE,
    #[serde(rename = "compliance")]
    Compliance,
    #[serde(rename = "viewer")]
    Viewer,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Admin => write!(f, "admin"),
            Role::Operator => write!(f, "operator"),
            Role::SRE => write!(f, "sre"),
            Role::Compliance => write!(f, "compliance"),
            Role::Viewer => write!(f, "viewer"),
        }
    }
}

impl std::str::FromStr for Role {
    type Err = adapteros_core::AosError;

    fn from_str(s: &str) -> Result<Self> {
        // Case-insensitive parsing for defense-in-depth
        match s.to_lowercase().as_str() {
            "admin" => Ok(Role::Admin),
            "operator" => Ok(Role::Operator),
            "sre" => Ok(Role::SRE),
            "compliance" => Ok(Role::Compliance),
            "viewer" => Ok(Role::Viewer),
            _ => Err(AosError::Parse(format!("invalid role: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(skip_serializing)]
    pub pw_hash: String,
    pub role: String,
    pub disabled: bool,
    pub created_at: String,
    /// Tenant ID associated with the user (defaults to "default" if not set in DB)
    #[sqlx(default)]
    #[serde(default = "default_tenant_id")]
    pub tenant_id: String,
}

fn default_tenant_id() -> String {
    "default".to_string()
}

/// Convert local Role to KV storage Role
fn to_kv_role(role: &Role) -> KvRole {
    match role {
        Role::Admin => KvRole::Admin,
        Role::Operator => KvRole::Operator,
        Role::SRE => KvRole::SRE,
        Role::Compliance => KvRole::Compliance,
        Role::Viewer => KvRole::Viewer,
    }
}

impl Db {
    /// Get a UserKvRepository if KV writes are enabled
    fn get_user_kv_repo(&self) -> Option<UserKvRepository<crate::kv_backend::KvDb>> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
            self.kv_backend()
                .map(|kv| UserKvRepository::new((**kv).clone()))
        } else {
            None
        }
    }

    /// Create a new user with dual-write support
    ///
    /// Writes to SQL backend, and also to KV backend if dual-write mode is enabled.
    /// The user ID is generated using UUIDv7 for time-ordered IDs.
    pub async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
        tenant_id: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let role_str = role.to_string();

        // SQL write if enabled
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT INTO users (id, email, display_name, pw_hash, role, tenant_id) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(email)
                .bind(display_name)
                .bind(pw_hash)
                .bind(&role_str)
                .bind(tenant_id)
                .execute(pool)
                .await?;
            } else {
                return Err(AosError::Database(
                    "SQL backend unavailable for user creation".to_string(),
                ));
            }
        }

        // KV write (dual-write mode)
        if let Some(repo) = self.get_user_kv_repo() {
            let kv_role = to_kv_role(&role);
            if let Err(e) = repo
                .create_user_kv(email, display_name, pw_hash, kv_role, tenant_id)
                .await
            {
                self.record_kv_write_fallback("users.create");
                warn!(error = %e, user_id = %id, "Failed to write user to KV backend (dual-write)");
            } else {
                debug!(user_id = %id, "User written to both SQL and KV backends");
            }
        }

        Ok(id)
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        // Prefer KV if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_user_kv_repo() {
                if let Some(kv_user) = repo.get_user_by_email_kv(email).await? {
                    return Ok(Some(crate::users_kv::kv_to_user(&kv_user)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("users.get_by_email.fallback");
        }

        // SQL fallback/read
        let pool = match self.pool_opt() {
            Some(p) => p,
            None => {
                return Ok(None);
            }
        };

        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, display_name, pw_hash, role, disabled, created_at, tenant_id FROM users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;
        Ok(user)
    }

    pub async fn get_user(&self, id: &str) -> Result<Option<User>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_user_kv_repo() {
                if let Some(kv_user) = repo.get_user_kv(id).await? {
                    return Ok(Some(crate::users_kv::kv_to_user(&kv_user)));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
            self.record_kv_read_fallback("users.get.fallback");
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(None),
        };

        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, display_name, pw_hash, role, disabled, created_at, tenant_id FROM users WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(user)
    }

    /// Ensure a user with a specific ID exists (used for dev bypass)
    /// Creates the user if not exists - does NOT update existing users to avoid FK issues
    pub async fn ensure_user(
        &self,
        id: &str,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
        tenant_id: &str,
    ) -> Result<()> {
        // First check if user already exists
        let existing = self.get_user(id).await?;
        if existing.is_some() {
            // User already exists, nothing to do
            return Ok(());
        }

        // User doesn't exist, insert new row
        let role_str = role.to_string();
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    "INSERT INTO users (id, email, display_name, pw_hash, role, tenant_id) VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(id)
                .bind(email)
                .bind(display_name)
                .bind(pw_hash)
                .bind(&role_str)
                .bind(tenant_id)
                .execute(pool)
                .await?;
            } else {
                return Err(AosError::Database(
                    "SQL backend unavailable for ensure_user".to_string(),
                ));
            }
        }

        // KV write (dual-write mode)
        if let Some(repo) = self.get_user_kv_repo() {
            let kv_role = to_kv_role(&role);
            if let Err(e) = repo
                .ensure_user_kv(id, email, display_name, pw_hash, kv_role, tenant_id)
                .await
            {
                self.record_kv_write_fallback("users.ensure");
                warn!(error = %e, user_id = %id, "Failed to ensure user in KV backend (dual-write)");
            } else {
                debug!(user_id = %id, "User ensured in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Count total number of users in the system
    pub async fn count_users(&self) -> Result<i64> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_user_kv_repo() {
                match repo.count_users_kv().await {
                    Ok(count) => return Ok(count),
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("users.count.fallback");
                        warn!(error = %e, "KV user count failed, falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Err(AosError::Database(
                    "KV user count unavailable and SQL fallback disabled".into(),
                ));
            }
        }

        let pool = self
            .pool_opt()
            .ok_or_else(|| AosError::Database("SQL backend unavailable for count_users".into()))?;

        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
            .fetch_one(pool)
            .await
            .db_err("count users")
    }

    /// List users with pagination and filtering
    ///
    /// Returns a tuple of (users, total_count) for pagination support.
    /// Supports filtering by role and tenant_id.
    pub async fn list_users(
        &self,
        page: i64,
        page_size: i64,
        role_filter: Option<&str>,
        tenant_filter: Option<&str>,
    ) -> Result<(Vec<User>, i64)> {
        if self.storage_mode().read_from_kv() {
            use std::str::FromStr;

            if let Some(repo) = self.get_user_kv_repo() {
                let kv_users_result = if let Some(tenant) = tenant_filter {
                    repo.list_users_by_tenant_kv(tenant).await
                } else if let Some(role) = role_filter {
                    let kv_role = KvRole::from_str(role).ok_or_else(|| {
                        AosError::Parse(format!("invalid role filter for KV: {}", role))
                    })?;
                    repo.list_users_by_role_kv(&kv_role).await
                } else {
                    repo.list_users_kv().await
                };

                match kv_users_result {
                    Ok(mut kv_users) => {
                        kv_users.sort_by(|a, b| {
                            b.created_at
                                .cmp(&a.created_at)
                                .then_with(|| a.id.cmp(&b.id))
                        });

                        let total = kv_users.len() as i64;
                        let start = if page <= 1 {
                            0
                        } else {
                            ((page - 1) * page_size).max(0) as usize
                        };
                        let end = (start + page_size.max(0) as usize).min(kv_users.len());
                        let window = if start < end {
                            kv_users[start..end].to_vec()
                        } else {
                            Vec::new()
                        };

                        let users = window
                            .iter()
                            .map(crate::users_kv::kv_to_user)
                            .collect::<Vec<_>>();
                        return Ok((users, total));
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("users.list.fallback");
                        warn!(error = %e, "KV user list failed, falling back to SQL");
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Build WHERE clause dynamically
        let mut where_clauses = Vec::new();

        if role_filter.is_some() {
            where_clauses.push("role = ?");
        }
        if tenant_filter.is_some() {
            where_clauses.push("COALESCE(tenant_id, 'default') = ?");
        }

        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        // Count total matching users
        let count_query = format!("SELECT COUNT(*) FROM users {}", where_clause);
        let mut count_builder = sqlx::query_scalar::<_, i64>(&count_query);

        if let Some(role) = role_filter {
            count_builder = count_builder.bind(role);
        }
        if let Some(tenant) = tenant_filter {
            count_builder = count_builder.bind(tenant);
        }

        let pool = self
            .pool_opt()
            .ok_or_else(|| AosError::Database("SQL backend unavailable for list_users".into()))?;
        let total = count_builder
            .fetch_one(pool)
            .await
            .db_err("count users with filters")?;

        // Calculate offset
        let offset = (page - 1) * page_size;

        // Build SELECT query
        let select_query = format!(
            "SELECT id, email, display_name, pw_hash, role, disabled, created_at, COALESCE(tenant_id, 'default') as tenant_id FROM users {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            where_clause
        );

        let mut select_builder = sqlx::query_as::<_, User>(&select_query);

        if let Some(role) = role_filter {
            select_builder = select_builder.bind(role);
        }
        if let Some(tenant) = tenant_filter {
            select_builder = select_builder.bind(tenant);
        }
        select_builder = select_builder.bind(page_size).bind(offset);

        let users = select_builder
            .fetch_all(pool)
            .await
            .db_err("list users")?;

        Ok((users, total))
    }

    /// Update user role with dual-write support
    ///
    /// Updates the user's role in SQL, and also in KV backend if dual-write mode is enabled.
    pub async fn update_user_role(&self, id: &str, role: Role) -> Result<()> {
        let role_str = role.to_string();

        // SQL write if enabled
        if self.storage_mode().write_to_sql() {
            let pool = self
                .pool_opt()
                .ok_or_else(|| AosError::Database("SQL backend unavailable for update_user_role".into()))?;
            let result = sqlx::query(
                "UPDATE users SET role = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(&role_str)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!("User not found: {}", id)));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No writable backend configured for update_user_role".to_string(),
            ));
        }

        // KV write (dual-write mode)
        if let Some(repo) = self.get_user_kv_repo() {
            let kv_role = to_kv_role(&role);
            if let Err(e) = repo.update_user_role_kv(id, kv_role).await {
                self.record_kv_write_fallback("users.update_role");
                warn!(error = %e, user_id = %id, "Failed to update user role in KV backend (dual-write)");
            } else {
                debug!(user_id = %id, role = %role, "User role updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Update user disabled status with dual-write support
    ///
    /// Updates the user's disabled status in SQL, and also in KV backend if dual-write mode is enabled.
    pub async fn update_user_disabled(&self, id: &str, disabled: bool) -> Result<()> {
        // SQL write if enabled
        if self.storage_mode().write_to_sql() {
            let pool = self
                .pool_opt()
                .ok_or_else(|| AosError::Database("SQL backend unavailable for update_user_disabled".into()))?;
            let result =
                sqlx::query("UPDATE users SET disabled = ?, updated_at = datetime('now') WHERE id = ?")
                    .bind(disabled)
                    .bind(id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(e.to_string()))?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!("User not found: {}", id)));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No writable backend configured for update_user_disabled".to_string(),
            ));
        }

        // KV write (dual-write mode)
        if let Some(repo) = self.get_user_kv_repo() {
            if let Err(e) = repo.update_user_disabled_kv(id, disabled).await {
                self.record_kv_write_fallback("users.update_disabled");
                warn!(error = %e, user_id = %id, "Failed to update user disabled status in KV backend (dual-write)");
            } else {
                debug!(user_id = %id, disabled = %disabled, "User disabled status updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Delete user with dual-write support
    ///
    /// Deletes the user from SQL, and also from KV backend if dual-write mode is enabled.
    /// Returns Ok(()) if user was deleted or didn't exist.
    pub async fn delete_user(&self, id: &str) -> Result<()> {
        // SQL write if enabled
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query("DELETE FROM users WHERE id = ?")
                    .bind(id)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::Database(e.to_string()))?;
            } else {
                return Err(AosError::Database(
                    "SQL backend unavailable for delete_user".to_string(),
                ));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No writable backend configured for delete_user".to_string(),
            ));
        }

        // KV write (dual-write mode)
        if let Some(repo) = self.get_user_kv_repo() {
            if let Err(e) = repo.delete_user_kv(id).await {
                self.record_kv_write_fallback("users.delete");
                warn!(error = %e, user_id = %id, "Failed to delete user from KV backend (dual-write)");
            } else {
                debug!(user_id = %id, "User deleted from both SQL and KV backends");
            }
        }

        Ok(())
    }
}
