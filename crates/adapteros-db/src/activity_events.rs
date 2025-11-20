use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityEventType {
    AdapterCreated,
    AdapterUpdated,
    AdapterDeleted,
    AdapterShared,
    AdapterUnshared,
    ResourceShared,
    ResourceUnshared,
    MessageSent,
    MessageEdited,
    UserMentioned,
    UserJoinedWorkspace,
    UserLeftWorkspace,
    WorkspaceCreated,
    WorkspaceUpdated,
    MemberAdded,
    MemberRemoved,
    MemberRoleChanged,
    RepoScanTriggered,
    RepoReportViewed,
    TrainingSessionStarted,
}

impl std::fmt::Display for ActivityEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActivityEventType::AdapterCreated => write!(f, "adapter_created"),
            ActivityEventType::AdapterUpdated => write!(f, "adapter_updated"),
            ActivityEventType::AdapterDeleted => write!(f, "adapter_deleted"),
            ActivityEventType::AdapterShared => write!(f, "adapter_shared"),
            ActivityEventType::AdapterUnshared => write!(f, "adapter_unshared"),
            ActivityEventType::ResourceShared => write!(f, "resource_shared"),
            ActivityEventType::ResourceUnshared => write!(f, "resource_unshared"),
            ActivityEventType::MessageSent => write!(f, "message_sent"),
            ActivityEventType::MessageEdited => write!(f, "message_edited"),
            ActivityEventType::UserMentioned => write!(f, "user_mentioned"),
            ActivityEventType::UserJoinedWorkspace => write!(f, "user_joined_workspace"),
            ActivityEventType::UserLeftWorkspace => write!(f, "user_left_workspace"),
            ActivityEventType::WorkspaceCreated => write!(f, "workspace_created"),
            ActivityEventType::WorkspaceUpdated => write!(f, "workspace_updated"),
            ActivityEventType::MemberAdded => write!(f, "member_added"),
            ActivityEventType::MemberRemoved => write!(f, "member_removed"),
            ActivityEventType::MemberRoleChanged => write!(f, "member_role_changed"),
            ActivityEventType::RepoScanTriggered => write!(f, "repo_scan_triggered"),
            ActivityEventType::RepoReportViewed => write!(f, "repo_report_viewed"),
            ActivityEventType::TrainingSessionStarted => write!(f, "training_session_started"),
        }
    }
}

impl std::str::FromStr for ActivityEventType {
    type Err = adapteros_core::AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "adapter_created" => Ok(ActivityEventType::AdapterCreated),
            "adapter_updated" => Ok(ActivityEventType::AdapterUpdated),
            "adapter_deleted" => Ok(ActivityEventType::AdapterDeleted),
            "adapter_shared" => Ok(ActivityEventType::AdapterShared),
            "adapter_unshared" => Ok(ActivityEventType::AdapterUnshared),
            "resource_shared" => Ok(ActivityEventType::ResourceShared),
            "resource_unshared" => Ok(ActivityEventType::ResourceUnshared),
            "message_sent" => Ok(ActivityEventType::MessageSent),
            "message_edited" => Ok(ActivityEventType::MessageEdited),
            "user_mentioned" => Ok(ActivityEventType::UserMentioned),
            "user_joined_workspace" => Ok(ActivityEventType::UserJoinedWorkspace),
            "user_left_workspace" => Ok(ActivityEventType::UserLeftWorkspace),
            "workspace_created" => Ok(ActivityEventType::WorkspaceCreated),
            "workspace_updated" => Ok(ActivityEventType::WorkspaceUpdated),
            "member_added" => Ok(ActivityEventType::MemberAdded),
            "member_removed" => Ok(ActivityEventType::MemberRemoved),
            "member_role_changed" => Ok(ActivityEventType::MemberRoleChanged),
            "repo_scan_triggered" => Ok(ActivityEventType::RepoScanTriggered),
            "repo_report_viewed" => Ok(ActivityEventType::RepoReportViewed),
            "training_session_started" => Ok(ActivityEventType::TrainingSessionStarted),
            _ => Err(AosError::Parse(format!("invalid activity event type: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ActivityEvent {
    pub id: String,
    pub workspace_id: Option<String>,
    pub user_id: String,
    pub tenant_id: String,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

impl Db {
    pub async fn create_activity_event(
        &self,
        workspace_id: Option<&str>,
        user_id: &str,
        tenant_id: &str,
        event_type: ActivityEventType,
        target_type: Option<&str>,
        target_id: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let event_type_str = event_type.to_string();
        sqlx::query(
            r#"
            INSERT INTO activity_events (id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(workspace_id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(&event_type_str)
        .bind(target_type)
        .bind(target_id)
        .bind(metadata_json)
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    pub async fn get_activity_event(&self, id: &str) -> Result<Option<ActivityEvent>> {
        let event = sqlx::query_as::<_, ActivityEvent>(
            r#"
            SELECT id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json, created_at
            FROM activity_events
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await?;
        Ok(event)
    }

    pub async fn list_activity_events(
        &self,
        workspace_id: Option<&str>,
        user_id: Option<&str>,
        tenant_id: Option<&str>,
        event_type: Option<ActivityEventType>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<ActivityEvent>> {
        let limit = limit.unwrap_or(50);
        let offset = offset.unwrap_or(0);
        let mut query = String::from(
            r#"
            SELECT id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json, created_at
            FROM activity_events
            WHERE 1=1
            "#,
        );

        if workspace_id.is_some() {
            query.push_str(" AND workspace_id = ?");
        }

        if user_id.is_some() {
            query.push_str(" AND user_id = ?");
        }

        if tenant_id.is_some() {
            query.push_str(" AND tenant_id = ?");
        }

        if event_type.is_some() {
            query.push_str(" AND event_type = ?");
        }

        query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let mut query_builder = sqlx::query_as::<_, ActivityEvent>(&query);

        if let Some(ws_id) = workspace_id {
            query_builder = query_builder.bind(ws_id);
        }

        if let Some(u_id) = user_id {
            query_builder = query_builder.bind(u_id);
        }

        if let Some(t_id) = tenant_id {
            query_builder = query_builder.bind(t_id);
        }

        if let Some(ref e_type) = event_type {
            query_builder = query_builder.bind(e_type.to_string());
        }

        query_builder = query_builder.bind(limit).bind(offset);

        let events = query_builder.fetch_all(self.pool()).await?;
        Ok(events)
    }

    pub async fn list_user_workspace_activity(
        &self,
        user_id: &str,
        tenant_id: &str,
        _workspace_ids: &[&str],
        limit: Option<i64>,
    ) -> Result<Vec<ActivityEvent>> {
        let limit = limit.unwrap_or(50);
        // Get activity from workspaces user is a member of, or tenant-wide activity
        let events = sqlx::query_as::<_, ActivityEvent>(
            r#"
            SELECT id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json, created_at
            FROM activity_events
            WHERE (workspace_id IN (
                SELECT workspace_id FROM workspace_members
                WHERE tenant_id = ? AND (user_id = ? OR user_id IS NULL)
            ) OR workspace_id IS NULL)
            AND tenant_id = ?
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(tenant_id)
        .bind(user_id)
        .bind(tenant_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await?;
        Ok(events)
    }
}
