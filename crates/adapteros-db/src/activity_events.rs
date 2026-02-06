use crate::query_helpers::{db_err, FilterBuilder};
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityEventType {
    // Adapter events
    AdapterCreated,
    AdapterUpdated,
    AdapterDeleted,
    AdapterShared,
    AdapterUnshared,
    AdapterPromoted,
    AdapterPinned,
    AdapterUnpinned,
    AdapterSwapped,

    // Resource sharing
    ResourceShared,
    ResourceUnshared,

    // Messaging
    MessageSent,
    MessageEdited,
    UserMentioned,

    // Workspace membership
    UserJoinedWorkspace,
    UserLeftWorkspace,
    WorkspaceCreated,
    WorkspaceUpdated,
    WorkspaceDeleted,
    MemberAdded,
    MemberRemoved,
    MemberRoleChanged,

    // Code/Repo events
    RepoScanTriggered,
    RepoReportViewed,

    // Training events
    TrainingSessionStarted,
    TrainingSessionCancelled,
    TrainingSessionCompleted,

    // Document events
    DocumentUploaded,
    DocumentDeleted,

    // Auth events
    UserLoggedIn,
}

impl std::fmt::Display for ActivityEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Adapter events
            ActivityEventType::AdapterCreated => write!(f, "adapter_created"),
            ActivityEventType::AdapterUpdated => write!(f, "adapter_updated"),
            ActivityEventType::AdapterDeleted => write!(f, "adapter_deleted"),
            ActivityEventType::AdapterShared => write!(f, "adapter_shared"),
            ActivityEventType::AdapterUnshared => write!(f, "adapter_unshared"),
            ActivityEventType::AdapterPromoted => write!(f, "adapter_promoted"),
            ActivityEventType::AdapterPinned => write!(f, "adapter_pinned"),
            ActivityEventType::AdapterUnpinned => write!(f, "adapter_unpinned"),
            ActivityEventType::AdapterSwapped => write!(f, "adapter_swapped"),
            // Resource sharing
            ActivityEventType::ResourceShared => write!(f, "resource_shared"),
            ActivityEventType::ResourceUnshared => write!(f, "resource_unshared"),
            // Messaging
            ActivityEventType::MessageSent => write!(f, "message_sent"),
            ActivityEventType::MessageEdited => write!(f, "message_edited"),
            ActivityEventType::UserMentioned => write!(f, "user_mentioned"),
            // Workspace membership
            ActivityEventType::UserJoinedWorkspace => write!(f, "user_joined_workspace"),
            ActivityEventType::UserLeftWorkspace => write!(f, "user_left_workspace"),
            ActivityEventType::WorkspaceCreated => write!(f, "workspace_created"),
            ActivityEventType::WorkspaceUpdated => write!(f, "workspace_updated"),
            ActivityEventType::WorkspaceDeleted => write!(f, "workspace_deleted"),
            ActivityEventType::MemberAdded => write!(f, "member_added"),
            ActivityEventType::MemberRemoved => write!(f, "member_removed"),
            ActivityEventType::MemberRoleChanged => write!(f, "member_role_changed"),
            // Code/Repo events
            ActivityEventType::RepoScanTriggered => write!(f, "repo_scan_triggered"),
            ActivityEventType::RepoReportViewed => write!(f, "repo_report_viewed"),
            // Training events
            ActivityEventType::TrainingSessionStarted => write!(f, "training_session_started"),
            ActivityEventType::TrainingSessionCancelled => write!(f, "training_session_cancelled"),
            ActivityEventType::TrainingSessionCompleted => write!(f, "training_session_completed"),
            // Document events
            ActivityEventType::DocumentUploaded => write!(f, "document_uploaded"),
            ActivityEventType::DocumentDeleted => write!(f, "document_deleted"),
            // Auth events
            ActivityEventType::UserLoggedIn => write!(f, "user_logged_in"),
        }
    }
}

impl std::str::FromStr for ActivityEventType {
    type Err = adapteros_core::AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            // Adapter events
            "adapter_created" => Ok(ActivityEventType::AdapterCreated),
            "adapter_updated" => Ok(ActivityEventType::AdapterUpdated),
            "adapter_deleted" => Ok(ActivityEventType::AdapterDeleted),
            "adapter_shared" => Ok(ActivityEventType::AdapterShared),
            "adapter_unshared" => Ok(ActivityEventType::AdapterUnshared),
            "adapter_promoted" => Ok(ActivityEventType::AdapterPromoted),
            "adapter_pinned" => Ok(ActivityEventType::AdapterPinned),
            "adapter_unpinned" => Ok(ActivityEventType::AdapterUnpinned),
            "adapter_swapped" => Ok(ActivityEventType::AdapterSwapped),
            // Resource sharing
            "resource_shared" => Ok(ActivityEventType::ResourceShared),
            "resource_unshared" => Ok(ActivityEventType::ResourceUnshared),
            // Messaging
            "message_sent" => Ok(ActivityEventType::MessageSent),
            "message_edited" => Ok(ActivityEventType::MessageEdited),
            "user_mentioned" => Ok(ActivityEventType::UserMentioned),
            // Workspace membership
            "user_joined_workspace" => Ok(ActivityEventType::UserJoinedWorkspace),
            "user_left_workspace" => Ok(ActivityEventType::UserLeftWorkspace),
            "workspace_created" => Ok(ActivityEventType::WorkspaceCreated),
            "workspace_updated" => Ok(ActivityEventType::WorkspaceUpdated),
            "workspace_deleted" => Ok(ActivityEventType::WorkspaceDeleted),
            "member_added" => Ok(ActivityEventType::MemberAdded),
            "member_removed" => Ok(ActivityEventType::MemberRemoved),
            "member_role_changed" => Ok(ActivityEventType::MemberRoleChanged),
            // Code/Repo events
            "repo_scan_triggered" => Ok(ActivityEventType::RepoScanTriggered),
            "repo_report_viewed" => Ok(ActivityEventType::RepoReportViewed),
            // Training events
            "training_session_started" => Ok(ActivityEventType::TrainingSessionStarted),
            "training_session_cancelled" => Ok(ActivityEventType::TrainingSessionCancelled),
            "training_session_completed" => Ok(ActivityEventType::TrainingSessionCompleted),
            // Document events
            "document_uploaded" => Ok(ActivityEventType::DocumentUploaded),
            "document_deleted" => Ok(ActivityEventType::DocumentDeleted),
            // Auth events
            "user_logged_in" => Ok(ActivityEventType::UserLoggedIn),
            _ => Err(AosError::Parse(format!(
                "invalid activity event type: {}",
                s
            ))),
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
        let id = new_id(IdPrefix::Evt);
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
        .await
        .map_err(|e| AosError::Database(format!("failed to create activity event: {}", e)))?;
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
        .await
        .map_err(|e| AosError::Database(format!("failed to fetch activity event '{}': {}", id, e)))?;
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

        let mut builder = FilterBuilder::new(
            r#"
            SELECT id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json, created_at
            FROM activity_events
            WHERE 1=1
            "#,
        );

        builder.add_filter("workspace_id", workspace_id);
        builder.add_filter("user_id", user_id);
        builder.add_filter("tenant_id", tenant_id);
        builder.add_filter("event_type", event_type.as_ref().map(|e| e.to_string()));

        builder.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
        builder.add_param(limit);
        builder.add_param(offset);

        let mut q = sqlx::query_as::<_, ActivityEvent>(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        let events = q
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list activity events"))?;
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
        .await
        .map_err(|e| AosError::Database(format!("failed to list user workspace activity for user '{}' in tenant '{}': {}", user_id, tenant_id, e)))?;
        Ok(events)
    }

    /// List activity events for a workspace created after a given timestamp (delta mode for SSE streaming).
    /// Returns events ordered by created_at ASC so clients process them in chronological order.
    pub async fn list_activity_events_since(
        &self,
        workspace_id: &str,
        since_timestamp: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ActivityEvent>> {
        let limit = limit.unwrap_or(50);

        let events = if let Some(since_ts) = since_timestamp {
            sqlx::query_as::<_, ActivityEvent>(
                r#"
                SELECT id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json, created_at
                FROM activity_events
                WHERE workspace_id = ? AND created_at > ?
                ORDER BY created_at ASC
                LIMIT ?
                "#,
            )
            .bind(workspace_id)
            .bind(since_ts)
            .bind(limit)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("failed to list activity events since '{}' for workspace '{}': {}", since_ts, workspace_id, e)))?
        } else {
            // No since_timestamp: return most recent events
            sqlx::query_as::<_, ActivityEvent>(
                r#"
                SELECT id, workspace_id, user_id, tenant_id, event_type, target_type, target_id, metadata_json, created_at
                FROM activity_events
                WHERE workspace_id = ?
                ORDER BY created_at DESC
                LIMIT ?
                "#,
            )
            .bind(workspace_id)
            .bind(limit)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("failed to list activity events for workspace '{}': {}", workspace_id, e)))?
        };

        Ok(events)
    }
}
