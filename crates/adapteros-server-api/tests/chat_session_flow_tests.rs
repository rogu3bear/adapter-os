use adapteros_server_api::handlers::chat_sessions::{
    add_chat_message, create_chat_session, list_chat_sessions, AddChatMessageRequest,
    CreateChatSessionRequest, ListSessionsQuery,
};
use adapteros_server_api::handlers::owner_chat::{
    handle_owner_chat, ChatMessage, OwnerChatRequest,
};
use axum::{extract::Path, extract::State, http::StatusCode, Extension, Json};

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn create_general_session_and_messages_flow() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let req = CreateChatSessionRequest {
        tenant_id: None,
        name: "General flow session".to_string(),
        title: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };

    let (status, Json(created)) =
        create_chat_session(State(state.clone()), Extension(claims.clone()), Json(req))
            .await
            .expect("session created");
    assert_eq!(status, StatusCode::CREATED);

    let stored = state
        .db
        .get_chat_session(&created.session_id)
        .await
        .expect("db fetch")
        .expect("session missing");
    assert_eq!(stored.tenant_id, claims.tenant_id);
    assert_eq!(stored.source_type.as_deref(), Some("general"));

    let query = ListSessionsQuery {
        user_id: None,
        limit: None,
        source_type: Some("general".to_string()),
        document_id: None,
    };
    let listed = list_chat_sessions(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Query(query),
    )
    .await
    .expect("list sessions")
    .0;
    assert!(
        listed.iter().any(|s| s.id == created.session_id),
        "general session should appear in list filter"
    );

    let user_msg = AddChatMessageRequest {
        role: "user".to_string(),
        content: "hello".to_string(),
        metadata_json: None,
    };
    let assistant_msg = AddChatMessageRequest {
        role: "assistant".to_string(),
        content: "hi back".to_string(),
        metadata_json: None,
    };
    add_chat_message(
        State(state.clone()),
        Extension(claims.clone()),
        Path(created.session_id.clone()),
        Json(user_msg),
    )
    .await
    .expect("user message created");
    add_chat_message(
        State(state.clone()),
        Extension(claims.clone()),
        Path(created.session_id.clone()),
        Json(assistant_msg),
    )
    .await
    .expect("assistant message created");

    let messages = state
        .db
        .get_chat_messages(&created.session_id, None)
        .await
        .expect("messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");
}

#[tokio::test]
async fn owner_system_chat_creates_session_and_persists_messages() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let request = OwnerChatRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "status?".to_string(),
        }],
        context: None,
    };

    let _ = handle_owner_chat(
        State(state.clone()),
        Extension(claims.clone()),
        Json(request),
    )
    .await
    .expect("owner chat response");

    let session_id = format!("owner-session-{}-{}", claims.tenant_id, claims.sub);
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .expect("db fetch")
        .expect("owner session missing");
    assert_eq!(session.source_type.as_deref(), Some("owner_system"));
    assert_eq!(session.tenant_id, claims.tenant_id);

    let list_query = ListSessionsQuery {
        user_id: None,
        limit: None,
        source_type: Some("owner_system".to_string()),
        document_id: None,
    };
    let owner_sessions = list_chat_sessions(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Query(list_query),
    )
    .await
    .expect("list owner sessions")
    .0;
    assert!(
        owner_sessions.iter().any(|s| s.id == session_id),
        "owner session should appear in owner_system filter"
    );

    let messages = state
        .db
        .get_chat_messages(&session_id, None)
        .await
        .expect("owner messages");
    assert!(
        messages.len() >= 2,
        "owner flow should persist user and assistant messages"
    );
    let roles: Vec<String> = messages.iter().map(|m| m.role.clone()).collect();
    assert!(
        roles.contains(&"user".to_string()) && roles.contains(&"assistant".to_string()),
        "owner flow should capture both user and assistant roles"
    );
}
