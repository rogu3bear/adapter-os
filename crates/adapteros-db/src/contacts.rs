use crate::Db;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    pub async fn upsert_contact(
        &self,
        tenant_id: &str,
        name: &str,
        category: &str,
        email: Option<&str>,
        role: Option<&str>,
        metadata_json: Option<&str>,
        discovered_by: Option<&str>,
    ) -> Result<String> {
        // Try to get existing contact by tenant_id, name, and category
        let existing = sqlx::query(
            "SELECT id FROM contacts WHERE tenant_id = ? AND name = ? AND category = ?",
        )
        .bind(tenant_id)
        .bind(name)
        .bind(category)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = existing {
            let id: String = row.try_get("id")?;

            // Update existing contact
            sqlx::query(
                "UPDATE contacts SET 
                 email = ?,
                 role = ?,
                 metadata_json = ?,
                 updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(email)
            .bind(role)
            .bind(metadata_json)
            .bind(&id)
            .execute(self.pool())
            .await?;

            Ok(id)
        } else {
            // Insert new contact
            let id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO contacts (id, tenant_id, name, email, category, role, metadata_json, discovered_by)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(tenant_id)
            .bind(name)
            .bind(email)
            .bind(category)
            .bind(role)
            .bind(metadata_json)
            .bind(discovered_by)
            .execute(self.pool())
            .await?;

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
        .await?;

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
        .await?;

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
        .await?;

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
            self.upsert_contact(
                tenant_id,
                contact_name,
                "user",
                None,
                None,
                None,
                Some(trace_id),
            )
            .await?
        };

        // Insert interaction entry
        let id = Uuid::now_v7().to_string();
        let context_json = context.map(serde_json::to_string).transpose()?;

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
        .await?;

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
        .await?;

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
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let contact = Contact {
                id: row.try_get("id")?,
                tenant_id: row.try_get("tenant_id")?,
                name: row.try_get("name")?,
                email: row.try_get("email")?,
                category: row.try_get("category")?,
                role: row.try_get("role")?,
                metadata_json: row.try_get("metadata_json")?,
                avatar_url: row.try_get("avatar_url")?,
                discovered_at: row.try_get("discovered_at")?,
                discovered_by: row.try_get("discovered_by")?,
                last_interaction: row.try_get("last_interaction")?,
                interaction_count: row.try_get("interaction_count")?,
                permissions_json: row.try_get("permissions_json")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            };
            let count: i64 = row.try_get("logged_interaction_count").unwrap_or(0);
            result.push((contact, count));
        }

        Ok(result)
    }

    /// Delete contact (and associated interactions)
    pub async fn delete_contact(&self, id: &str) -> Result<()> {
        // Delete interactions first (foreign key constraint)
        sqlx::query("DELETE FROM contact_interactions WHERE contact_id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;

        // Delete contact
        sqlx::query("DELETE FROM contacts WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;

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
        .await?;

        let total: i64 = row.try_get("total")?;
        let first: Option<String> = row.try_get("first_interaction").ok();
        let last: Option<String> = row.try_get("last_interaction").ok();

        Ok((total, first, last))
    }
}
