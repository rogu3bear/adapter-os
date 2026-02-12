//! Chat session routes.
//!
//! This module contains all routes for:
//! - `/v1/chat/sessions/*` - Chat session CRUD, messages, sharing
//! - `/v1/chat/tags/*` - Chat tag management
//! - `/v1/chat/categories/*` - Chat category management
//! - `/v1/chats/*` - Chat creation from training jobs

use crate::handlers;
use crate::state::AppState;
use axum::{
    routing::{delete, get, post, put},
    Router,
};

/// Build the chat routes subrouter.
///
/// These routes require authentication and are merged into the protected routes.
pub fn chat_routes() -> Router<AppState> {
    Router::new()
        // Chat session routes
        .route(
            "/v1/chat/sessions",
            post(handlers::chat_sessions::create_chat_session)
                .get(handlers::chat_sessions::list_chat_sessions),
        )
        .route(
            "/v1/chats/from-training-job",
            post(handlers::create_chat_from_training_job),
        )
        // Special paths MUST come before the {session_id} wildcard
        .route(
            "/v1/chat/sessions/archived",
            get(handlers::chat_sessions::list_archived_sessions),
        )
        .route(
            "/v1/chat/sessions/trash",
            get(handlers::chat_sessions::list_deleted_sessions),
        )
        .route(
            "/v1/chat/sessions/search",
            get(handlers::chat_sessions::search_chat_sessions),
        )
        .route(
            "/v1/chat/sessions/shared-with-me",
            get(handlers::chat_sessions::get_sessions_shared_with_me),
        )
        // Wildcard route after special paths
        .route(
            "/v1/chat/sessions/{session_id}",
            get(handlers::chat_sessions::get_chat_session)
                .put(handlers::chat_sessions::update_chat_session)
                .delete(handlers::chat_sessions::delete_chat_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/messages",
            post(handlers::chat_sessions::add_chat_message)
                .get(handlers::chat_sessions::get_chat_messages),
        )
        .route(
            "/v1/chat/sessions/{session_id}/summary",
            get(handlers::chat_sessions::get_session_summary),
        )
        .route(
            "/v1/chat/sessions/{session_id}/collection",
            put(handlers::chat_sessions::update_session_collection),
        )
        .route(
            "/v1/chat/messages/{message_id}/evidence",
            get(handlers::chat_sessions::get_message_evidence),
        )
        .route(
            "/v1/chat/sessions/{session_id}/provenance",
            get(handlers::chat_sessions::get_chat_provenance),
        )
        // Chat tags routes
        .route(
            "/v1/chat/tags",
            get(handlers::chat_sessions::list_chat_tags)
                .post(handlers::chat_sessions::create_chat_tag),
        )
        .route(
            "/v1/chat/tags/{tag_id}",
            put(handlers::chat_sessions::update_chat_tag)
                .delete(handlers::chat_sessions::delete_chat_tag),
        )
        // Chat categories routes
        .route(
            "/v1/chat/categories",
            get(handlers::chat_sessions::list_chat_categories)
                .post(handlers::chat_sessions::create_chat_category),
        )
        .route(
            "/v1/chat/categories/{category_id}",
            put(handlers::chat_sessions::update_chat_category)
                .delete(handlers::chat_sessions::delete_chat_category),
        )
        // Chat session tags
        .route(
            "/v1/chat/sessions/{session_id}/tags",
            get(handlers::chat_sessions::get_session_tags)
                .post(handlers::chat_sessions::assign_tags_to_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/tags/{tag_id}",
            delete(handlers::chat_sessions::remove_tag_from_session),
        )
        // Chat session category
        .route(
            "/v1/chat/sessions/{session_id}/category",
            put(handlers::chat_sessions::set_session_category),
        )
        // Chat session fork
        .route(
            "/v1/chat/sessions/{session_id}/fork",
            post(handlers::chat_sessions::fork_chat_session),
        )
        // Chat session archive/restore
        .route(
            "/v1/chat/sessions/{session_id}/archive",
            post(handlers::chat_sessions::archive_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/restore",
            post(handlers::chat_sessions::restore_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/permanent",
            delete(handlers::chat_sessions::hard_delete_session),
        )
        // Chat session shares
        .route(
            "/v1/chat/sessions/{session_id}/shares",
            get(handlers::chat_sessions::get_session_shares)
                .post(handlers::chat_sessions::share_session),
        )
        .route(
            "/v1/chat/sessions/{session_id}/shares/{share_id}",
            delete(handlers::chat_sessions::revoke_session_share),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_routes_builds() {
        // Verify routes compile and build without panic
        let _router: Router<AppState> = chat_routes();
    }
}
