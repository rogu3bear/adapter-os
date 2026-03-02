//! Persistent knowledge collection types.
//!
//! Introduces the distinction between per-session document collections and
//! persistent "Knowledge" collections that are available across all sessions.
//!
//! Backend: `document_collections.scope` column distinguishes the two.
//! The existing collection CRUD and RAG pipeline work unchanged; scope controls
//! visibility and session binding behavior.

use serde::{Deserialize, Serialize};

use crate::schema_version;

// =============================================================================
// Collection Scope
// =============================================================================

/// Scope of a document collection.
///
/// Controls whether a collection is bound to a single chat session or is
/// persistently available across all sessions as part of the tenant's knowledge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CollectionScope {
    /// Collection is used within one chat session (default, current behavior).
    #[default]
    Session,
    /// Collection is part of the tenant's persistent knowledge base.
    /// Always available for RAG across all sessions.
    Knowledge,
}

impl CollectionScope {
    /// SQL string representation for the `scope` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Knowledge => "knowledge",
        }
    }

    /// Parse from SQL string representation.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "knowledge" => Self::Knowledge,
            _ => Self::Session,
        }
    }
}

// =============================================================================
// Knowledge Collection API Types
// =============================================================================

/// Request to add a document to the tenant's knowledge base.
///
/// When a document is added to knowledge, it joins (or creates) the tenant's
/// persistent knowledge collection and becomes available for RAG in all sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AddToKnowledgeRequest {
    /// Document ID to add to knowledge.
    pub document_id: String,
    /// Optional: specific collection within knowledge to add to.
    /// If omitted, uses the tenant's default knowledge collection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
}

/// Response after adding a document to knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AddToKnowledgeResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    /// The knowledge collection the document was added to.
    pub collection_id: String,
    /// Document ID that was added.
    pub document_id: String,
    /// Whether a new collection was created for this.
    pub collection_created: bool,
}

/// Summary of a knowledge collection for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct KnowledgeCollectionSummary {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub document_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Document scope choice presented to the user during upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DocumentScopeChoice {
    /// Use this document in the current conversation only.
    ThisConversation,
    /// Add to persistent knowledge (available in all sessions).
    AddToKnowledge,
}
