//! User entity KV storage operations
//!
//! This module provides KV storage operations for the User entity, enabling
//! dual-write migration from SQL to KV storage.

use crate::users::User;
// Use storage Role type for KV operations, local Role for SQL compatibility
use adapteros_core::{AosError, Result};
pub use adapteros_storage::entities::user::Role;
use adapteros_storage::entities::user::UserKv;
use adapteros_storage::KvBackend;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

/// Key patterns for user storage
///
/// Primary key: `user/{id}`
/// Secondary indexes:
/// - `user-by-email/{email}` -> `{id}`
/// - `tenant/{tenant_id}/users` -> Set<{id}>
/// - `users-by-role/{role}` -> Set<{id}>
pub struct UserKeys;

impl UserKeys {
    /// Primary key for user entity
    pub fn user(id: &str) -> String {
        format!("user/{}", id)
    }

    /// Secondary index: email -> user_id
    pub fn email_index(email: &str) -> String {
        format!("user-by-email/{}", email)
    }

    /// Secondary index: tenant users set
    pub fn tenant_users_set(tenant_id: &str) -> String {
        format!("tenant/{}/users", tenant_id)
    }

    /// Secondary index: role users set
    /// Takes a role string to avoid type conflicts between crate::users::Role
    /// and adapteros_storage::entities::user::Role
    pub fn role_users_set(role_str: &str) -> String {
        format!("users-by-role/{}", role_str)
    }

    /// Prefix for scanning all users
    pub fn all_users_prefix() -> &'static str {
        "user/"
    }
}

/// User KV operations trait
///
/// Defines all user-related operations for KV storage backend.
/// Implementations must maintain consistency with secondary indexes.
#[async_trait]
pub trait UserKvOps {
    /// Create a new user
    async fn create_user_kv(
        &self,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
        tenant_id: &str,
    ) -> Result<String>;

    /// Get user by ID
    async fn get_user_kv(&self, id: &str) -> Result<Option<UserKv>>;

    /// Get user by email
    async fn get_user_by_email_kv(&self, email: &str) -> Result<Option<UserKv>>;

    /// Ensure a user with specific ID exists (idempotent)
    async fn ensure_user_kv(
        &self,
        id: &str,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
        tenant_id: &str,
    ) -> Result<()>;

    /// Count total users
    async fn count_users_kv(&self) -> Result<i64>;

    /// List users by tenant
    async fn list_users_by_tenant_kv(&self, tenant_id: &str) -> Result<Vec<UserKv>>;

    /// List users by role
    async fn list_users_by_role_kv(&self, role: &Role) -> Result<Vec<UserKv>>;

    /// Update user role
    async fn update_user_role_kv(&self, id: &str, role: Role) -> Result<()>;

    /// Update user disabled status
    async fn update_user_disabled_kv(&self, id: &str, disabled: bool) -> Result<()>;

    /// Delete user (with cascade cleanup of indexes)
    async fn delete_user_kv(&self, id: &str) -> Result<bool>;
}

/// Implementation of UserKvOps for any KvBackend
pub struct UserKvRepository<B: KvBackend> {
    backend: B,
}

impl<B: KvBackend> UserKvRepository<B> {
    /// Create a new UserKvRepository instance
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    /// Internal helper: serialize UserKv to bytes
    fn serialize_user(user: &UserKv) -> Result<Vec<u8>> {
        serde_json::to_vec(user).map_err(|e| {
            AosError::Serialization(e)
        })
    }

    /// Internal helper: deserialize bytes to UserKv
    fn deserialize_user(bytes: &[u8]) -> Result<UserKv> {
        serde_json::from_slice(bytes).map_err(|e| {
            AosError::Serialization(e)
        })
    }

    /// Internal helper: get user without exposing password hash
    async fn get_user_safe(&self, id: &str) -> Result<Option<UserKv>> {
        let key = UserKeys::user(id);

        match self.backend.get(&key).await {
            Ok(Some(bytes)) => {
                let mut user = Self::deserialize_user(&bytes)?;
                // Never expose password hash in reads
                user.pw_hash = String::new();
                Ok(Some(user))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(AosError::Database(format!("Failed to get user: {}", e))),
        }
    }

    /// Internal helper: get user including password hash (for auth only)
    async fn get_user_with_pw_hash(&self, id: &str) -> Result<Option<UserKv>> {
        let key = UserKeys::user(id);

        match self.backend.get(&key).await {
            Ok(Some(bytes)) => Ok(Some(Self::deserialize_user(&bytes)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(AosError::Database(format!("Failed to get user: {}", e))),
        }
    }

    /// Internal helper: update secondary indexes when creating/updating user
    async fn update_indexes(&self, user: &UserKv, old_user: Option<&UserKv>) -> Result<()> {
        // Email index
        let email_key = UserKeys::email_index(&user.email);
        self.backend
            .set(&email_key, user.id.as_bytes().to_vec())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update email index: {}", e)))?;

        // Tenant users set - add to set
        let tenant_set_key = UserKeys::tenant_users_set(&user.tenant_id);
        // For now, we'll store a simple value - proper set support requires backend enhancement
        self.backend
            .set(&format!("{}::{}", tenant_set_key, user.id), vec![1])
            .await
            .map_err(|e| AosError::Database(format!("Failed to update tenant index: {}", e)))?;

        // Role users set - add to new role, remove from old role if changed
        if let Some(old) = old_user {
            if old.role != user.role {
                // Remove from old role set
                let old_role_key = format!("{}::{}", UserKeys::role_users_set(&old.role.to_string()), user.id);
                let _ = self.backend.delete(&old_role_key).await;
            }
        }

        let role_set_key = UserKeys::role_users_set(&user.role.to_string());
        self.backend
            .set(&format!("{}::{}", role_set_key, user.id), vec![1])
            .await
            .map_err(|e| AosError::Database(format!("Failed to update role index: {}", e)))?;

        Ok(())
    }

    /// Internal helper: cleanup secondary indexes when deleting user
    async fn cleanup_indexes(&self, user: &UserKv) -> Result<()> {
        // Email index
        let email_key = UserKeys::email_index(&user.email);
        let _ = self.backend.delete(&email_key).await;

        // Tenant users set
        let tenant_key = format!("{}::{}", UserKeys::tenant_users_set(&user.tenant_id), user.id);
        let _ = self.backend.delete(&tenant_key).await;

        // Role users set
        let role_key = format!("{}::{}", UserKeys::role_users_set(&user.role.to_string()), user.id);
        let _ = self.backend.delete(&role_key).await;

        Ok(())
    }
}

#[async_trait]
impl<B: KvBackend> UserKvOps for UserKvRepository<B> {
    async fn create_user_kv(
        &self,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
        tenant_id: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        let user = UserKv {
            id: id.clone(),
            email: email.to_string(),
            display_name: display_name.to_string(),
            pw_hash: pw_hash.to_string(),
            role,
            tenant_id: tenant_id.to_string(),
            disabled: false,
            created_at: Utc::now(),
        };

        // Store user entity
        let key = UserKeys::user(&id);
        let value = Self::serialize_user(&user)?;

        self.backend
            .set(&key, value)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create user: {}", e)))?;

        // Update secondary indexes
        self.update_indexes(&user, None).await?;

        debug!(user_id = %id, email = %email, role = %role, "Created user in KV storage");

        Ok(id)
    }

    async fn get_user_kv(&self, id: &str) -> Result<Option<UserKv>> {
        self.get_user_safe(id).await
    }

    async fn get_user_by_email_kv(&self, email: &str) -> Result<Option<UserKv>> {
        // Look up user ID from email index
        let email_key = UserKeys::email_index(email);

        match self.backend.get(&email_key).await {
            Ok(Some(id_bytes)) => {
                let id = String::from_utf8(id_bytes)
                    .map_err(|e| AosError::Database(format!("Invalid user ID in email index: {}", e)))?;

                // Get user with password hash (needed for authentication)
                self.get_user_with_pw_hash(&id).await
            }
            Ok(None) => Ok(None),
            Err(e) => Err(AosError::Database(format!("Failed to lookup email index: {}", e))),
        }
    }

    async fn ensure_user_kv(
        &self,
        id: &str,
        email: &str,
        display_name: &str,
        pw_hash: &str,
        role: Role,
        tenant_id: &str,
    ) -> Result<()> {
        // Check if user already exists
        if self.get_user_kv(id).await?.is_some() {
            debug!(user_id = %id, "User already exists, skipping creation");
            return Ok(());
        }

        // User doesn't exist, create it with specific ID
        let user = UserKv {
            id: id.to_string(),
            email: email.to_string(),
            display_name: display_name.to_string(),
            pw_hash: pw_hash.to_string(),
            role,
            tenant_id: tenant_id.to_string(),
            disabled: false,
            created_at: Utc::now(),
        };

        // Store user entity
        let key = UserKeys::user(id);
        let value = Self::serialize_user(&user)?;

        self.backend
            .set(&key, value)
            .await
            .map_err(|e| AosError::Database(format!("Failed to ensure user: {}", e)))?;

        // Update secondary indexes
        self.update_indexes(&user, None).await?;

        debug!(user_id = %id, email = %email, "Ensured user exists in KV storage");

        Ok(())
    }

    async fn count_users_kv(&self) -> Result<i64> {
        // Scan all user keys and count them
        let prefix = UserKeys::all_users_prefix();

        match self.backend.scan_prefix(prefix).await {
            Ok(keys) => Ok(keys.len() as i64),
            Err(e) => Err(AosError::Database(format!("Failed to count users: {}", e))),
        }
    }

    async fn list_users_by_tenant_kv(&self, tenant_id: &str) -> Result<Vec<UserKv>> {
        // Scan tenant users set
        let prefix = format!("{}::", UserKeys::tenant_users_set(tenant_id));

        let keys = self.backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan tenant users: {}", e)))?;

        let mut users = Vec::new();

        for key in keys {
            // Extract user_id from key format "tenant/{tenant_id}/users::{user_id}"
            if let Some(user_id) = key.split("::").nth(1) {
                if let Some(user) = self.get_user_safe(user_id).await? {
                    users.push(user);
                }
            }
        }

        Ok(users)
    }

    async fn list_users_by_role_kv(&self, role: &Role) -> Result<Vec<UserKv>> {
        // Scan role users set
        let prefix = format!("{}::", UserKeys::role_users_set(&role.to_string()));

        let keys = self.backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan role users: {}", e)))?;

        let mut users = Vec::new();

        for key in keys {
            // Extract user_id from key format "users-by-role/{role}::{user_id}"
            if let Some(user_id) = key.split("::").nth(1) {
                if let Some(user) = self.get_user_safe(user_id).await? {
                    users.push(user);
                }
            }
        }

        Ok(users)
    }

    async fn update_user_role_kv(&self, id: &str, role: Role) -> Result<()> {
        // Get existing user
        let key = UserKeys::user(id);
        let bytes = self.backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get user: {}", e)))?
            .ok_or_else(|| AosError::Database(format!("User not found: {}", id)))?;

        let old_user = Self::deserialize_user(&bytes)?;
        let mut updated_user = old_user.clone();
        updated_user.role = role;

        // Update user entity
        let value = Self::serialize_user(&updated_user)?;
        self.backend
            .set(&key, value)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update user role: {}", e)))?;

        // Update indexes (will handle role set changes)
        self.update_indexes(&updated_user, Some(&old_user)).await?;

        debug!(user_id = %id, new_role = %role, "Updated user role in KV storage");

        Ok(())
    }

    async fn update_user_disabled_kv(&self, id: &str, disabled: bool) -> Result<()> {
        // Get existing user
        let key = UserKeys::user(id);
        let bytes = self.backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get user: {}", e)))?
            .ok_or_else(|| AosError::Database(format!("User not found: {}", id)))?;

        let mut user = Self::deserialize_user(&bytes)?;
        user.disabled = disabled;

        // Update user entity
        let value = Self::serialize_user(&user)?;
        self.backend
            .set(&key, value)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update user disabled status: {}", e)))?;

        debug!(user_id = %id, disabled = %disabled, "Updated user disabled status in KV storage");

        Ok(())
    }

    async fn delete_user_kv(&self, id: &str) -> Result<bool> {
        // Get existing user to cleanup indexes
        let key = UserKeys::user(id);

        let bytes = match self.backend.get(&key).await {
            Ok(Some(b)) => b,
            Ok(None) => return Ok(false),
            Err(e) => return Err(AosError::Database(format!("Failed to get user: {}", e))),
        };

        let user = Self::deserialize_user(&bytes)?;

        // Cleanup secondary indexes first
        self.cleanup_indexes(&user).await?;

        // Delete user entity
        let deleted = self.backend
            .delete(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete user: {}", e)))?;

        if deleted {
            debug!(user_id = %id, "Deleted user from KV storage");
        }

        Ok(deleted)
    }
}

/// Conversion functions between SQL User and KV UserKv types

/// Convert SQL User to KV UserKv
pub fn user_to_kv(sql_user: &User) -> Result<UserKv> {
    // Parse role from string using storage Role type
    let role: Role = sql_user
        .role
        .parse()
        .map_err(|e| AosError::Parse(format!("Invalid role '{}': {}", sql_user.role, e)))?;

    // Parse created_at timestamp
    let created_at = DateTime::parse_from_rfc3339(&sql_user.created_at)
        .or_else(|_| {
            // Try parsing SQLite datetime format
            chrono::NaiveDateTime::parse_from_str(&sql_user.created_at, "%Y-%m-%d %H:%M:%S")
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
                .map(|dt| dt.into())
        })
        .map_err(|e| AosError::Parse(format!("Failed to parse created_at: {}", e)))?
        .with_timezone(&Utc);

    Ok(UserKv {
        id: sql_user.id.clone(),
        email: sql_user.email.clone(),
        display_name: sql_user.display_name.clone(),
        pw_hash: sql_user.pw_hash.clone(),
        role,
        tenant_id: sql_user.tenant_id.clone(),
        disabled: sql_user.disabled,
        created_at,
    })
}

/// Convert KV UserKv to SQL User
pub fn kv_to_user(kv_user: &UserKv) -> User {
    User {
        id: kv_user.id.clone(),
        email: kv_user.email.clone(),
        display_name: kv_user.display_name.clone(),
        pw_hash: kv_user.pw_hash.clone(),
        role: kv_user.role.to_string(),
        disabled: kv_user.disabled,
        created_at: kv_user.created_at.to_rfc3339(),
        tenant_id: kv_user.tenant_id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_keys() {
        assert_eq!(UserKeys::user("user-123"), "user/user-123");
        assert_eq!(
            UserKeys::email_index("admin@aos.local"),
            "user-by-email/admin@aos.local"
        );
        assert_eq!(
            UserKeys::tenant_users_set("tenant-1"),
            "tenant/tenant-1/users"
        );
        assert_eq!(
            UserKeys::role_users_set("admin"),
            "users-by-role/admin"
        );
    }

    #[test]
    fn test_user_conversion() {
        let sql_user = User {
            id: "user-1".to_string(),
            email: "test@example.com".to_string(),
            display_name: "Test User".to_string(),
            pw_hash: "hash123".to_string(),
            role: "admin".to_string(),
            disabled: false,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            tenant_id: "tenant-1".to_string(),
        };

        let kv_user = user_to_kv(&sql_user).unwrap();
        assert_eq!(kv_user.id, "user-1");
        assert_eq!(kv_user.email, "test@example.com");
        assert_eq!(kv_user.role, Role::Admin);

        let back_to_sql = kv_to_user(&kv_user);
        assert_eq!(back_to_sql.id, sql_user.id);
        assert_eq!(back_to_sql.email, sql_user.email);
        assert_eq!(back_to_sql.role, "admin");
    }
}
