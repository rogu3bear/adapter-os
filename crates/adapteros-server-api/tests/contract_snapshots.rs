use adapteros_db::sqlx;
use adapteros_server_api::auth::{
    generate_token_ed25519_with_admin_tenants_mfa, generate_token_with_admin_tenants_mfa,
};
use adapteros_server_api::handlers::streaming_infer::{Delta, StreamingChoice, StreamingChunk};
use adapteros_server_api::sse::{SseEventManager, SseStreamType};
use adapteros_server_api::{create_app, AppState};
use axum::{
    body::{to_bytes, Body},
    http::{HeaderMap, Method, Request, StatusCode},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tower::ServiceExt;

mod common;

fn snapshot_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    // Keep snapshots alongside tests to make drift obvious and reviewable.
    settings.set_snapshot_path("contracts");
    settings.set_sort_maps(true);
    settings
}

fn header_subset(headers: &HeaderMap, keys: &[&str]) -> BTreeMap<String, String> {
    let mut subset = BTreeMap::new();
    for key in keys {
        if let Some(value) = headers.get(*key) {
            if let Ok(value_str) = value.to_str() {
                subset.insert((*key).to_string(), value_str.to_string());
            }
        }
    }
    subset
}

fn redact_pii(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                let k = key.as_str();
                if matches!(
                    k,
                    "token"
                        | "email"
                        | "user_id"
                        | "session_id"
                        | "jti"
                        | "ip_address"
                        | "expires_in"
                        | "expires_at"
                        | "run_head_hash"
                        | "output_digest"
                        | "receipt_digest"
                        | "previous_hash"
                        | "entry_hash"
                        | "created_by"
                        | "request_id"
                        | "trace_id"
                        | "boot_trace_id"
                ) || k.ends_with("_hash")
                    || k.ends_with("_digest")
                {
                    *val = Value::String("<redacted>".to_string());
                    continue;
                }

                if k.ends_with("_at") || k == "timestamp" || k == "timestamp_ms" {
                    *val = Value::String("<timestamp>".to_string());
                    continue;
                }

                redact_pii(val);
            }
        }
        Value::Array(items) => {
            for v in items {
                redact_pii(v);
            }
        }
        _ => {}
    }
}

async fn json_request(
    app: &axum::Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method.clone()).uri(path);
    if matches!(method, Method::POST | Method::PUT | Method::PATCH) {
        builder = builder.header("content-type", "application/json");
    }
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {}", t));
    }

    let body = match body {
        Some(b) => Body::from(b.to_string()),
        None => Body::empty(),
    };

    let response = app
        .clone()
        .oneshot(builder.body(body).expect("request build"))
        .await
        .expect("router response");

    let status = response.status();
    let bytes = to_bytes(response.into_body(), 4 * 1024 * 1024)
        .await
        .expect("read body");
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };

    (status, json)
}

async fn head_request(
    app: &axum::Router,
    path: &str,
    token: Option<&str>,
) -> (StatusCode, HeaderMap) {
    let mut builder = Request::builder().method(Method::HEAD).uri(path);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {}", t));
    }

    let response = app
        .clone()
        .oneshot(builder.body(Body::empty()).expect("request build"))
        .await
        .expect("router response");

    let status = response.status();
    let headers = response.headers().clone();
    (status, headers)
}

struct ContractHarness {
    app: axum::Router,
    state: AppState,
    _env: common::TestkitEnvGuard,
}

impl ContractHarness {
    async fn new() -> Self {
        let env_guard = common::TestkitEnvGuard::enabled(true).await;

        let state = common::setup_state(None).await.expect("state setup");
        let app = create_app(state.clone());
        let harness = Self {
            app,
            state,
            _env: env_guard,
        };

        harness.reset().await;
        harness.seed_minimal().await;
        harness.create_trace_fixture().await;
        harness.create_evidence_fixture().await;

        harness
    }

    async fn reset(&self) {
        let (status, _) = json_request(&self.app, Method::POST, "/testkit/reset", None, None).await;
        assert_eq!(status, StatusCode::OK, "testkit reset should succeed");
    }

    async fn seed_minimal(&self) {
        let (status, _) =
            json_request(&self.app, Method::POST, "/testkit/seed_minimal", None, None).await;
        assert_eq!(status, StatusCode::OK, "seed_minimal should succeed");
    }

    async fn create_trace_fixture(&self) {
        let (status, _) = json_request(
            &self.app,
            Method::POST,
            "/testkit/create_trace_fixture",
            Some(json!({ "token_count": 3 })),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "trace fixture should succeed");
    }

    async fn create_evidence_fixture(&self) {
        let (status, _) = json_request(
            &self.app,
            Method::POST,
            "/testkit/create_evidence_fixture",
            Some(json!({ "inference_id": "trace-fixture" })),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "evidence fixture should succeed");
    }

    async fn seed_audit_log(&self) {
        let audit_id = self
            .state
            .db
            .log_audit(
                "user-e2e",
                "admin",
                "tenant-test",
                "policy.update",
                "policy",
                Some("egress"),
                "success",
                None,
                Some("127.0.0.1"),
                Some(r#"{"source":"contract-test"}"#),
            )
            .await
            .expect("insert audit log");

        sqlx::query("UPDATE audit_logs SET timestamp = ? WHERE id = ?")
            .bind("2025-01-01T00:00:00Z")
            .bind(audit_id)
            .execute(self.state.db.pool())
            .await
            .expect("timestamp update");
    }

    async fn post(
        &self,
        path: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        json_request(&self.app, Method::POST, path, body, token).await
    }

    async fn get(&self, path: &str, token: Option<&str>) -> (StatusCode, Value) {
        json_request(&self.app, Method::GET, path, None, token).await
    }

    async fn head(&self, path: &str, token: Option<&str>) -> (StatusCode, HeaderMap) {
        head_request(&self.app, path, token).await
    }

    fn issue_token(
        &self,
        user_id: &str,
        email: &str,
        role: &str,
        tenant_id: &str,
        admin_tenants: &[String],
    ) -> String {
        let ttl_seconds = 3600;
        if self.state.use_ed25519 {
            generate_token_ed25519_with_admin_tenants_mfa(
                user_id,
                email,
                role,
                tenant_id,
                admin_tenants,
                &self.state.ed25519_keypair,
                ttl_seconds,
                None,
                Some(self.state.jwt_primary_kid.as_str()),
            )
            .expect("issue ed25519 token")
        } else {
            generate_token_with_admin_tenants_mfa(
                user_id,
                email,
                role,
                tenant_id,
                admin_tenants,
                self.state.jwt_secret.as_slice(),
                ttl_seconds,
                None,
                Some(self.state.jwt_primary_kid.as_str()),
            )
            .expect("issue hmac token")
        }
    }
}

#[tokio::test]
async fn api_contract_snapshots() {
    let harness = ContractHarness::new().await;
    let settings = snapshot_settings();

    // Healthz contract
    let (health_status, mut health_body) = harness.get("/healthz", None).await;
    redact_pii(&mut health_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "healthz_contract",
            json!({ "status": health_status.as_u16(), "body": health_body })
        );
    });

    // Readyz contract
    let (ready_status, mut ready_body) = harness.get("/readyz", None).await;
    redact_pii(&mut ready_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "readyz_contract",
            json!({ "status": ready_status.as_u16(), "body": ready_body })
        );
    });

    // Login success
    let (login_status, mut login_body) = harness
        .post(
            "/v1/auth/login",
            Some(json!({
                "email": "test@example.com",
                "password": "password"
            })),
            None,
        )
        .await;
    assert_eq!(login_status, StatusCode::OK, "login success");
    let token = login_body
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    if let Some(tenants) = login_body.get_mut("tenants").and_then(|v| v.as_array_mut()) {
        tenants.sort_by(|a, b| {
            let a_id = a.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let b_id = b.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            a_id.cmp(b_id)
        });
    }
    redact_pii(&mut login_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "auth_login_success",
            json!({ "status": login_status.as_u16(), "body": login_body })
        );
    });

    // Notifications stream handshake (HEAD preflight)
    let (stream_status, stream_headers) =
        harness.head("/v1/stream/notifications", Some(&token)).await;
    let headers = header_subset(
        &stream_headers,
        &["content-type", "cache-control", "connection"],
    );
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "notifications_stream_handshake",
            json!({ "status": stream_status.as_u16(), "headers": headers })
        );
    });

    // Auth me contract
    let viewer_claims = common::test_viewer_claims();
    let viewer_token = harness.issue_token(
        &viewer_claims.sub,
        &viewer_claims.email,
        &viewer_claims.role,
        &viewer_claims.tenant_id,
        &viewer_claims.admin_tenants,
    );
    let (auth_me_status, mut auth_me_body) = harness.get("/v1/auth/me", Some(&viewer_token)).await;
    redact_pii(&mut auth_me_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "auth_me_contract",
            json!({ "status": auth_me_status.as_u16(), "body": auth_me_body })
        );
    });

    // Auth me unauthorized
    let (auth_me_unauth_status, mut auth_me_unauth_body) = harness.get("/v1/auth/me", None).await;
    redact_pii(&mut auth_me_unauth_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "auth_me_unauthorized",
            json!({ "status": auth_me_unauth_status.as_u16(), "body": auth_me_unauth_body })
        );
    });

    // Protected endpoint forbidden (RBAC)
    let (tenant_forbidden_status, mut tenant_forbidden_body) =
        harness.get("/v1/tenants", Some(&viewer_token)).await;
    redact_pii(&mut tenant_forbidden_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "tenants_forbidden_contract",
            json!({ "status": tenant_forbidden_status.as_u16(), "body": tenant_forbidden_body })
        );
    });

    // SSE reconnect replay contract
    let manager = harness.state.sse_manager.clone();
    let _ = manager
        .create_event(
            SseStreamType::SystemMetrics,
            "metrics",
            json!({ "cpu": 0.42 }).to_string(),
        )
        .await;
    let _ = manager
        .create_event(
            SseStreamType::SystemMetrics,
            "metrics",
            json!({ "cpu": 0.84 }).to_string(),
        )
        .await;
    let mut replay_headers = HeaderMap::new();
    replay_headers.insert("Last-Event-ID", "0".parse().expect("header"));
    let last_event_id =
        SseEventManager::parse_last_event_id(&replay_headers).expect("last event id");
    let replay = manager
        .get_replay_with_analysis(SseStreamType::SystemMetrics, last_event_id)
        .await;
    let mut replay_events: Vec<Value> = replay
        .events
        .iter()
        .map(|event| serde_json::to_value(event).expect("replay event json"))
        .collect();
    for event in &mut replay_events {
        redact_pii(event);
    }
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "sse_reconnect_replay",
            json!({
                "last_event_id": last_event_id,
                "has_gap": replay.has_gap,
                "dropped_count": replay.dropped_count,
                "events": replay_events
            })
        );
    });

    // Streaming inference chunk shapes
    let token_chunk = StreamingChunk {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1735689600,
        model: "adapteros-test".to_string(),
        system_fingerprint: None,
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                role: None,
                content: Some("Hello".to_string()),
            },
            finish_reason: None,
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
        }],
    };
    let done_chunk = StreamingChunk {
        id: "chatcmpl-test".to_string(),
        object: "chat.completion.chunk".to_string(),
        created: 1735689601,
        model: "adapteros-test".to_string(),
        system_fingerprint: None,
        choices: vec![StreamingChoice {
            index: 0,
            delta: Delta {
                role: None,
                content: None,
            },
            finish_reason: Some("stop".to_string()),
            stop_reason_code: None,
            stop_reason_token_index: None,
            stop_policy_digest_b3: None,
        }],
    };
    let done_payload = format!(
        "{}\n\ndata: [DONE]",
        serde_json::to_string(&done_chunk).expect("done chunk json")
    );
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "streaming_infer_chunk_shapes",
            json!({
                "token_chunk": serde_json::to_value(&token_chunk).expect("token chunk json"),
                "done_chunk": serde_json::to_value(&done_chunk).expect("done chunk json"),
                "done_sse_data": done_payload
            })
        );
    });

    // Login failure
    let (login_fail_status, mut login_fail_body) = harness
        .post(
            "/v1/auth/login",
            Some(json!({
                "email": "test@example.com",
                "password": "wrong-password"
            })),
            None,
        )
        .await;
    redact_pii(&mut login_fail_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "auth_login_failure",
            json!({ "status": login_fail_status.as_u16(), "body": login_fail_body })
        );
    });

    // Inference stub (shape contract)
    let (infer_status, mut infer_body) = harness
        .post(
            "/testkit/inference_stub",
            Some(json!({ "prompt": "contract snapshot" })),
            Some(&token),
        )
        .await;
    redact_pii(&mut infer_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "inference_stub_contract",
            json!({ "status": infer_status.as_u16(), "body": infer_body })
        );
    });

    // Trace detail
    let (trace_status, mut trace_body) =
        harness.get("/v1/traces/trace-fixture", Some(&token)).await;
    redact_pii(&mut trace_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "trace_detail_contract",
            json!({ "status": trace_status.as_u16(), "body": trace_body })
        );
    });

    // Evidence list
    let (evidence_list_status, mut evidence_list_body) =
        harness.get("/v1/evidence", Some(&token)).await;
    redact_pii(&mut evidence_list_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "evidence_list_contract",
            json!({ "status": evidence_list_status.as_u16(), "body": evidence_list_body })
        );
    });

    // Evidence create
    let (evidence_create_status, mut evidence_create_body) = harness
        .post(
            "/v1/evidence",
            Some(json!({
                "adapter_id": "adapter-test",
                "evidence_type": "audit",
                "reference": "https://example.invalid/contract-evidence",
                "description": "Contract snapshot create payload",
                "confidence": "high",
                "metadata_json": "{\"source\":\"contract-test\"}"
            })),
            Some(&token),
        )
        .await;
    if let Some(obj) = evidence_create_body.as_object_mut() {
        if let Some(id) = obj.get_mut("id") {
            *id = Value::String("<redacted>".to_string());
        }
    }
    redact_pii(&mut evidence_create_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "evidence_create_contract",
            json!({ "status": evidence_create_status.as_u16(), "body": evidence_create_body })
        );
    });

    // Audit logs
    harness.seed_audit_log().await;
    let (audit_status, mut audit_body) = harness.get("/v1/audit/logs", Some(&token)).await;
    if let Some(obj) = audit_body.as_object_mut() {
        if let Some(logs) = obj.get_mut("logs").and_then(|v| v.as_array_mut()) {
            for log in logs {
                if let Some(log_obj) = log.as_object_mut() {
                    if let Some(id) = log_obj.get_mut("id") {
                        *id = Value::String("<redacted>".to_string());
                    }
                }
            }
        }
    }
    redact_pii(&mut audit_body);
    settings.bind(|| {
        insta::assert_json_snapshot!(
            "audit_log_contract",
            json!({ "status": audit_status.as_u16(), "body": audit_body })
        );
    });
}
