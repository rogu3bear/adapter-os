//! Adapter resolution and validation tests for inference_core.

use crate::config::PathsConfig;
use crate::inference_core::{
    parse_pinned_adapter_ids, validate_pinned_within_effective_set, InferenceCore,
};
use crate::state::{ApiConfig, AppState, GeneralConfig, MetricsConfig};
use crate::telemetry::MetricsRegistry;
use crate::types::{InferenceError, InferenceRequestInternal};
use adapteros_core::{determinism_mode::DeterminismMode, BackendKind, SeedMode};
use adapteros_db::chat_sessions::CreateChatSessionParams;
use adapteros_db::traits::CreateStackRequest;
use adapteros_db::Db;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_telemetry::MetricsCollector;
use std::fs;
use std::sync::{Arc, RwLock};
use tempfile::Builder as TempDirBuilder;
use uuid::Uuid;

fn stack_name() -> String {
    format!("stack.test.{}", Uuid::new_v4().simple())
}

async fn build_test_state(use_session_stack: bool) -> AppState {
    build_test_state_with_general(use_session_stack, None).await
}

async fn build_test_state_with_general(
    use_session_stack: bool,
    general_determinism_mode: Option<DeterminismMode>,
) -> AppState {
    let base = std::path::Path::new("var/test-dbs");
    fs::create_dir_all(base).unwrap();
    let dir = TempDirBuilder::new()
        .prefix("aos-inference-core-")
        .tempdir_in(base)
        .unwrap();
    let db_path = dir.path().join("db.sqlite3");
    let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();
    let _db_dir = dir.keep();

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let general = general_determinism_mode.map(|mode| GeneralConfig {
        system_name: None,
        environment: None,
        api_base_url: None,
        determinism_mode: Some(mode),
    });

    let config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test".to_string(),
        },
        directory_analysis_timeout_secs: 120,
        use_session_stack_for_routing: use_session_stack,
        capacity_limits: Default::default(),
        general,
        server: Default::default(),
        security: Default::default(),
        auth: Default::default(),
        self_hosting: Default::default(),
        performance: Default::default(),
        paths: PathsConfig {
            artifacts_root: "var/artifacts".into(),
            bundles_root: "var/bundles".into(),
            adapters_root: "var/adapters/repo".into(),
            plan_dir: "var/plan".into(),
            datasets_root: "var/datasets".into(),
            documents_root: "var/documents".into(),
        },
        chat_context: Default::default(),
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::Auto,
        worker_id: 0,
    }));

    let metrics_exporter = Arc::new(MetricsExporter::new(vec![0.1]).unwrap());
    let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
    let metrics_registry = Arc::new(MetricsRegistry::new());
    let uma_monitor = Arc::new(adapteros_lora_worker::memory::UmaPressureMonitor::new(
        15, None,
    ));

    AppState::new(
        db,
        b"test-jwt-secret-for-effective-adapters".to_vec(),
        config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        uma_monitor,
    )
    .with_manifest_info("test-manifest-hash".to_string(), "mlx".to_string())
}

async fn insert_stack(db: &Db, tenant: &str, adapter_ids: &[&str]) -> String {
    let req = CreateStackRequest {
        tenant_id: tenant.to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: adapter_ids.iter().map(|s| s.to_string()).collect(),
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    db.insert_stack(&req).await.expect("insert stack")
}

#[tokio::test]
async fn test_resolve_effective_adapters_adapters_only() {
    let state = build_test_state(false).await;
    let core = InferenceCore::new(&state);
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
    req.adapters = Some(vec!["adapter-a".to_string(), "adapter-b".to_string()]);

    core.resolve_effective_adapters(&mut req, None)
        .await
        .expect("resolve");

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
    );
    assert!(req.stack_id.is_none());
}

#[tokio::test]
async fn test_resolve_effective_adapters_stack_only() {
    let state = build_test_state(false).await;
    let stack_id = insert_stack(&state.db, "tenant-1", &["adapter-a", "adapter-c"]).await;
    let core = InferenceCore::new(&state);
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
    req.stack_id = Some(stack_id.clone());

    core.resolve_effective_adapters(&mut req, None)
        .await
        .expect("resolve");

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["adapter-a".to_string(), "adapter-c".to_string()])
    );
    assert_eq!(req.stack_id, Some(stack_id.clone()));
    assert!(req.stack_version.is_some());
}

#[tokio::test]
async fn test_session_stack_fallback_disabled() {
    let state = build_test_state(false).await;
    let session = adapteros_db::chat_sessions::ChatSession {
        id: "s1".to_string(),
        tenant_id: "tenant-1".to_string(),
        user_id: None,
        created_by: None,
        stack_id: Some("stack-session".to_string()),
        collection_id: None,
        document_id: None,
        name: "test".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
        last_activity_at: "now".to_string(),
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: None,
        codebase_adapter_id: None,
        archived_at: None,
        status: None,
    };
    let core = InferenceCore::new(&state);
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
    req.session_id = Some(session.id.clone());

    core.resolve_effective_adapters(&mut req, Some(&session))
        .await
        .expect("resolve");

    assert!(
        req.effective_adapter_ids.is_none(),
        "fallback disabled should not use session stack"
    );
}

#[tokio::test]
async fn test_session_stack_fallback_enabled() {
    let state = build_test_state(true).await;
    let stack_id = insert_stack(&state.db, "tenant-1", &["adapter-a", "adapter-c"]).await;
    let session = adapteros_db::chat_sessions::ChatSession {
        id: "s1".to_string(),
        tenant_id: "tenant-1".to_string(),
        user_id: None,
        created_by: None,
        stack_id: Some(stack_id.clone()),
        collection_id: None,
        document_id: None,
        name: "test".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
        last_activity_at: "now".to_string(),
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: None,
        codebase_adapter_id: None,
        archived_at: None,
        status: None,
    };
    let core = InferenceCore::new(&state);
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "hi".to_string());
    req.session_id = Some(session.id.clone());

    core.resolve_effective_adapters(&mut req, Some(&session))
        .await
        .expect("resolve");

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["adapter-a".to_string(), "adapter-c".to_string()])
    );
    assert_eq!(req.stack_id, Some(stack_id));
}

#[tokio::test]
async fn test_effective_adapters_explicit_list() {
    let state = build_test_state(false).await;
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.adapters = Some(vec!["adapter-a".to_string(), "adapter-b".to_string()]);

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, None)
        .await
        .unwrap();

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
    );
    assert!(req.stack_id.is_none());
}

#[tokio::test]
async fn resolve_worker_path_requires_manifest_hash() {
    let state = build_test_state(false).await;
    let core = InferenceCore::new(&state);
    let err = core.resolve_worker_path("tenant-1").await.unwrap_err();
    match err {
        InferenceError::NoCompatibleWorker { required_hash, .. } => {
            assert_eq!(required_hash, "test-manifest-hash")
        }
        other => panic!("unexpected error: {:?}", other),
    }
}

#[tokio::test]
async fn test_effective_adapters_from_stack_id() {
    let state = build_test_state(false).await;
    let stack_req = CreateStackRequest {
        tenant_id: "tenant-1".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["stack-a".to_string(), "stack-b".to_string()],
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.stack_id = Some(stack_id.clone());

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, None)
        .await
        .unwrap();

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["stack-a".to_string(), "stack-b".to_string()])
    );
    assert_eq!(req.stack_id, Some(stack_id));
    assert_eq!(req.stack_version, Some(1));
}

#[tokio::test]
async fn test_effective_adapters_default_stack_fallback() {
    let state = build_test_state(false).await;
    let stack_req = CreateStackRequest {
        tenant_id: "tenant-1".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["default-a".to_string(), "default-b".to_string()],
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.unwrap();
    state
        .db
        .set_default_stack("tenant-1", &stack_id)
        .await
        .unwrap();

    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, None)
        .await
        .unwrap();

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["default-a".to_string(), "default-b".to_string()])
    );
    assert_eq!(req.stack_id, Some(stack_id.clone()));
    assert_eq!(req.stack_version, Some(1));

    // Active stack cache should be populated for the tenant
    let active_map = state.active_stack.read().unwrap();
    assert_eq!(
        active_map.get("tenant-1").cloned().flatten(),
        Some(stack_id.clone())
    );
}

#[tokio::test]
async fn test_stack_with_pinned_adapters_subset_allowed() {
    let state = build_test_state(false).await;
    let stack_req = CreateStackRequest {
        tenant_id: "tenant-1".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["stack-a".to_string(), "stack-b".to_string()],
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.stack_id = Some(stack_id);
    req.pinned_adapter_ids = Some(vec!["stack-b".to_string()]);

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, None)
        .await
        .unwrap();

    // Pinned adapter is part of the resolved effective set, so validation should pass
    validate_pinned_within_effective_set(&req.effective_adapter_ids, &req.pinned_adapter_ids)
        .expect("pinned adapters should be allowed");
}

#[tokio::test]
async fn test_effective_adapters_from_session_stack_when_enabled() {
    let state = build_test_state(true).await;
    let stack_req = CreateStackRequest {
        tenant_id: "tenant-1".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["s1".to_string(), "s2".to_string()],
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

    // Create a session that references the stack
    let session_id = "session-1".to_string();
    let session_params = CreateChatSessionParams {
        id: session_id.clone(),
        tenant_id: "tenant-1".to_string(),
        user_id: None,
        created_by: None,
        stack_id: Some(stack_id.clone()),
        collection_id: None,
        document_id: None,
        name: "test".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: None,
        codebase_adapter_id: None,
    };
    state.db.create_chat_session(session_params).await.unwrap();
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .unwrap()
        .unwrap();

    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.session_id = Some(session_id.clone());

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, Some(&session))
        .await
        .unwrap();

    assert_eq!(
        req.effective_adapter_ids,
        Some(vec!["s1".to_string(), "s2".to_string()])
    );
    assert_eq!(req.stack_id, Some(stack_id));
}

#[tokio::test]
async fn test_session_stack_ignored_when_disabled() {
    let state = build_test_state(false).await;
    let stack_req = CreateStackRequest {
        tenant_id: "tenant-1".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["s1".to_string()],
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

    let session_id = "session-2".to_string();
    let session_params = CreateChatSessionParams {
        id: session_id.clone(),
        tenant_id: "tenant-1".to_string(),
        user_id: None,
        created_by: None,
        stack_id: Some(stack_id),
        collection_id: None,
        document_id: None,
        name: "test".to_string(),
        title: None,
        source_type: Some("general".to_string()),
        source_ref_id: None,
        metadata_json: None,
        tags_json: None,
        pinned_adapter_ids: None,
        codebase_adapter_id: None,
    };
    state.db.create_chat_session(session_params).await.unwrap();
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .unwrap()
        .unwrap();

    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.session_id = Some(session_id);

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, Some(&session))
        .await
        .unwrap();

    // Without the flag, we should not inherit session.stack_id
    assert!(req.effective_adapter_ids.is_none());
    assert!(req.stack_id.is_none());
}

#[tokio::test]
async fn test_pinned_not_in_effective_set_rejected_in_core() {
    let state = build_test_state(false).await;
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.adapters = Some(vec!["adapter-a".to_string()]);
    req.pinned_adapter_ids = Some(vec!["adapter-b".to_string()]);

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, None)
        .await
        .unwrap();

    let err =
        validate_pinned_within_effective_set(&req.effective_adapter_ids, &req.pinned_adapter_ids)
            .expect_err("pinned adapter not in effective set should be rejected");

    match err {
        InferenceError::ValidationError(msg) => {
            assert!(
                msg.contains("adapter-b"),
                "error message should name the pinned adapter: {}",
                msg
            );
        }
        other => panic!("expected ValidationError, got {:?}", other),
    }
}

#[tokio::test]
async fn test_pinned_adapter_missing_rejected() {
    let state = build_test_state(false).await;
    let core = InferenceCore::new(&state);
    let err = core
        .validate_pinned_adapters_for_tenant("tenant-1", &[String::from("missing-pin")])
        .await
        .unwrap_err();

    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains("missing-pin"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_pinned_adapter_wrong_tenant_rejected() {
    let state = build_test_state(false).await;
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-2', 'Other Tenant')",
    )
    .execute(state.db.pool())
    .await
    .unwrap();

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id("tenant-2")
        .adapter_id("tenant2-adapter")
        .name("Tenant 2 Adapter")
        .hash_b3("b3:tenant2")
        .rank(4)
        .build()
        .unwrap();
    state.db.register_adapter(params).await.unwrap();

    let core = InferenceCore::new(&state);
    let err = core
        .validate_pinned_adapters_for_tenant("tenant-1", &[String::from("tenant2-adapter")])
        .await
        .unwrap_err();

    // PRD-RECT-001: Cross-tenant access returns AdapterNotFound (not PermissionDenied)
    // to prevent tenant enumeration attacks.
    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains("tenant2-adapter"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_pinned_adapter_outside_allowlist_rejected() {
    let state = build_test_state(false).await;
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-2', 'Other Tenant')",
    )
    .execute(state.db.pool())
    .await
    .unwrap();

    // Register one adapter for each tenant
    let tenant1_params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id("tenant-1")
        .adapter_id("t1-allowed")
        .name("Tenant1")
        .hash_b3("b3:t1")
        .rank(4)
        .build()
        .unwrap();
    state.db.register_adapter(tenant1_params).await.unwrap();

    let tenant2_params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .tenant_id("tenant-2")
        .adapter_id("t2-disallowed")
        .name("Tenant2")
        .hash_b3("b3:t2")
        .rank(4)
        .build()
        .unwrap();
    state.db.register_adapter(tenant2_params).await.unwrap();

    let core = InferenceCore::new(&state);
    let allowlist = core
        .adapter_allowlist_for_tenant("tenant-1")
        .await
        .expect("allowlist");

    let err = core
        .validate_ids_against_allowlist(
            &[String::from("t2-disallowed")],
            "tenant-1",
            &allowlist,
            "Pinned adapter",
        )
        .unwrap_err();

    // PRD-RECT-001: Allowlist violations return AdapterNotFound (not PermissionDenied)
    // to prevent leaking adapter existence across tenants.
    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains("t2-disallowed"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_stack_from_other_tenant_not_resolved() {
    let state = build_test_state(false).await;
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('tenant-2', 'Other Tenant')",
    )
    .execute(state.db.pool())
    .await
    .unwrap();

    let stack_req = CreateStackRequest {
        tenant_id: "tenant-2".to_string(),
        name: stack_name(),
        description: None,
        adapter_ids: vec!["cross-a".to_string()],
        workflow_type: None,
        determinism_mode: None,
        routing_determinism_mode: None,
    };
    let stack_id = state.db.insert_stack(&stack_req).await.unwrap();

    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.stack_id = Some(stack_id.clone());

    let core = InferenceCore::new(&state);
    let err = core
        .resolve_effective_adapters(&mut req, None)
        .await
        .unwrap_err();

    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains("tenant-1"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }
}

#[tokio::test]
async fn test_bad_adapter_id_rejected() {
    let state = build_test_state(false).await;
    let mut req = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    req.adapters = Some(vec!["missing-adapter".to_string()]);

    let core = InferenceCore::new(&state);
    core.resolve_effective_adapters(&mut req, None)
        .await
        .unwrap();

    let err = core.validate_adapters_loadable(&req).await.unwrap_err();
    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains("missing-adapter"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }
}

#[test]
fn test_parse_pinned_adapter_ids_valid_json() {
    let result = parse_pinned_adapter_ids(Some(r#"["adapter-a", "adapter-b"]"#));
    assert_eq!(
        result,
        Some(vec!["adapter-a".to_string(), "adapter-b".to_string()])
    );
}

#[test]
fn test_parse_pinned_adapter_ids_empty_array() {
    let result = parse_pinned_adapter_ids(Some("[]"));
    assert_eq!(result, Some(vec![]));
}

#[test]
fn test_parse_pinned_adapter_ids_none_input() {
    let result = parse_pinned_adapter_ids(None);
    assert!(result.is_none());
}

#[test]
fn test_parse_pinned_adapter_ids_invalid_json() {
    // Malformed JSON should return None (not panic)
    let result = parse_pinned_adapter_ids(Some("not valid json"));
    assert!(result.is_none());
}
