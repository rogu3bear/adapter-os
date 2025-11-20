use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceRole {
    #[serde(rename = "owner")]
    Owner,
    #[serde(rename = "member")]
    Member,
    #[serde(rename = "viewer")]
    Viewer,
}

impl std::fmt::Display for WorkspaceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceRole::Owner => write!(f, "owner"),
            WorkspaceRole::Member => write!(f, "member"),
            WorkspaceRole::Viewer => write!(f, "viewer"),
        }
    }
}

impl std::str::FromStr for WorkspaceRole {
    type Err = adapteros_core::AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "owner" => Ok(WorkspaceRole::Owner),
            "member" => Ok(WorkspaceRole::Member),
            "viewer" => Ok(WorkspaceRole::Viewer),
            _ => Err(AosError::Parse(format!("invalid workspace role: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceMember {
    pub id: String,
    pub workspace_id: String,
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub role: String,
    pub permissions_json: Option<String>,
    pub added_by: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResourceType {
    #[serde(rename = "adapter")]
    Adapter,
    #[serde(rename = "node")]
    Node,
    #[serde(rename = "model")]
    Model,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Adapter => write!(f, "adapter"),
            ResourceType::Node => write!(f, "node"),
            ResourceType::Model => write!(f, "model"),
        }
    }
}

impl std::str::FromStr for ResourceType {
    type Err = adapteros_core::AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "adapter" => Ok(ResourceType::Adapter),
            "node" => Ok(ResourceType::Node),
            "model" => Ok(ResourceType::Model),
            _ => Err(AosError::Parse(format!("invalid resource type: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WorkspaceResource {
    pub id: String,
    pub workspace_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub shared_by: String,
    pub shared_by_tenant_id: String,
    pub shared_at: String,
}

impl Db {
    // Workspace CRUD operations

    pub async fn create_workspace(
        &self,
        name: &str,
        description: Option<&str>,
        created_by: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO workspaces (id, name, description, created_by) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(created_by)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let workspace = sqlx::query_as::<_, Workspace>(
            "SELECT id, name, description, created_by, created_at, updated_at FROM workspaces WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(workspace)
    }

    pub async fn update_workspace(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let mut query = String::from("UPDATE workspaces SET updated_at = datetime('now')");
        let mut params: Vec<String> = vec![];

        if let Some(n) = name {
            query.push_str(", name = ?");
            params.push(n.to_string());
        }

        if let Some(d) = description {
            query.push_str(", description = ?");
            params.push(d.to_string());
        }

        query.push_str(" WHERE id = ?");
        let mut query_builder = sqlx::query(&query);

        for param in params {
            query_builder = query_builder.bind(param);
        }
        query_builder = query_builder.bind(id);

        query_builder.execute(self.pool()).await?;
        Ok(())
    }

    pub async fn delete_workspace(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let workspaces = sqlx::query_as::<_, Workspace>(
            "SELECT id, name, description, created_by, created_at, updated_at FROM workspaces ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(workspaces)
    }

    pub async fn list_user_workspaces(
        &self,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<Vec<Workspace>> {
        // Get workspaces where user is a member (either directly or via tenant membership)
        let workspaces = sqlx::query_as::<_, Workspace>(
            r#"
            SELECT DISTINCT w.id, w.name, w.description, w.created_by, w.created_at, w.updated_at
            FROM workspaces w
            INNER JOIN workspace_members wm ON w.id = wm.workspace_id
            WHERE (wm.user_id = ? OR (wm.user_id IS NULL AND wm.tenant_id = ?))
            ORDER BY w.created_at DESC
            "#,
        )
        .bind(user_id)
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await?;
        Ok(workspaces)
    }

    // Workspace membership operations

    pub async fn add_workspace_member(
        &self,
        workspace_id: &str,
        tenant_id: &str,
        user_id: Option<&str>,
        role: WorkspaceRole,
        permissions_json: Option<&str>,
        added_by: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let role_str = role.to_string();
        sqlx::query(
            r#"
            INSERT INTO workspace_members (id, workspace_id, tenant_id, user_id, role, permissions_json, added_by)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(&role_str)
        .bind(permissions_json)
        .bind(added_by)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_workspace_member(
        &self,
        workspace_id: &str,
        tenant_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<WorkspaceMember>> {
        let member = sqlx::query_as::<_, WorkspaceMember>(
            r#"
            SELECT id, workspace_id, tenant_id, user_id, role, permissions_json, added_by, added_at
            FROM workspace_members
            WHERE workspace_id = ? AND tenant_id = ? AND (user_id = ? OR (user_id IS NULL AND ? IS NULL))
            "#,
        )
        .bind(workspace_id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(user_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(member)
    }

    pub async fn list_workspace_members(&self, workspace_id: &str) -> Result<Vec<WorkspaceMember>> {
        let members = sqlx::query_as::<_, WorkspaceMember>(
            r#"
            SELECT id, workspace_id, tenant_id, user_id, role, permissions_json, added_by, added_at
            FROM workspace_members
            WHERE workspace_id = ?
            ORDER BY added_at DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(self.pool())
        .await?;
        Ok(members)
    }

    pub async fn update_workspace_member_role(
        &self,
        workspace_id: &str,
        tenant_id: &str,
        user_id: Option<&str>,
        role: WorkspaceRole,
    ) -> Result<()> {
        let role_str = role.to_string();
        sqlx::query(
            r#"
            UPDATE workspace_members
            SET role = ?
            WHERE workspace_id = ? AND tenant_id = ? AND (user_id = ? OR (user_id IS NULL AND ? IS NULL))
            "#,
        )
        .bind(&role_str)
        .bind(workspace_id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(user_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn remove_workspace_member(
        &self,
        workspace_id: &str,
        tenant_id: &str,
        user_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            DELETE FROM workspace_members
            WHERE workspace_id = ? AND tenant_id = ? AND (user_id = ? OR (user_id IS NULL AND ? IS NULL))
            "#,
        )
        .bind(workspace_id)
        .bind(tenant_id)
        .bind(user_id)
        .bind(user_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    // Permission checking

    pub async fn check_workspace_access(
        &self,
        workspace_id: &str,
        user_id: &str,
        tenant_id: &str,
    ) -> Result<Option<WorkspaceRole>> {
        // Check if user has direct membership or tenant-wide membership
        let row = sqlx::query(
            r#"
            SELECT role
            FROM workspace_members
            WHERE workspace_id = ? AND tenant_id = ?
            AND (user_id = ? OR user_id IS NULL)
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(self.pool())
        .await?;

        if let Some(row) = row {
            let role_str: String = row.get(0);
            Ok(Some(WorkspaceRole::from_str(&role_str)?))
        } else {
            Ok(None)
        }
    }

    // Workspace resource operations

    pub async fn add_workspace_resource(
        &self,
        workspace_id: &str,
        resource_type: ResourceType,
        resource_id: &str,
        shared_by: &str,
        shared_by_tenant_id: &str,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let resource_type_str = resource_type.to_string();
        sqlx::query(
            r#"
            INSERT INTO workspace_resources (id, workspace_id, resource_type, resource_id, shared_by, shared_by_tenant_id)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(&resource_type_str)
        .bind(resource_id)
        .bind(shared_by)
        .bind(shared_by_tenant_id)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn list_workspace_resources(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<WorkspaceResource>> {
        let resources = sqlx::query_as::<_, WorkspaceResource>(
            r#"
            SELECT id, workspace_id, resource_type, resource_id, shared_by, shared_by_tenant_id, shared_at
            FROM workspace_resources
            WHERE workspace_id = ?
            ORDER BY shared_at DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(self.pool())
        .await?;
        Ok(resources)
    }

    pub async fn remove_workspace_resource(
        &self,
        workspace_id: &str,
        resource_type: ResourceType,
        resource_id: &str,
    ) -> Result<()> {
        let resource_type_str = resource_type.to_string();
        sqlx::query(
            r#"
            DELETE FROM workspace_resources
            WHERE workspace_id = ? AND resource_type = ? AND resource_id = ?
            "#,
        )
        .bind(workspace_id)
        .bind(&resource_type_str)
        .bind(resource_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
