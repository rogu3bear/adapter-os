use adapteros_db::collections::CreateCollectionParams;
use adapteros_db::documents::CreateDocumentParams;
use adapteros_server_api::handlers::chat_sessions::{
    create_chat_session, get_chat_session, list_chat_sessions, CreateChatSessionRequest,
    ListSessionsQuery,
};
use axum::{extract::State, http::StatusCode, Extension, Json};

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn create_document_session_requires_document_id() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let req = CreateChatSessionRequest {
        tenant_id: None,
        name: "Doc chat without doc id".to_string(),
        title: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        source_type: Some("document".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };

    let result = create_chat_session(State(state.clone()), Extension(claims), Json(req)).await;
    let err = result.expect_err("expected validation failure");
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn document_and_collection_must_match() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Create collection and document in the same tenant, but do NOT link them.
    let collection_id = state
        .db
        .create_collection(CreateCollectionParams {
            tenant_id: claims.tenant_id.clone(),
            name: "Test Collection".to_string(),
            description: None,
            metadata_json: None,
        })
        .await
        .expect("collection");

    let document_id = "doc-unlinked".to_string();
    state
        .db
        .create_document(CreateDocumentParams {
            id: document_id.clone(),
            tenant_id: claims.tenant_id.clone(),
            name: "Unlinked Doc".to_string(),
            content_hash: "hash".to_string(),
            file_path: "var/test-documents/unlinked.txt".to_string(),
            file_size: 1,
            mime_type: "text/plain".to_string(),
            page_count: None,
        })
        .await
        .expect("document");

    let req = CreateChatSessionRequest {
        tenant_id: None,
        name: "Doc chat mismatch".to_string(),
        title: None,
        stack_id: None,
        collection_id: Some(collection_id),
        document_id: Some(document_id),
        source_type: Some("document".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };

    let result = create_chat_session(State(state.clone()), Extension(claims), Json(req)).await;
    let err = result.expect_err("expected validation failure");
    assert_eq!(err.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn cross_tenant_access_is_forbidden() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Create a valid general chat session for tenant-1
    let req = CreateChatSessionRequest {
        tenant_id: None,
        name: "Tenant 1 session".to_string(),
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
        create_chat_session(State(state.clone()), Extension(claims), Json(req))
            .await
            .expect("session created");
    assert_eq!(status, StatusCode::CREATED);

    // Use claims from another tenant without admin_tenants wildcard
    let mut other_claims = test_admin_claims();
    other_claims.tenant_id = "default".to_string();
    other_claims.admin_tenants.clear();

    let result = get_chat_session(
        State(state),
        Extension(other_claims),
        axum::extract::Path(created.session_id),
    )
    .await;

    let err = result.expect_err("expected forbidden");
    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "TENANT_ISOLATION_ERROR");
}

#[tokio::test]
async fn list_sessions_respects_source_filters() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Create a general session
    let general_req = CreateChatSessionRequest {
        tenant_id: None,
        name: "General session".to_string(),
        title: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };
    let _ = create_chat_session(
        State(state.clone()),
        Extension(claims.clone()),
        Json(general_req),
    )
    .await
    .expect("general session");

    // Create a document session with a real document
    let document_id = "doc-filtered".to_string();
    state
        .db
        .create_document(CreateDocumentParams {
            id: document_id.clone(),
            tenant_id: claims.tenant_id.clone(),
            name: "Filter Doc".to_string(),
            content_hash: "hash".to_string(),
            file_path: "var/test-documents/filter.txt".to_string(),
            file_size: 1,
            mime_type: "text/plain".to_string(),
            page_count: None,
        })
        .await
        .expect("document");

    let doc_req = CreateChatSessionRequest {
        tenant_id: None,
        name: "Document session".to_string(),
        title: None,
        stack_id: None,
        collection_id: None,
        document_id: Some(document_id.clone()),
        source_type: Some("document".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };
    let _ = create_chat_session(
        State(state.clone()),
        Extension(claims.clone()),
        Json(doc_req),
    )
    .await
    .expect("doc session");

    // List with source_type + document_id filters
    let query = ListSessionsQuery {
        user_id: None,
        limit: None,
        source_type: Some("document".to_string()),
        document_id: Some(document_id),
    };

    let response = list_chat_sessions(State(state), Extension(claims), axum::extract::Query(query))
        .await
        .expect("list");
    let sessions = response.0;
    assert_eq!(sessions.len(), 1, "only document chat should match filters");
    assert_eq!(sessions[0].source_type.as_deref(), Some("document"));
}

#[tokio::test]
async fn non_admin_cannot_list_other_users_sessions() {
    let state = setup_state(None).await.expect("state");
    let mut claims = test_admin_claims();
    claims.role = "operator".to_string();
    claims.roles = vec!["operator".to_string()];

    let req = CreateChatSessionRequest {
        tenant_id: None,
        name: "Operator owned session".to_string(),
        title: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };
    let _ = create_chat_session(State(state.clone()), Extension(claims.clone()), Json(req))
        .await
        .expect("session created");

    let query = ListSessionsQuery {
        user_id: Some("someone-else".to_string()),
        limit: None,
        source_type: None,
        document_id: None,
    };

    let result =
        list_chat_sessions(State(state), Extension(claims), axum::extract::Query(query)).await;
    let err = result.expect_err("expected forbidden");
    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert!(
        err.message
            .contains("Only admins can list sessions for other users"),
        "unexpected error message: {}",
        err.message
    );
}

#[tokio::test]
async fn document_session_with_matching_collection_is_created_and_listed() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Create collection and document for the tenant
    let collection_id = state
        .db
        .create_collection(CreateCollectionParams {
            tenant_id: claims.tenant_id.clone(),
            name: "Linked Collection".to_string(),
            description: None,
            metadata_json: None,
        })
        .await
        .expect("collection");

    let document_id = "doc-linked".to_string();
    state
        .db
        .create_document(CreateDocumentParams {
            id: document_id.clone(),
            tenant_id: claims.tenant_id.clone(),
            name: "Linked Doc".to_string(),
            content_hash: "hash".to_string(),
            file_path: "var/test-documents/linked.txt".to_string(),
            file_size: 1,
            mime_type: "text/plain".to_string(),
            page_count: None,
        })
        .await
        .expect("document");

    state
        .db
        .add_document_to_collection(&claims.tenant_id, &collection_id, &document_id)
        .await
        .expect("bind doc to collection");

    let req = CreateChatSessionRequest {
        tenant_id: None,
        name: "Doc chat linked".to_string(),
        title: None,
        stack_id: None,
        collection_id: Some(collection_id.clone()),
        document_id: Some(document_id.clone()),
        source_type: Some("document".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags: None,
    };

    let (status, Json(created)) =
        create_chat_session(State(state.clone()), Extension(claims.clone()), Json(req))
            .await
            .expect("session created");
    assert_eq!(status, StatusCode::CREATED);

    let session = state
        .db
        .get_chat_session(&created.session_id)
        .await
        .expect("db fetch")
        .expect("session missing");
    assert_eq!(session.collection_id, Some(collection_id.clone()));
    assert_eq!(session.document_id, Some(document_id.clone()));
    assert_eq!(session.tenant_id, claims.tenant_id);

    let query = ListSessionsQuery {
        user_id: None,
        limit: None,
        source_type: Some("document".to_string()),
        document_id: Some(document_id),
    };
    let listed = list_chat_sessions(State(state), Extension(claims), axum::extract::Query(query))
        .await
        .expect("list sessions")
        .0;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].collection_id, Some(collection_id));
}
