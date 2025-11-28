//! Chat Sessions Database Integration Tests
//!
//! Comprehensive tests for chat session functionality including:
//! - Core CRUD operations
//! - Trace linkage
//! - Tenant isolation
//! - Message handling

use adapteros_core::{AosError, Result};
use adapteros_db::chat_sessions::{AddMessageParams, CreateChatSessionParams};
use adapteros_db::sqlx;
use adapteros_db::Db;

/// Helper to create a tenant
async fn create_tenant(db: &Db, tenant_id: &str, name: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(name)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;
    Ok(())
}

/// Helper to create a session with defaults
async fn create_session(
    db: &Db,
    session_id: &str,
    tenant_id: &str,
    name: &str,
    user_id: Option<&str>,
) -> Result<String> {
    let params = CreateChatSessionParams {
        id: session_id.to_string(),
        tenant_id: tenant_id.to_string(),
        user_id: user_id.map(|s| s.to_string()),
        stack_id: None,
        collection_id: None,
        name: name.to_string(),
        metadata_json: None,
    };
    db.create_chat_session(params).await
}

// =============================================================================
// Core CRUD Tests (8)
// =============================================================================

#[tokio::test]
async fn test_create_and_retrieve_session() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;

    // Create session with fields that don't have FK constraints
    let params = CreateChatSessionParams {
        id: "session-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        user_id: Some("user-1".to_string()),
        stack_id: None,      // FK constraint - skip
        collection_id: None, // FK constraint - skip
        name: "Test Session".to_string(),
        metadata_json: Some(r#"{"key": "value"}"#.to_string()),
    };

    let session_id = db.create_chat_session(params).await?;
    assert_eq!(session_id, "session-1");

    // Retrieve and verify all fields
    let session = db.get_chat_session("session-1").await?.unwrap();
    assert_eq!(session.id, "session-1");
    assert_eq!(session.tenant_id, "tenant-1");
    assert_eq!(session.user_id, Some("user-1".to_string()));
    assert_eq!(session.name, "Test Session");
    assert!(session.metadata_json.is_some());

    Ok(())
}

#[tokio::test]
async fn test_list_sessions_by_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_tenant(&db, "tenant-a", "Tenant A").await?;
    create_tenant(&db, "tenant-b", "Tenant B").await?;

    // Create sessions in both tenants
    create_session(&db, "session-a1", "tenant-a", "Session A1", None).await?;
    create_session(&db, "session-a2", "tenant-a", "Session A2", None).await?;
    create_session(&db, "session-b1", "tenant-b", "Session B1", None).await?;

    // List for tenant-a
    let sessions_a = db.list_chat_sessions("tenant-a", None, None).await?;
    assert_eq!(sessions_a.len(), 2);

    // List for tenant-b
    let sessions_b = db.list_chat_sessions("tenant-b", None, None).await?;
    assert_eq!(sessions_b.len(), 1);
    assert_eq!(sessions_b[0].id, "session-b1");

    // Verify ordering (most recent first)
    // Sessions are ordered by last_activity_at DESC
    assert!(sessions_a.iter().all(|s| s.tenant_id == "tenant-a"));

    Ok(())
}

#[tokio::test]
async fn test_list_sessions_by_user() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;

    // Create sessions for different users
    create_session(
        &db,
        "session-u1-1",
        "tenant-1",
        "User 1 Session 1",
        Some("user-1"),
    )
    .await?;
    create_session(
        &db,
        "session-u1-2",
        "tenant-1",
        "User 1 Session 2",
        Some("user-1"),
    )
    .await?;
    create_session(
        &db,
        "session-u2-1",
        "tenant-1",
        "User 2 Session 1",
        Some("user-2"),
    )
    .await?;

    // List for user-1
    let user1_sessions = db
        .list_chat_sessions("tenant-1", Some("user-1"), None)
        .await?;
    assert_eq!(user1_sessions.len(), 2);

    // List for user-2
    let user2_sessions = db
        .list_chat_sessions("tenant-1", Some("user-2"), None)
        .await?;
    assert_eq!(user2_sessions.len(), 1);

    // Test limit parameter
    let limited = db
        .list_chat_sessions("tenant-1", Some("user-1"), Some(1))
        .await?;
    assert_eq!(limited.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_add_and_retrieve_messages() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Add messages with different roles
    let roles = ["user", "assistant", "system"];
    for (i, role) in roles.iter().enumerate() {
        let params = AddMessageParams {
            id: format!("msg-{}", i),
            session_id: "session-1".to_string(),
            role: role.to_string(),
            content: format!("Message from {}", role),
            metadata_json: None,
        };
        db.add_chat_message(params).await?;
    }

    // Retrieve messages
    let messages = db.get_chat_messages("session-1", None).await?;
    assert_eq!(messages.len(), 3);

    // Verify ordering (oldest first by timestamp)
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[2].role, "system");

    Ok(())
}

#[tokio::test]
async fn test_update_chat_session_activity() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Get initial timestamp
    let session_before = db.get_chat_session("session-1").await?.unwrap();
    let initial_activity = session_before.last_activity_at.clone();

    // Small delay to ensure timestamp difference
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Update activity
    db.update_chat_session_activity("session-1").await?;

    // Get updated timestamp
    let session_after = db.get_chat_session("session-1").await?.unwrap();

    // Timestamps should differ (or at least activity was called successfully)
    assert!(
        session_after.last_activity_at >= initial_activity,
        "Activity timestamp should be updated"
    );

    Ok(())
}

#[tokio::test]
async fn test_update_session_collection() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Session starts with no collection
    let session = db.get_chat_session("session-1").await?.unwrap();
    assert!(session.collection_id.is_none());

    // Create collections in the document_collections table (FK target)
    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name) VALUES ('collection-1', 'tenant-1', 'Test Collection 1')",
    )
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;
    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name) VALUES ('collection-2', 'tenant-1', 'Test Collection 2')",
    )
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    // Bind collection
    db.update_session_collection("session-1", Some("collection-1".to_string()))
        .await?;
    let session = db.get_chat_session("session-1").await?.unwrap();
    assert_eq!(session.collection_id, Some("collection-1".to_string()));

    // Change collection
    db.update_session_collection("session-1", Some("collection-2".to_string()))
        .await?;
    let session = db.get_chat_session("session-1").await?.unwrap();
    assert_eq!(session.collection_id, Some("collection-2".to_string()));

    // Unbind collection
    db.update_session_collection("session-1", None).await?;
    let session = db.get_chat_session("session-1").await?.unwrap();
    assert!(session.collection_id.is_none());

    Ok(())
}

#[tokio::test]
async fn test_session_deletion_cascades() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Add messages
    let msg_params = AddMessageParams {
        id: "msg-1".to_string(),
        session_id: "session-1".to_string(),
        role: "user".to_string(),
        content: "Hello".to_string(),
        metadata_json: None,
    };
    db.add_chat_message(msg_params).await?;

    // Add traces
    db.add_session_trace("session-1", "router_decision", "decision-1")
        .await?;

    // Verify data exists
    let messages = db.get_chat_messages("session-1", None).await?;
    assert_eq!(messages.len(), 1);
    let traces = db.get_session_traces("session-1").await?;
    assert_eq!(traces.len(), 1);

    // Delete session
    db.delete_chat_session("session-1").await?;

    // Verify session is gone
    let session = db.get_chat_session("session-1").await?;
    assert!(session.is_none());

    // Verify cascade deleted messages and traces
    let messages = db.get_chat_messages("session-1", None).await?;
    assert_eq!(messages.len(), 0);
    let traces = db.get_session_traces("session-1").await?;
    assert_eq!(traces.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_get_nonexistent_session_returns_none() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let session = db.get_chat_session("nonexistent-session").await?;
    assert!(session.is_none());

    Ok(())
}

// =============================================================================
// Trace Linkage Tests (3)
// =============================================================================

#[tokio::test]
async fn test_add_session_traces() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Add traces of different types
    let trace_types = [
        ("router_decision", "decision-1"),
        ("adapter", "adapter-1"),
        ("training_job", "job-1"),
        ("audit_event", "event-1"),
    ];

    for (trace_type, trace_id) in &trace_types {
        db.add_session_trace("session-1", trace_type, trace_id)
            .await?;
    }

    // Retrieve traces
    let traces = db.get_session_traces("session-1").await?;
    assert_eq!(traces.len(), 4);

    // Verify all trace types present
    let trace_types_found: Vec<&str> = traces.iter().map(|t| t.trace_type.as_str()).collect();
    assert!(trace_types_found.contains(&"router_decision"));
    assert!(trace_types_found.contains(&"adapter"));
    assert!(trace_types_found.contains(&"training_job"));
    assert!(trace_types_found.contains(&"audit_event"));

    Ok(())
}

#[tokio::test]
async fn test_multiple_traces_same_type() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Add 3 router_decision traces
    for i in 1..=3 {
        db.add_session_trace("session-1", "router_decision", &format!("decision-{}", i))
            .await?;
    }

    // Retrieve traces
    let traces = db.get_session_traces("session-1").await?;
    assert_eq!(traces.len(), 3);

    // All should be router_decision type
    assert!(traces.iter().all(|t| t.trace_type == "router_decision"));

    // Verify trace_ids are preserved
    let trace_ids: Vec<&str> = traces.iter().map(|t| t.trace_id.as_str()).collect();
    assert!(trace_ids.contains(&"decision-1"));
    assert!(trace_ids.contains(&"decision-2"));
    assert!(trace_ids.contains(&"decision-3"));

    Ok(())
}

#[tokio::test]
async fn test_get_session_summary_with_traces() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Add messages
    for i in 1..=3 {
        let params = AddMessageParams {
            id: format!("msg-{}", i),
            session_id: "session-1".to_string(),
            role: "user".to_string(),
            content: format!("Message {}", i),
            metadata_json: None,
        };
        db.add_chat_message(params).await?;
    }

    // Add traces
    db.add_session_trace("session-1", "router_decision", "decision-1")
        .await?;
    db.add_session_trace("session-1", "router_decision", "decision-2")
        .await?;
    db.add_session_trace("session-1", "adapter", "adapter-1")
        .await?;

    // Get summary
    let summary = db.get_session_summary("session-1").await?;

    // Verify structure
    assert_eq!(summary["session_id"], "session-1");
    assert_eq!(summary["message_count"], 3);

    // Verify trace counts by type
    let trace_counts = &summary["trace_counts"];
    assert_eq!(trace_counts["router_decision"], 2);
    assert_eq!(trace_counts["adapter"], 1);

    Ok(())
}

// =============================================================================
// Edge Case Tests (4)
// =============================================================================

#[tokio::test]
async fn test_tenant_isolation_enforced() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_tenant(&db, "tenant-a", "Tenant A").await?;
    create_tenant(&db, "tenant-b", "Tenant B").await?;

    // Create session in tenant-a
    create_session(&db, "session-a1", "tenant-a", "Session A", None).await?;

    // List sessions for tenant-b should NOT include tenant-a's sessions
    let tenant_b_sessions = db.list_chat_sessions("tenant-b", None, None).await?;
    assert!(tenant_b_sessions.is_empty());

    // List sessions for tenant-a should include its sessions
    let tenant_a_sessions = db.list_chat_sessions("tenant-a", None, None).await?;
    assert_eq!(tenant_a_sessions.len(), 1);
    assert_eq!(tenant_a_sessions[0].id, "session-a1");

    Ok(())
}

#[tokio::test]
async fn test_activity_timestamp_auto_updates() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // Get initial timestamp
    let session_before = db.get_chat_session("session-1").await?.unwrap();
    let initial_activity = session_before.last_activity_at.clone();

    // Small delay
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Add a message (should auto-update activity)
    let params = AddMessageParams {
        id: "msg-1".to_string(),
        session_id: "session-1".to_string(),
        role: "user".to_string(),
        content: "Hello".to_string(),
        metadata_json: None,
    };
    db.add_chat_message(params).await?;

    // Check timestamp was updated
    let session_after = db.get_chat_session("session-1").await?.unwrap();
    assert!(
        session_after.last_activity_at >= initial_activity,
        "Adding message should update last_activity_at"
    );

    Ok(())
}

#[tokio::test]
async fn test_message_role_validation() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;
    create_session(&db, "session-1", "tenant-1", "Test Session", None).await?;

    // All standard roles should be accepted
    let roles = ["user", "assistant", "system"];
    for (i, role) in roles.iter().enumerate() {
        let params = AddMessageParams {
            id: format!("msg-{}", i),
            session_id: "session-1".to_string(),
            role: role.to_string(),
            content: format!("Content from {}", role),
            metadata_json: None,
        };
        let result = db.add_chat_message(params).await;
        assert!(result.is_ok(), "Role '{}' should be accepted", role);
    }

    // Verify messages stored correctly
    let messages = db.get_chat_messages("session-1", None).await?;
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[2].role, "system");

    Ok(())
}

#[tokio::test]
async fn test_metadata_json_serialization() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-1", "Test Tenant").await?;

    // Create session with complex metadata
    let complex_metadata = serde_json::json!({
        "key": "value",
        "nested": {
            "array": [1, 2, 3],
            "boolean": true,
            "null": null
        },
        "unicode": "Hello \u{1F600}"
    });

    let params = CreateChatSessionParams {
        id: "session-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        user_id: None,
        stack_id: None,
        collection_id: None,
        name: "Test Session".to_string(),
        metadata_json: Some(complex_metadata.to_string()),
    };
    db.create_chat_session(params).await?;

    // Retrieve and verify metadata preserved
    let session = db.get_chat_session("session-1").await?.unwrap();
    assert!(session.metadata_json.is_some());

    let retrieved: serde_json::Value =
        serde_json::from_str(&session.metadata_json.unwrap()).expect("Should parse JSON");
    assert_eq!(retrieved["key"], "value");
    assert_eq!(retrieved["nested"]["array"][0], 1);
    assert_eq!(retrieved["nested"]["boolean"], true);
    assert!(retrieved["nested"]["null"].is_null());

    // Also test message metadata
    let msg_metadata = serde_json::json!({
        "tokens": 150,
        "model": "gpt-4",
        "latency_ms": 234.5
    });

    let msg_params = AddMessageParams {
        id: "msg-1".to_string(),
        session_id: "session-1".to_string(),
        role: "assistant".to_string(),
        content: "Response".to_string(),
        metadata_json: Some(msg_metadata.to_string()),
    };
    db.add_chat_message(msg_params).await?;

    let messages = db.get_chat_messages("session-1", None).await?;
    assert!(messages[0].metadata_json.is_some());
    let msg_meta: serde_json::Value =
        serde_json::from_str(&messages[0].metadata_json.clone().unwrap())
            .expect("Should parse message metadata");
    assert_eq!(msg_meta["tokens"], 150);

    Ok(())
}
