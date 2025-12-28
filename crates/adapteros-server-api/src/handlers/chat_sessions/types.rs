//! Request/response types for chat session handlers
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_types】

use adapteros_db::ChatMessage;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// Request to create a new chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Response for chat session creation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatSessionResponse {
    pub session_id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags_json: Option<String>,
    pub created_at: String,
}

/// Request to update an existing chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateChatSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub stack_id: Option<Option<String>>,
    #[serde(default)]
    pub collection_id: Option<Option<String>>,
    #[serde(default)]
    pub document_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<Option<String>>,
    #[serde(default)]
    pub tags_json: Option<Option<String>>,
}

/// Request to add a message to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddChatMessageRequest {
    pub role: String, // 'user', 'assistant', 'system', 'tool', 'owner_system'
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// API response wrapper for ChatMessage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatMessageResponse {
    pub id: String,
    pub session_id: String,
    pub tenant_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub created_at: String,
    pub sequence: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

impl From<ChatMessage> for ChatMessageResponse {
    fn from(msg: ChatMessage) -> Self {
        let timestamp = msg
            .timestamp
            .clone()
            .unwrap_or_else(|| msg.created_at.clone());
        Self {
            id: msg.id,
            session_id: msg.session_id,
            tenant_id: msg.tenant_id,
            role: msg.role,
            content: msg.content,
            timestamp,
            created_at: msg.created_at,
            sequence: msg.sequence,
            metadata_json: msg.metadata_json,
        }
    }
}

/// Query parameters for listing sessions
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListSessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
}

/// Request to update collection binding for a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCollectionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
}

/// Request to create a new tag
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to update a tag
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTagRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to assign tags to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssignTagsRequest {
    pub tag_ids: Vec<String>,
}

/// Request to create a category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to update a category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to set session category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SetCategoryRequest {
    pub category_id: Option<String>,
}

/// Request to archive a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArchiveSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Query parameters for listing archived sessions
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListArchivedQuery {
    pub limit: Option<i64>,
}

/// Query parameters for session search
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct SearchSessionsQuery {
    pub q: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub category_id: Option<String>,
    pub tags: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_scope() -> String {
    "all".to_string()
}
fn default_limit() -> i64 {
    20
}

/// Request to share a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShareSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Request to fork a chat session
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ForkChatSessionRequest {
    /// Optional name for the forked session (defaults to "{original_name} (forked)")
    #[serde(default)]
    pub name: Option<String>,

    /// Whether to copy messages from the source session (default: true)
    #[serde(default = "default_include_messages")]
    pub include_messages: bool,
}

fn default_include_messages() -> bool {
    true
}

/// Response from forking a chat session
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ForkChatSessionResponse {
    pub session_id: String,
    pub name: String,
    pub created_at: String,
    pub forked_from: ForkedFromInfo,
}

/// Information about the source session
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ForkedFromInfo {
    pub session_id: String,
    pub name: String,
}

/// Helper struct for querying adapter provenance data
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)] // base_model_id is queried for completeness but derived from training_job
pub struct AdapterProvenanceRow {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub tier: String,
    pub training_job_id: Option<String>,
    pub base_model_id: Option<String>,
    pub created_at: String,
}

/// Helper struct for querying base model data
#[derive(Debug, sqlx::FromRow)]
pub struct BaseModelRow {
    pub id: String,
    pub name: String,
    pub hash_b3: String,
    pub created_at: String,
}
