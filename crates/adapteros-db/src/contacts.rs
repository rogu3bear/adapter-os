use crate::new_id;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Builder for creating contact upsert parameters
#[derive(Debug, Default)]
pub struct ContactUpsertBuilder {
    tenant_id: Option<String>,
    name: Option<String>,
    category: Option<String>,
    email: Option<String>,
    role: Option<String>,
    metadata_json: Option<String>,
    discovered_by: Option<String>,
}

/// Parameters for contact upsert operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ContactUpsertParams {
    pub tenant_id: String,
    pub name: String,
    pub category: String,
    pub email: Option<String>,
    pub role: Option<String>,
    pub metadata_json: Option<String>,
    pub discovered_by: Option<String>,
}

impl ContactUpsertBuilder {
    /// Create a new contact upsert builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the contact name (required)
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the category (required)
    pub fn category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    /// Set the email (optional)
    pub fn email(mut self, email: Option<impl Into<String>>) -> Self {
        self.email = email.map(|s| s.into());
        self
    }

    /// Set the role (optional)
    pub fn role(mut self, role: Option<impl Into<String>>) -> Self {
        self.role = role.map(|s| s.into());
        self
    }

    /// Set the metadata JSON (optional)
    pub fn metadata_json(mut self, metadata_json: Option<impl Into<String>>) -> Self {
        self.metadata_json = metadata_json.map(|s| s.into());
        self
    }

    /// Set the discovered by field (optional)
    pub fn discovered_by(mut self, discovered_by: Option<impl Into<String>>) -> Self {
        self.discovered_by = discovered_by.map(|s| s.into());
        self
    }

    /// Build the contact upsert parameters
    pub fn build(self) -> Result<ContactUpsertParams> {
        Ok(ContactUpsertParams {
            tenant_id: self
                .tenant_id
                .ok_or_else(|| AosError::Validation("tenant_id is required".to_string()))?,
            name: self
                .name
                .ok_or_else(|| AosError::Validation("name is required".to_string()))?,
            category: self
                .category
                .ok_or_else(|| AosError::Validation("category is required".to_string()))?,
            email: self.email,
            role: self.role,
            metadata_json: self.metadata_json,
            discovered_by: self.discovered_by,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct Contact {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub email: Option<String>,
    pub category: String,
    pub role: Option<String>,
    pub metadata_json: Option<String>,
    pub avatar_url: Option<String>,
    pub discovered_at: String,
    pub discovered_by: Option<String>,
    pub last_interaction: Option<String>,
    pub interaction_count: i64,
    pub permissions_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ContactInteraction {
    pub id: String,
    pub contact_id: String,
    pub trace_id: String,
    pub cpid: String,
    pub interaction_type: String,
    pub context_json: Option<String>,
    pub created_at: String,
}

// Alias for backwards compatibility with worker code
pub type ContactStream = ContactInteraction;

impl Db {
    /// Upsert a contact (insert or update if exists)
    ///
    /// Use [`ContactUpsertBuilder`] to construct contact parameters:
    /// ```no_run
    /// use adapteros_db::contacts::ContactUpsertBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = ContactUpsertBuilder::new()
    ///     .tenant_id("tenant-123")
    ///     .name("John Doe")
    ///     .category("user")
    ///     .email(Some("john@example.com"))
    ///     .role(Some("developer"))
    ///     .metadata_json(Some(r#"{"department": "engineering"}"#))
    ///     .discovered_by(Some("trace-456"))
    ///     .build()
    ///     .expect("required fields");
    /// db.upsert_contact(params).await.expect("upsert succeeds");
    /// # }
    /// ```
    pub async fn upsert_contact(&self, params: ContactUpsertParams) -> Result<String> {
        // Try to get existing contact by tenant_id, name, and category
        let existing = sqlx::query(
            "SELECT id FROM contacts WHERE tenant_id = ? AND name = ? AND category = ?",
        )
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.category)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(row) = existing {
            let id: String = row
                .try_get("id")
                .map_err(|e| AosError::Database(e.to_string()))?;

            // Update existing contact
            sqlx::query(
                "UPDATE contacts SET
                 email = ?,
                 role = ?,
                 metadata_json = ?,
                 updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&params.email)
            .bind(&params.role)
            .bind(&params.metadata_json)
            .bind(&id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            Ok(id)
        } else {
            // Insert new contact
            let id = new_id(IdPrefix::Usr);
            sqlx::query(
                "INSERT INTO contacts (id, tenant_id, name, email, category, role, metadata_json, discovered_by)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&params.tenant_id)
            .bind(&params.name)
            .bind(&params.email)
            .bind(&params.category)
            .bind(&params.role)
            .bind(&params.metadata_json)
            .bind(&params.discovered_by)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            Ok(id)
        }
    }

    /// Get contact by name and tenant
    pub async fn get_contact_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<Contact>> {
        let contact = sqlx::query_as::<_, Contact>(
            "SELECT id, tenant_id, name, email, category, role, metadata_json, avatar_url,
                    discovered_at, discovered_by, last_interaction, interaction_count,
                    permissions_json, created_at, updated_at
             FROM contacts
             WHERE tenant_id = ? AND name = ?",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(contact)
    }

    /// Get contact by ID
    pub async fn get_contact(&self, id: &str) -> Result<Option<Contact>> {
        let contact = sqlx::query_as::<_, Contact>(
            "SELECT id, tenant_id, name, email, category, role, metadata_json, avatar_url,
                    discovered_at, discovered_by, last_interaction, interaction_count,
                    permissions_json, created_at, updated_at
             FROM contacts
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(contact)
    }

    /// List contacts for a tenant
    pub async fn list_contacts(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Contact>> {
        let contacts = sqlx::query_as::<_, Contact>(
            "SELECT id, tenant_id, name, email, category, role, metadata_json, avatar_url,
                    discovered_at, discovered_by, last_interaction, interaction_count,
                    permissions_json, created_at, updated_at
             FROM contacts
             WHERE tenant_id = ?
             ORDER BY last_interaction DESC, name ASC
             LIMIT ? OFFSET ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(contacts)
    }

    /// Log contact interaction
    pub async fn log_contact_interaction(
        &self,
        tenant_id: &str,
        contact_name: &str,
        trace_id: &str,
        cpid: &str,
        interaction_type: &str,
        context: Option<&serde_json::Value>,
    ) -> Result<String> {
        // Get or create contact
        let contact = self.get_contact_by_name(tenant_id, contact_name).await?;
        let contact_id = if let Some(c) = contact {
            c.id
        } else {
            // Create new contact if doesn't exist (default category: "user")
            let contact_params = ContactUpsertBuilder::new()
                .tenant_id(tenant_id)
                .name(contact_name)
                .category("user")
                .discovered_by(Some(trace_id))
                .build()?;
            self.upsert_contact(contact_params).await?
        };

        // Insert interaction entry
        let id = new_id(IdPrefix::Usr);
        let context_json = context
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| AosError::Validation(format!("Failed to serialize context: {}", e)))?;

        sqlx::query(
            "INSERT INTO contact_interactions (id, contact_id, trace_id, cpid, interaction_type, context_json)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&contact_id)
        .bind(trace_id)
        .bind(cpid)
        .bind(interaction_type)
        .bind(context_json)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    /// Get contact interactions
    pub async fn get_contact_interactions(
        &self,
        contact_id: &str,
        limit: i64,
    ) -> Result<Vec<ContactInteraction>> {
        let interactions = sqlx::query_as::<_, ContactInteraction>(
            "SELECT id, contact_id, trace_id, cpid, interaction_type, context_json, created_at
             FROM contact_interactions
             WHERE contact_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )
        .bind(contact_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(interactions)
    }

    /// Get recent contact activity for a tenant
    pub async fn get_recent_contact_activity(
        &self,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<(Contact, i64)>> {
        let rows = sqlx::query(
            "SELECT c.id, c.tenant_id, c.name, c.email, c.category, c.role,
                    c.metadata_json, c.avatar_url, c.discovered_at, c.discovered_by,
                    c.last_interaction, c.interaction_count, c.permissions_json,
                    c.created_at, c.updated_at,
                    COUNT(ci.id) as logged_interaction_count
             FROM contacts c
             LEFT JOIN contact_interactions ci ON c.id = ci.contact_id
             WHERE c.tenant_id = ?
             GROUP BY c.id
             ORDER BY c.last_interaction DESC, logged_interaction_count DESC
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let mut result = Vec::new();
        for row in rows {
            let contact = Contact {
                id: row
                    .try_get("id")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                tenant_id: row
                    .try_get("tenant_id")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                name: row
                    .try_get("name")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                email: row
                    .try_get("email")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                category: row
                    .try_get("category")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                role: row
                    .try_get("role")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                metadata_json: row
                    .try_get("metadata_json")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                avatar_url: row
                    .try_get("avatar_url")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                discovered_at: row
                    .try_get("discovered_at")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                discovered_by: row
                    .try_get("discovered_by")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                last_interaction: row
                    .try_get("last_interaction")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                interaction_count: row
                    .try_get("interaction_count")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                permissions_json: row
                    .try_get("permissions_json")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                created_at: row
                    .try_get("created_at")
                    .map_err(|e| AosError::Database(e.to_string()))?,
                updated_at: row
                    .try_get("updated_at")
                    .map_err(|e| AosError::Database(e.to_string()))?,
            };
            let count: i64 = row.try_get("logged_interaction_count").unwrap_or(0);
            result.push((contact, count));
        }

        Ok(result)
    }

    /// Delete contact (and associated interactions)
    pub async fn delete_contact(&self, id: &str) -> Result<()> {
        // Begin transaction for atomic multi-step deletion
        let mut tx = self.begin_write_tx().await?;

        // Delete interactions first (foreign key constraint)
        sqlx::query("DELETE FROM contact_interactions WHERE contact_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Delete contact
        sqlx::query("DELETE FROM contacts WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

        // Commit transaction
        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit transaction: {}", e)))?;

        Ok(())
    }

    /// Get contact interaction stats
    pub async fn get_contact_interaction_stats(
        &self,
        contact_id: &str,
    ) -> Result<(i64, Option<String>, Option<String>)> {
        let row = sqlx::query(
            "SELECT 
                COUNT(*) as total,
                MIN(created_at) as first_interaction,
                MAX(created_at) as last_interaction
             FROM contact_interactions
             WHERE contact_id = ?",
        )
        .bind(contact_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let total: i64 = row
            .try_get("total")
            .map_err(|e| AosError::Database(e.to_string()))?;
        let first: Option<String> = row.try_get("first_interaction").ok();
        let last: Option<String> = row.try_get("last_interaction").ok();

        Ok((total, first, last))
    }
}
