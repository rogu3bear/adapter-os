//! KV storage for workspace messages.
//!
//! Provides a simple, prefix-scanned repository for messages to support
//! KV-primary and dual-write modes. Indexing is kept minimal; read paths
//! filter in-memory after prefix scans to preserve correctness.

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageKv {
    pub id: String,
    pub workspace_id: String,
    pub from_user_id: String,
    pub from_tenant_id: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
}

pub struct MessageKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl MessageKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn message_key(id: &str) -> String {
        format!("message:{id}")
    }

    pub async fn put(&self, message: &MessageKv) -> Result<()> {
        let bytes = serde_json::to_vec(message).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::message_key(&message.id), bytes)
            .await
            .map_err(|e| AosError::Database(format!("KV store message failed: {e}")))
    }

    pub async fn get(&self, id: &str) -> Result<Option<MessageKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::message_key(id))
            .await
            .map_err(|e| AosError::Database(format!("KV get message failed: {e}")))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        self.backend
            .delete(&Self::message_key(id))
            .await
            .map_err(|e| AosError::Database(format!("KV delete message failed: {e}")))?;
        Ok(())
    }

    async fn scan_all(&self) -> Result<Vec<MessageKv>> {
        let keys = self
            .backend
            .scan_prefix("message:")
            .await
            .map_err(|e| AosError::Database(format!("KV scan messages failed: {e}")))?;

        let mut messages = Vec::new();
        for key in keys {
            if let Some(bytes) = self
                .backend
                .get(&key)
                .await
                .map_err(|e| AosError::Database(format!("KV load message failed: {e}")))?
            {
                if let Ok(msg) = serde_json::from_slice::<MessageKv>(&bytes) {
                    messages.push(msg);
                }
            }
        }
        Ok(messages)
    }

    pub async fn list_workspace_messages(
        &self,
        workspace_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MessageKv>> {
        let mut messages: Vec<MessageKv> = self
            .scan_all()
            .await?
            .into_iter()
            .filter(|m| m.workspace_id == workspace_id && m.thread_id.is_none())
            .collect();

        // Deterministic ordering: created_at DESC, id DESC as tiebreaker
        messages.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| b.id.cmp(&a.id))
        });

        let start = offset.max(0) as usize;
        let end = (start + limit.max(0) as usize).min(messages.len());
        Ok(messages.get(start..end).unwrap_or_default().to_vec())
    }

    pub async fn list_thread_messages(
        &self,
        thread_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MessageKv>> {
        let mut messages: Vec<MessageKv> = self
            .scan_all()
            .await?
            .into_iter()
            .filter(|m| m.thread_id.as_deref() == Some(thread_id) || m.id == thread_id)
            .collect();

        // Deterministic ordering: created_at ASC, id ASC
        messages.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let start = offset.max(0) as usize;
        let end = (start + limit.max(0) as usize).min(messages.len());
        Ok(messages.get(start..end).unwrap_or_default().to_vec())
    }

    pub async fn list_recent_workspace(
        &self,
        workspace_id: &str,
        since: Option<&str>,
    ) -> Result<Vec<MessageKv>> {
        let mut messages: Vec<MessageKv> = self
            .scan_all()
            .await?
            .into_iter()
            .filter(|m| m.workspace_id == workspace_id)
            .collect();

        if let Some(ts) = since {
            // Compare as &str to avoid string ownership issues while keeping lexicographic ordering
            messages.retain(|m| m.created_at.as_str() > ts);
        }

        // For "recent", sort ascending to match SQL path behavior for since, else DESC limit 50
        messages.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(messages)
    }

    pub fn new_message_record(
        &self,
        workspace_id: &str,
        from_user_id: &str,
        from_tenant_id: &str,
        content: &str,
        thread_id: Option<&str>,
    ) -> MessageKv {
        MessageKv {
            id: new_id(IdPrefix::Msg),
            workspace_id: workspace_id.to_string(),
            from_user_id: from_user_id.to_string(),
            from_tenant_id: from_tenant_id.to_string(),
            content: content.to_string(),
            thread_id: thread_id.map(|s| s.to_string()),
            created_at: Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            edited_at: None,
        }
    }
}
