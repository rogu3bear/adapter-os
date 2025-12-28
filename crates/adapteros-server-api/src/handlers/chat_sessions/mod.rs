//! Chat session API handlers
//!
//! Provides endpoints for managing persistent chat sessions with stack context
//! and trace linkage for the workspace experience.
//!
//! This module is organized into submodules by functionality:
//! - `types`: Request/response types and helper structs
//! - `core`: CRUD operations (create, read, update, delete)
//! - `messages`: Message handling (add, get, summary, evidence)
//! - `provenance`: Provenance chain retrieval
//! - `collection`: Collection binding updates
//! - `tags`: Tag management
//! - `categories`: Category management
//! - `archive`: Archive/restore/delete operations
//! - `search`: Session search
//! - `sharing`: Session sharing
//! - `contacts`: Contact management
//! - `fork`: Session forking
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_handlers】

mod archive;
mod categories;
mod collection;
mod contacts;
mod core;
mod fork;
mod messages;
mod provenance;
mod search;
mod sharing;
mod tags;
mod types;

// Re-export all types
pub use types::*;

// Re-export core CRUD handlers and their OpenAPI path functions
pub use core::{
    __path_create_chat_session, __path_delete_chat_session, __path_get_chat_session,
    __path_list_chat_sessions, __path_update_chat_session, create_chat_session,
    delete_chat_session, get_chat_session, list_chat_sessions, update_chat_session,
};

// Re-export message handlers and their OpenAPI path functions
pub use messages::{
    __path_add_chat_message, __path_get_chat_messages, __path_get_message_evidence,
    __path_get_session_summary, add_chat_message, get_chat_messages, get_message_evidence,
    get_session_summary,
};

// Re-export provenance handlers and OpenAPI path function
pub use provenance::{__path_get_chat_provenance, get_chat_provenance};

// Re-export collection handlers and OpenAPI path function
pub use collection::{__path_update_session_collection, update_session_collection};

// Re-export tag handlers (no OpenAPI path attributes)
pub use tags::{
    assign_tags_to_session, create_chat_tag, delete_chat_tag, get_session_tags, list_chat_tags,
    remove_tag_from_session, update_chat_tag,
};

// Re-export category handlers (no OpenAPI path attributes)
pub use categories::{
    create_chat_category, delete_chat_category, list_chat_categories, set_session_category,
    update_chat_category,
};

// Re-export archive handlers (no OpenAPI path attributes)
pub use archive::{
    archive_session, hard_delete_session, list_archived_sessions, list_deleted_sessions,
    restore_session,
};

// Re-export search handlers (no OpenAPI path attributes)
pub use search::search_chat_sessions;

// Re-export sharing handlers (no OpenAPI path attributes)
pub use sharing::{
    get_session_shares, get_sessions_shared_with_me, revoke_session_share, share_session,
};

// Re-export contact handlers and their OpenAPI path functions
pub use contacts::{
    __path_create_contact, __path_delete_contact, __path_get_contact,
    __path_get_contact_interactions, __path_list_contacts, create_contact, delete_contact,
    get_contact, get_contact_interactions, list_contacts,
};

// Re-export fork handlers and OpenAPI path function
pub use fork::{__path_fork_chat_session, fork_chat_session};
