//! Integration tests for multi-turn chat context building
//!
//! These tests verify that `build_chat_prompt` correctly:
//! 1. Loads messages from the database
//! 2. Formats them with role markers
//! 3. Applies truncation rules
//! 4. Computes deterministic context hashes

use adapteros_core::identity::IdentityEnvelope;
use adapteros_db::chat_sessions::{AddMessageParams, CreateChatSessionParams};
use adapteros_db::Db;
use adapteros_server_api::chat_context::build_chat_prompt;
use adapteros_server_api::handlers::streaming_infer::{streaming_infer, StreamingInferRequest};
use adapteros_server_api::state::ChatContextConfig;
use axum::{extract::State, Extension, Json};
use std::sync::{Arc, Mutex};

mod common;
use common::{create_test_adapter_default, register_test_worker, setup_state, test_admin_claims};

/// Create an in-memory test database with chat tables
async fn create_test_db() -> Db {
    Db::new_in_memory()
        .await
        .expect("Failed to create in-memory database")
}

/// Helper to create a tenant (required for FK constraints)
async fn create_test_tenant(db: &Db, name: &str) -> String {
    db.create_tenant(name, false)
        .await
        .expect("Failed to create test tenant")
}

/// Helper to create a test session
async fn create_test_session(db: &Db, session_id: &str, tenant_id: &str) {
    db.create_chat_session(CreateChatSessionParams {
        id: session_id.to_string(),
        tenant_id: tenant_id.to_string(),
        user_id: None,
        created_by: None,
        stack_id: None,
        collection_id: None,
        document_id: None,
        name: "Test Session".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: None,
        codebase_adapter_id: None,
    })
    .await
    .expect("Failed to create test session");
}

/// Helper to add a message to a session
async fn add_test_message(db: &Db, session_id: &str, role: &str, content: &str) -> String {
    db.add_chat_message(AddMessageParams {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        tenant_id: None,
        role: role.to_string(),
        content: content.to_string(),
        sequence: None,
        created_at: None,
        metadata_json: None,
    })
    .await
    .expect("Failed to add test message")
}

#[tokio::test]
async fn test_build_chat_prompt_with_real_db() {
    let db = create_test_db().await;
    let session_id = "test-session-001";
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Create session and add messages
    create_test_session(&db, session_id, &tenant_id).await;

    add_test_message(&db, session_id, "system", "You are a helpful assistant.").await;
    // Delay to ensure timestamp ordering - use 50ms for CI reliability
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    add_test_message(&db, session_id, "user", "Hello, how are you?").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    add_test_message(&db, session_id, "assistant", "I'm doing well, thank you!").await;

    // Build prompt with new message
    let config = ChatContextConfig::default();
    let result = build_chat_prompt(&db, session_id, "What's the weather?", &config)
        .await
        .expect("Failed to build chat prompt");

    // Verify format
    assert!(
        result
            .prompt_text
            .contains("[system]: You are a helpful assistant."),
        "Missing system message in: {}",
        result.prompt_text
    );
    assert!(
        result.prompt_text.contains("[user]: Hello, how are you?"),
        "Missing user message in: {}",
        result.prompt_text
    );
    assert!(
        result
            .prompt_text
            .contains("[assistant]: I'm doing well, thank you!"),
        "Missing assistant message in: {}",
        result.prompt_text
    );
    assert!(
        result.prompt_text.contains("[user]: What's the weather?"),
        "Missing new message in: {}",
        result.prompt_text
    );

    // Verify message count (3 history messages)
    assert_eq!(result.message_count, 3);

    // Verify not truncated with default config
    assert!(!result.truncated);

    // Verify context hash is non-empty
    assert!(!result.context_hash.is_empty());
}

#[tokio::test]
async fn test_build_chat_prompt_empty_session() {
    let db = create_test_db().await;
    let session_id = "test-session-empty";
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Create session with no messages
    create_test_session(&db, session_id, &tenant_id).await;

    let config = ChatContextConfig::default();
    let result = build_chat_prompt(&db, session_id, "First message", &config)
        .await
        .expect("Failed to build chat prompt");

    // Should only have the new message
    assert_eq!(result.prompt_text, "[user]: First message");
    assert_eq!(result.message_count, 0);
    assert!(!result.truncated);
}

#[tokio::test]
async fn test_build_chat_prompt_truncation() {
    let db = create_test_db().await;
    let session_id = "test-session-truncate";
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    create_test_session(&db, session_id, &tenant_id).await;

    // Add many messages to exceed token budget
    for i in 0..10 {
        add_test_message(
            &db,
            session_id,
            if i % 2 == 0 { "user" } else { "assistant" },
            &format!(
                "This is message number {} with some extra content to increase token count.",
                i
            ),
        )
        .await;
        // Ensure ordering
        tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
    }

    // Use a small token budget to force truncation
    let config = ChatContextConfig {
        max_history_messages: 20,
        max_history_tokens: 100, // Very small budget
        include_system_messages: true,
    };

    let result = build_chat_prompt(&db, session_id, "New message", &config)
        .await
        .expect("Failed to build chat prompt");

    // Should be truncated
    assert!(result.truncated);
    assert!(result.message_count < 10);

    // Most recent messages should be kept
    assert!(result.prompt_text.contains("[user]: New message"));
}

#[tokio::test]
async fn test_build_chat_prompt_determinism() {
    let db = create_test_db().await;
    let session_id = "test-session-determinism";
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    create_test_session(&db, session_id, &tenant_id).await;

    add_test_message(&db, session_id, "user", "Hello").await;
    // Delay for timestamp ordering - use 50ms for CI reliability
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    add_test_message(&db, session_id, "assistant", "Hi there").await;

    let config = ChatContextConfig::default();

    // Build prompt twice
    let result1 = build_chat_prompt(&db, session_id, "Same message", &config)
        .await
        .expect("Failed to build chat prompt");

    let result2 = build_chat_prompt(&db, session_id, "Same message", &config)
        .await
        .expect("Failed to build chat prompt");

    // Must be identical (deterministic)
    assert_eq!(result1.prompt_text, result2.prompt_text);
    assert_eq!(result1.context_hash, result2.context_hash);
    assert_eq!(result1.message_count, result2.message_count);
    assert_eq!(result1.truncated, result2.truncated);
}

#[tokio::test]
async fn test_streaming_infer_builds_prompt_from_chat_history() {
    let state = setup_state(None).await.expect("state");
    let caps = adapteros_api_types::workers::WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: false,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: true,
    };
    let worker_id = register_test_worker(&state, "tenant-1", caps)
        .await
        .expect("register worker");
    let manifest_hash = format!("manifest-{}", worker_id);
    let state = state.with_manifest_info(manifest_hash, "mlx".to_string());

    let adapter_id = format!("adapter-chat-context-{}", uuid::Uuid::new_v4());
    create_test_adapter_default(&state, &adapter_id, "tenant-1")
        .await
        .expect("create adapter");

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test".to_string(),
    );

    let session_id = "streaming-session-001";
    create_test_session(&state.db, session_id, &claims.tenant_id).await;
    add_test_message(&state.db, session_id, "user", "Earlier message").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    add_test_message(&state.db, session_id, "assistant", "Earlier response").await;

    let chat_config = state
        .config
        .read()
        .expect("config read")
        .chat_context
        .clone();
    let expected = build_chat_prompt(
        &state.db,
        session_id,
        "New user message",
        &chat_config,
    )
    .await
    .expect("build chat prompt");
    assert!(
        expected.prompt_text.contains("Earlier message"),
        "expected history in prompt: {}",
        expected.prompt_text
    );

    #[derive(Clone)]
    struct TestWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut guard = self.0.lock().expect("lock");
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let make_writer = {
        let buffer = buffer.clone();
        move || TestWriter(buffer.clone())
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(make_writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .finish();

    let _guard = tracing::subscriber::set_default(subscriber);

    let req = StreamingInferRequest {
        prompt: "New user message".to_string(),
        model: None,
        backend: None,
        coreml_mode: None,
        routing_determinism_mode: None,
        stack_id: None,
        domain: None,
        max_tokens: 16,
        temperature: 0.7,
        top_p: None,
        top_k: None,
        stop: Vec::new(),
        adapter_stack: None,
        adapters: Some(vec![adapter_id]),
        seed: None,
        adapter_strength_overrides: None,
        require_evidence: false,
        reasoning_mode: false,
        collection_id: None,
        session_id: Some(session_id.to_string()),
        effective_adapter_ids: None,
        stop_policy: None,
        context: None,
    };

    let _ = streaming_infer(
        State(state),
        Extension(claims),
        Extension(identity),
        axum::http::HeaderMap::new(),
        None,
        Json(req),
    )
    .await
    .expect("sse response");

    let logs = String::from_utf8(buffer.lock().expect("lock").clone()).expect("utf8 logs");
    assert!(
        logs.contains("Built multi-turn prompt from session history"),
        "expected streaming handler to build prompt from chat history, logs: {}",
        logs
    );
    assert!(
        logs.contains(session_id),
        "expected session_id in logs, logs: {}",
        logs
    );
}

#[tokio::test]
async fn test_build_chat_prompt_excludes_system_messages() {
    let db = create_test_db().await;
    let session_id = "test-session-no-system";
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    create_test_session(&db, session_id, &tenant_id).await;

    add_test_message(&db, session_id, "system", "System prompt").await;
    // Delay for timestamp ordering - use 50ms for CI reliability
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    add_test_message(&db, session_id, "user", "User message").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    add_test_message(&db, session_id, "assistant", "Assistant response").await;

    // Config that excludes system messages
    let config = ChatContextConfig {
        max_history_messages: 20,
        max_history_tokens: 4096,
        include_system_messages: false,
    };

    let result = build_chat_prompt(&db, session_id, "New message", &config)
        .await
        .expect("Failed to build chat prompt");

    // System message should be excluded
    assert!(
        !result.prompt_text.contains("[system]:"),
        "System message should be excluded: {}",
        result.prompt_text
    );
    assert!(result.prompt_text.contains("[user]: User message"));
    assert!(result
        .prompt_text
        .contains("[assistant]: Assistant response"));

    // Only 2 history messages (user + assistant, not system)
    assert_eq!(result.message_count, 2);
}

#[tokio::test]
async fn test_build_chat_prompt_nonexistent_session() {
    let db = create_test_db().await;

    let config = ChatContextConfig::default();

    // Session doesn't exist - should still work but with no history
    // (get_chat_messages returns empty vec for nonexistent session)
    let result = build_chat_prompt(&db, "nonexistent-session", "Hello", &config)
        .await
        .expect("Should handle nonexistent session gracefully");

    assert_eq!(result.prompt_text, "[user]: Hello");
    assert_eq!(result.message_count, 0);
}

#[tokio::test]
async fn test_context_hash_changes_with_different_messages() {
    let db = create_test_db().await;
    let tenant_id = create_test_tenant(&db, "test-tenant").await;

    // Session 1
    let session1 = "test-session-hash-1";
    create_test_session(&db, session1, &tenant_id).await;
    add_test_message(&db, session1, "user", "Message A").await;

    // Session 2 with different message
    let session2 = "test-session-hash-2";
    create_test_session(&db, session2, &tenant_id).await;
    add_test_message(&db, session2, "user", "Message B").await;

    let config = ChatContextConfig::default();

    let result1 = build_chat_prompt(&db, session1, "New", &config)
        .await
        .expect("Failed");
    let result2 = build_chat_prompt(&db, session2, "New", &config)
        .await
        .expect("Failed");

    // Different message IDs should produce different hashes
    assert_ne!(result1.context_hash, result2.context_hash);
}
