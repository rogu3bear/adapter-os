//! Minimal KV storage for chat sessions, messages, and activity ordering.
//!
//! Keys (per-tenant namespace unless noted):
//! - `tenant/{tenant_id}/chat_session/{id}` -> ChatSessionKv (JSON)
//! - `tenant/{tenant_id}/chat_sessions` -> Vec<session_id> (ordering source)
//! - `tenant/{tenant_id}/chat_sessions/user/{user_id}` -> Vec<session_id>
//! - `tenant/{tenant_id}/chat_session/{id}/messages` -> Vec<message_id>
//! - `tenant/{tenant_id}/chat_session/{id}/message/{message_id}` -> ChatMessageKv (JSON)
//! - `chat-session-lookup/{id}` -> tenant_id (cross-tenant lookup)

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatSessionKv {
    pub id: String,
    pub tenant_id: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub document_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_ref_id: Option<String>,
    pub created_at: String,
    #[serde(default = "ChatSessionKv::default_timestamp")]
    pub updated_at: String,
    pub last_activity_at: String,
    #[serde(default)]
    pub metadata_json: Option<String>,
    #[serde(default)]
    pub tags_json: Option<String>,
    #[serde(default)]
    pub pinned_adapter_ids: Option<String>,
    /// Exclusive codebase adapter bound to this session
    #[serde(default)]
    pub codebase_adapter_id: Option<String>,
    pub status: String,
    /// Timestamp when session was archived (if applicable)
    #[serde(default)]
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatMessageKv {
    pub id: String,
    pub session_id: String,
    #[serde(default)]
    pub tenant_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    #[serde(default = "ChatMessageKv::default_created_at")]
    pub created_at: String,
    #[serde(default)]
    pub sequence: i64,
    #[serde(default)]
    pub metadata_json: Option<String>,
}

impl ChatSessionKv {
    fn default_timestamp() -> String {
        ChatSessionKvRepository::now()
    }
}

impl ChatMessageKv {
    fn default_created_at() -> String {
        ChatSessionKvRepository::now()
    }
}

pub struct ChatSessionKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl ChatSessionKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn now() -> String {
        Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn session_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{}/chat_session/{}", tenant_id, id)
    }

    fn session_index_key(tenant_id: &str) -> String {
        format!("tenant/{}/chat_sessions", tenant_id)
    }

    fn user_index_key(tenant_id: &str, user_id: &str) -> String {
        format!("tenant/{}/chat_sessions/user/{}", tenant_id, user_id)
    }

    fn messages_index_key(tenant_id: &str, session_id: &str) -> String {
        format!("tenant/{}/chat_session/{}/messages", tenant_id, session_id)
    }

    fn message_key(tenant_id: &str, session_id: &str, message_id: &str) -> String {
        format!(
            "tenant/{}/chat_session/{}/message/{}",
            tenant_id, session_id, message_id
        )
    }

    fn session_lookup_key(id: &str) -> String {
        format!("chat-session-lookup/{}", id)
    }

    async fn append_index(&self, tenant_id: &str, id: &str) -> Result<()> {
        let key = Self::session_index_key(tenant_id);
        let mut ids: Vec<String> =
            match self.backend.get(&key).await.map_err(|e| {
                AosError::Database(format!("Failed to read chat session index: {}", e))
            })? {
                Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
                None => Vec::new(),
            };
        if !ids.contains(&id.to_string()) {
            ids.push(id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend.set(&key, payload).await.map_err(|e| {
                AosError::Database(format!("Failed to update chat session index: {}", e))
            })?;
        }
        Ok(())
    }

    async fn append_user_index(
        &self,
        tenant_id: &str,
        user_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let key = Self::user_index_key(tenant_id, user_id);
        let mut ids: Vec<String> = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chat user index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        if !ids.contains(&session_id.to_string()) {
            ids.push(session_id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend.set(&key, payload).await.map_err(|e| {
                AosError::Database(format!("Failed to update chat user index: {}", e))
            })?;
        }
        Ok(())
    }

    async fn remove_user_index(
        &self,
        tenant_id: &str,
        user_id: &str,
        session_id: &str,
    ) -> Result<()> {
        let key = Self::user_index_key(tenant_id, user_id);
        if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chat user index: {}", e)))?
        {
            let mut ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            ids.retain(|v| v != session_id);
            if ids.is_empty() {
                let _ = self.backend.delete(&key).await;
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend.set(&key, payload).await.map_err(|e| {
                    AosError::Database(format!("Failed to update chat user index: {}", e))
                })?;
            }
        }
        Ok(())
    }

    /// Create a new chat session.
    pub async fn create_chat_session(
        &self,
        params: &crate::chat_sessions::CreateChatSessionParams,
    ) -> Result<String> {
        let now = Self::now();
        let title = params.title.clone().or_else(|| Some(params.name.clone()));
        let created_by = params.created_by.clone().or_else(|| params.user_id.clone());
        let source_type = params
            .source_type
            .clone()
            .unwrap_or_else(|| "general".to_string());
        let session = ChatSessionKv {
            id: params.id.clone(),
            tenant_id: params.tenant_id.clone(),
            user_id: params.user_id.clone(),
            created_by,
            stack_id: params.stack_id.clone(),
            collection_id: params.collection_id.clone(),
            document_id: params.document_id.clone(),
            name: params.name.clone(),
            title,
            source_type: Some(source_type),
            source_ref_id: params.source_ref_id.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            last_activity_at: now,
            metadata_json: params.metadata_json.clone(),
            tags_json: params.tags_json.clone(),
            pinned_adapter_ids: params.pinned_adapter_ids.clone(),
            codebase_adapter_id: params.codebase_adapter_id.clone(),
            status: "active".to_string(),
            archived_at: None,
        };
        let payload = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(&session.tenant_id, &session.id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chat session: {}", e)))?;
        self.backend
            .set(
                &Self::session_lookup_key(&session.id),
                session.tenant_id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to store session lookup: {}", e)))?;
        self.append_index(&session.tenant_id, &session.id).await?;
        if let Some(uid) = &session.user_id {
            self.append_user_index(&session.tenant_id, uid, &session.id)
                .await?;
        }
        Ok(session.id)
    }

    pub async fn get_chat_session(&self, session_id: &str) -> Result<Option<ChatSessionKv>> {
        let Some(tenant_bytes) = self
            .backend
            .get(&Self::session_lookup_key(session_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read session lookup: {}", e)))?
        else {
            return Ok(None);
        };
        let tenant_id = String::from_utf8(tenant_bytes).unwrap_or_default();
        let Some(bytes) = self
            .backend
            .get(&Self::session_key(&tenant_id, session_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chat session: {}", e)))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn list_chat_sessions(
        &self,
        tenant_id: &str,
        user_id: Option<&str>,
        source_type: Option<&str>,
        document_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ChatSessionKv>> {
        let ids: Vec<String> = if let Some(uid) = user_id {
            match self
                .backend
                .get(&Self::user_index_key(tenant_id, uid))
                .await
                .map_err(|e| AosError::Database(format!("Failed to read chat user index: {}", e)))?
            {
                Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
                None => Vec::new(),
            }
        } else {
            match self
                .backend
                .get(&Self::session_index_key(tenant_id))
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to read chat session index: {}", e))
                })? {
                Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
                None => Vec::new(),
            }
        };

        let mut sessions = Vec::new();
        for id in ids {
            if let Some(sess) = self.get_chat_session(&id).await? {
                if sess.tenant_id == tenant_id {
                    if let Some(src) = source_type {
                        if sess.source_type.as_deref() != Some(src) {
                            continue;
                        }
                    }
                    if let Some(doc) = document_id {
                        if sess.document_id.as_deref() != Some(doc) {
                            continue;
                        }
                    }
                    sessions.push(sess);
                }
            }
        }

        sessions.sort_by(|a, b| {
            b.last_activity_at
                .cmp(&a.last_activity_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions.truncate(limit);
        Ok(sessions)
    }

    pub async fn update_chat_session(
        &self,
        session_id: &str,
        tenant_id: &str,
        params: &crate::chat_sessions::UpdateChatSessionParams,
    ) -> Result<()> {
        let Some(mut session) = self.get_chat_session(session_id).await? else {
            return Ok(());
        };
        // Match SQL semantics: update only when tenant_id matches.
        if session.tenant_id != tenant_id {
            return Ok(());
        }

        if let Some(name) = params.name.as_ref() {
            session.name = name.clone();
        }
        if let Some(title) = params.title.as_ref() {
            session.title = Some(title.clone());
        }
        if let Some(stack_id) = params.stack_id.as_ref() {
            session.stack_id = stack_id.clone();
        }
        if let Some(collection_id) = params.collection_id.as_ref() {
            session.collection_id = collection_id.clone();
        }
        if let Some(document_id) = params.document_id.as_ref() {
            session.document_id = document_id.clone();
        }
        if let Some(source_type) = params.source_type.as_ref() {
            session.source_type = Some(source_type.clone());
        }
        if let Some(metadata_json) = params.metadata_json.as_ref() {
            session.metadata_json = metadata_json.clone();
        }
        if let Some(tags_json) = params.tags_json.as_ref() {
            session.tags_json = tags_json.clone();
        }
        if let Some(codebase_adapter_id) = params.codebase_adapter_id.as_ref() {
            session.codebase_adapter_id = codebase_adapter_id.clone();
        }

        let now = Self::now();
        session.updated_at = now.clone();
        session.last_activity_at = now;
        let payload = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(&session.tenant_id, session_id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chat session: {}", e)))?;
        Ok(())
    }

    pub async fn update_chat_session_activity(&self, session_id: &str) -> Result<()> {
        let Some(mut session) = self.get_chat_session(session_id).await? else {
            return Ok(());
        };
        session.last_activity_at = Self::now();
        session.updated_at = session.last_activity_at.clone();
        let payload = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(&session.tenant_id, session_id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chat session: {}", e)))?;
        Ok(())
    }

    pub async fn update_session_collection(
        &self,
        session_id: &str,
        collection_id: Option<String>,
    ) -> Result<()> {
        let Some(mut session) = self.get_chat_session(session_id).await? else {
            return Ok(());
        };
        session.collection_id = collection_id;
        session.updated_at = Self::now();
        let payload = serde_json::to_vec(&session).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::session_key(&session.tenant_id, session_id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chat session: {}", e)))?;
        Ok(())
    }

    pub async fn delete_chat_session(&self, session_id: &str) -> Result<()> {
        let Some(session) = self.get_chat_session(session_id).await? else {
            return Ok(());
        };
        // delete messages
        let msg_prefix = format!(
            "tenant/{}/chat_session/{}/message/",
            session.tenant_id, session.id
        );
        for key in self
            .backend
            .scan_prefix(&msg_prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan chat messages: {}", e)))?
        {
            if let Err(e) = self.backend.delete(&key).await {
                tracing::warn!(
                    target: "storage.kv",
                    tenant_id = %session.tenant_id,
                    session_id = %session.id,
                    key = %key,
                    error = %e,
                    "Failed to delete chat message"
                );
            }
        }
        if let Err(e) = self
            .backend
            .delete(&Self::messages_index_key(&session.tenant_id, &session.id))
            .await
        {
            tracing::warn!(
                target: "storage.kv",
                tenant_id = %session.tenant_id,
                session_id = %session.id,
                error = %e,
                "Failed to delete messages index"
            );
        }

        // remove session indexes
        if let Some(uid) = &session.user_id {
            self.remove_user_index(&session.tenant_id, uid, &session.id)
                .await?;
        }
        if let Some(bytes) = self
            .backend
            .get(&Self::session_index_key(&session.tenant_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chat session index: {}", e)))?
        {
            let mut ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            ids.retain(|v| v != &session.id);
            if ids.is_empty() {
                if let Err(e) = self
                    .backend
                    .delete(&Self::session_index_key(&session.tenant_id))
                    .await
                {
                    tracing::warn!(
                        target: "storage.kv",
                        tenant_id = %session.tenant_id,
                        session_id = %session.id,
                        error = %e,
                        "Failed to delete empty session index"
                    );
                }
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend
                    .set(&Self::session_index_key(&session.tenant_id), payload)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to update chat session index: {}", e))
                    })?;
            }
        }

        self.backend
            .delete(&Self::session_key(&session.tenant_id, &session.id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete chat session: {}", e)))?;
        if let Err(e) = self
            .backend
            .delete(&Self::session_lookup_key(&session.id))
            .await
        {
            tracing::warn!(
                target: "storage.kv",
                tenant_id = %session.tenant_id,
                session_id = %session.id,
                error = %e,
                "Failed to delete session lookup key"
            );
        }
        Ok(())
    }

    pub async fn add_chat_message(
        &self,
        params: &crate::chat_sessions::AddMessageParams,
    ) -> Result<String> {
        let Some(session) = self.get_chat_session(&params.session_id).await? else {
            return Err(AosError::NotFound("Chat session not found".to_string()));
        };
        let id = params.id.clone();
        let idx_key = Self::messages_index_key(&session.tenant_id, &session.id);
        let mut ids: Vec<String> = match self
            .backend
            .get(&idx_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read message index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        let next_sequence = params.sequence.unwrap_or(ids.len() as i64);
        let created_at = params.created_at.clone().unwrap_or_else(Self::now);
        let message = ChatMessageKv {
            id: id.clone(),
            session_id: params.session_id.clone(),
            tenant_id: session.tenant_id.clone(),
            role: params.role.clone(),
            content: params.content.clone(),
            timestamp: created_at.clone(),
            created_at,
            sequence: next_sequence,
            metadata_json: params.metadata_json.clone(),
        };
        let payload = serde_json::to_vec(&message).map_err(AosError::Serialization)?;
        self.backend
            .set(
                &Self::message_key(&session.tenant_id, &session.id, &id),
                payload,
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chat message: {}", e)))?;

        // index
        ids.push(id.clone());
        let payload_idx = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
        self.backend
            .set(&idx_key, payload_idx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update message index: {}", e)))?;

        // bump activity
        self.update_chat_session_activity(&session.id).await?;
        Ok(id)
    }

    pub async fn get_chat_messages(
        &self,
        session_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<ChatMessageKv>> {
        let Some(session) = self.get_chat_session(session_id).await? else {
            return Ok(Vec::new());
        };
        let idx_key = Self::messages_index_key(&session.tenant_id, session_id);
        let ids: Vec<String> = match self
            .backend
            .get(&idx_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to read message index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        // deterministic order: timestamp asc (index append order), tie by id
        let mut msgs = Vec::new();
        for id in &ids {
            if let Some(bytes) = self
                .backend
                .get(&Self::message_key(&session.tenant_id, session_id, id))
                .await
                .map_err(|e| AosError::Database(format!("Failed to read chat message: {}", e)))?
            {
                if let Ok(m) = serde_json::from_slice::<ChatMessageKv>(&bytes) {
                    msgs.push(m);
                }
            }
        }

        msgs.sort_by(|a, b| {
            a.sequence
                .cmp(&b.sequence)
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.id.cmp(&b.id))
        });
        if let Some(lim) = limit {
            msgs.truncate(lim.max(0) as usize);
        }
        Ok(msgs)
    }

    pub async fn count_active_chat_sessions(&self, tenant_id: &str) -> Result<i64> {
        let sessions = self
            .list_chat_sessions(tenant_id, None, None, None, usize::MAX)
            .await?;
        Ok(sessions.iter().filter(|s| s.status == "active").count() as i64)
    }
}
