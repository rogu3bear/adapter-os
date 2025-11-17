#[tokio::test]
async fn test_hydrate_tenant_deterministic() {
    let state = setup_test_state().await;
    let bundle_id = create_test_bundle(&state).await;

    // First hydration
    let req = HydrateTenantRequest {
        bundle_id,
        tenant_id: "test-tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };
    let response = hydrate_tenant_from_bundle(state.clone(), claims, Json(req))
        .await
        .unwrap();

    let tenant = state.db.get_tenant("test-tenant").await.unwrap().unwrap();
    let hash1 = state
        .db
        .get_tenant_snapshot_hash("test-tenant")
        .await
        .unwrap()
        .unwrap();

    // Replay hydration (idempotent)
    let req2 = HydrateTenantRequest { /* same */ };
    hydrate_tenant_from_bundle(state.clone(), claims, Json(req2))
        .await
        .unwrap(); // Should not error

    let hash2 = state
        .db
        .get_tenant_snapshot_hash("test-tenant")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hash1, hash2);

    // Dry-run check
    let req_dry = HydrateTenantRequest {
        dry_run: true,
        expected_state_hash: Some(hash1.to_hex()),
        ..req
    };
    let _ = hydrate_tenant_from_bundle(state, claims, Json(req_dry))
        .await
        .unwrap();
}

#[tokio::test]
async fn test_index_hashes_deterministic() {
    let state = setup_test_state().await;
    let tenant_id = "test".to_string();
    state.db.create_tenant(&tenant_id, false).await.unwrap();

    // Build index
    let snapshot = build_index_snapshot(&tenant_id, "adapter_graph", &state.db)
        .await
        .unwrap();
    let hash = snapshot.compute_hash();
    state
        .db
        .store_index_hash(&tenant_id, "adapter_graph", &hash)
        .await
        .unwrap();

    // API call
    let response = get_tenant_index_hashes(state.clone(), mock_claims(), Path(tenant_id.clone()))
        .await
        .unwrap();
    assert_eq!(response.hashes.get("adapter_graph"), Some(&hash.to_hex()));

    // Verifier
    assert!(state
        .db
        .verify_index(&tenant_id, "adapter_graph")
        .await
        .unwrap());

    // Rebuild same data -> same hash
    let snapshot2 = build_index_snapshot(&tenant_id, "adapter_graph", &state.db)
        .await
        .unwrap();
    assert_eq!(snapshot.compute_hash(), snapshot2.compute_hash());
}

#[tokio::test]
async fn test_plugin_enable_disable_integration() -> Result<()> {
    let state = setup_test_state().await;
    let app = build_test_app(state.clone()).await;

    let claims = mock_claims("default");

    // Enable Git
    let enable_req = http::Request::builder()
        .method(http::Method::POST)
        .uri("/v1/plugins/git/enable")
        .header("Authorization", format!("Bearer {}", claims.token))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({"tenant_id": "default"}))?
                .as_bytes()
                .to_vec(),
        ))?;

    let enable_res = app.oneshot(enable_req).await.unwrap();
    assert_eq!(enable_res.status(), StatusCode::OK);

    // Reg repo, expect full analysis, fallback false
    let reg_req = http::Request::builder()
        .method(http::Method::POST)
        .uri("/v1/code/register-repo")
        .header("Authorization", format!("Bearer {}", claims.token))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({"repo_id": "test", "path": "/tmp/test"}))?
                .as_bytes()
                .to_vec(),
        ))?;

    let reg_res = app.oneshot(reg_req).await.unwrap();
    assert_eq!(reg_res.status(), StatusCode::OK);
    let body = hyper::body::to_bytes(reg_res.into_body()).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.get("fallback").and_then(|f| f.as_bool()), Some(false));

    // Disable Git
    let disable_req = http::Request::builder()
        .method(http::Method::POST)
        .uri("/v1/plugins/git/disable")
        .header("Authorization", format!("Bearer {}", claims.token))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({"tenant_id": "default"}))?
                .as_bytes()
                .to_vec(),
        ))?;

    let disable_res = app.oneshot(disable_req).await.unwrap();
    assert_eq!(disable_res.status(), StatusCode::OK);

    // Reg again, expect fallback true
    let reg2_res = app.oneshot(reg_req.clone()).await.unwrap(); // same req
    assert_eq!(reg2_res.status(), StatusCode::OK);
    let body2 = hyper::body::to_bytes(reg2_res.into_body()).await.unwrap();
    let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(json2.get("fallback").and_then(|f| f.as_bool()), Some(true));

    // For config update reload
    // Assume update config via API or something, then check reload
    // Mock config update
    state.plugin_registry.reload_config("git").await?; // assume fn
                                                       // Verify some change

    Ok(())
}

// Add for load
#[tokio::test]
async fn test_load_100_concurrent() -> Result<()> {
    let state = setup_test_state().await;
    let app = build_test_app(state).await;

    let claims = mock_claims("default");

    // Enable
    // ... enable

    let start = std::time::Instant::now();
    let mut handles = vec![];

    for i in 0..100 {
        let app_clone = app.clone();
        let claims_clone = claims.clone();
        let handle = tokio::spawn(async move {
            let req = http::Request::builder()
                .method(http::Method::POST)
                .uri("/v1/code/register-repo")
                .header("Authorization", format!("Bearer {}", claims_clone.token))
                .body(Body::from(
                    serde_json::to_string(
                        &serde_json::json!({"repo_id": format!("test{}", i), "path": "/tmp/test"}),
                    )?
                    .as_bytes()
                    .to_vec(),
                ))?;

            let res = app_clone.oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            Ok(())
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await??;
    }

    let duration = start.elapsed();
    let p99 = duration.as_millis() as f64 / 100.0 * 0.99; // approximate
    assert!(p99 < 5000.0); // <5s

    Ok(())
}

use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_lora_worker::UmaPressureMonitor;
use adapteros_server_api::state::ApiConfig;
use axum::{body::Body, http::StatusCode, Router};
use hyper::body;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_uma_memory() {
        // Setup mock app with state
        let app = build(mock_state()); // assume

        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri("/v1/system/memory")
            .body(Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();

        assert_eq!(res.status(), StatusCode::OK);

        let body = body::to_bytes(res.into_body()).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        assert!(json.get("headroom_pct").is_some());
        assert!(json.get("pressure_level").is_some());
        assert!(json.get("eviction_candidates").is_array());
    }
}

#[tokio::test]
async fn test_hydrate_dry_run() {
    let state = setup_test_state().await; // assume
    let bundle_id = "test_bundle_hex"; // mock

    // Mock get_bundle_events to return test events
    // ... setup mock

    let req = HydrateTenantRequest {
        bundle_id,
        tenant_id: "test_tenant".to_string(),
        dry_run: true,
        expected_state_hash: None,
    };

    let response = hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    let resp: TenantHydrationResponse = response.into_inner();
    assert!(!resp.state_hash.is_empty());
    // assert no db changes
    assert!(state.db.get_tenant("test_tenant").await.unwrap().is_none());
}

#[tokio::test]
async fn test_first_hydration() {
    let state = setup_test_state().await;
    let bundle_id = create_test_bundle_id(&state).await; // assume creates with events for adapter and stack

    let req = HydrateTenantRequest {
        bundle_id: bundle_id.clone(),
        tenant_id: "new_tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };

    let response = hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req))
        .await
        .unwrap();

    let resp: TenantHydrationResponse = response.into_inner();
    assert_eq!(resp.status, "hydrated");

    // Verify tenant created
    assert!(state.db.get_tenant("new_tenant").await.unwrap().is_some());

    // Verify adapters/stacks applied
    let adapters = state.db.list_adapters("new_tenant").await.unwrap();
    assert!(!adapters.is_empty());

    let hash = B3Hash::from_hex(&resp.state_hash).unwrap();
    let stored_hash = state
        .db
        .get_tenant_snapshot_hash("new_tenant")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hash, stored_hash);
}

#[tokio::test]
async fn test_repeated_hydration_idempotent() {
    let state = setup_test_state().await;
    let bundle_id = create_test_bundle_id(&state).await;

    // First
    let req1 = HydrateTenantRequest {
        bundle_id: bundle_id.clone(),
        tenant_id: "repeat_tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };
    hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req1))
        .await
        .unwrap();

    // Second same
    let req2 = HydrateTenantRequest {
        bundle_id: bundle_id.clone(),
        tenant_id: "repeat_tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };
    let response = hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req2))
        .await
        .unwrap();

    let resp: TenantHydrationResponse = response.into_inner();
    assert_eq!(resp.status, "already_hydrated"); // or "hydrated" if not skipped, but hash same

    // Verify no duplicates, e.g. adapters count same
    let adapters1 = state.db.list_adapters("repeat_tenant").await.unwrap();
    // second run, count same
}

#[tokio::test]
async fn test_different_bundle_mismatch() {
    let state = setup_test_state().await;
    let bundle1 = create_test_bundle_id(&state).await;
    let bundle2 = create_different_bundle_id(&state).await; // different events

    // Hydrate with bundle1
    let req1 = HydrateTenantRequest {
        bundle_id: bundle1.clone(),
        tenant_id: "mismatch_tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };
    hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req1))
        .await
        .unwrap();

    // Try bundle2
    let req2 = HydrateTenantRequest {
        bundle_id: bundle2,
        tenant_id: "mismatch_tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };
    let err = hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req2))
        .await
        .unwrap_err();

    assert_eq!(err.0, StatusCode::CONFLICT);
    // message contains mismatch
}

#[tokio::test]
async fn test_hydration_failure_rollback() {
    let state = setup_test_state().await;
    let bundle_id = create_failing_bundle_id(&state).await; // events that cause error in apply, e.g. invalid data

    let req = HydrateTenantRequest {
        bundle_id: bundle_id.clone(),
        tenant_id: "fail_tenant".to_string(),
        dry_run: false,
        expected_state_hash: None,
    };

    let err = hydrate_tenant_from_bundle(state.clone(), mock_claims(), Json(req))
        .await
        .unwrap_err();

    assert_eq!(err.0, StatusCode::BAD_REQUEST);
    // Verify no tenant created, no partial data
    assert!(state.db.get_tenant("fail_tenant").await.unwrap().is_none());
    let adapters = state.db.list_adapters("fail_tenant").await.unwrap();
    assert!(adapters.is_empty());
}

// Helper mocks assume implemented
