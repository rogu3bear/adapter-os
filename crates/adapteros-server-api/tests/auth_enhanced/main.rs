#[path = "../common/mod.rs"]
mod common;

use std::time::{SystemTime, UNIX_EPOCH};

use adapteros_api_types::auth::MfaEnrollVerifyRequest;
use adapteros_db::Db;
use adapteros_server_api::handlers::auth_enhanced::{
    bootstrap_admin_handler, mfa_start_handler, mfa_status_handler, mfa_verify_handler,
    BootstrapRequest,
};
use adapteros_server_api::state::AppState;
use axum::{extract::State, http::StatusCode, Extension, Json};
use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};

use common::{setup_state, test_admin_claims};

type HmacSha1 = Hmac<sha1::Sha1>;

async fn setup_bootstrap_state() -> AppState {
    let seeded_state = setup_state(None).await.expect("seeded state");
    let empty_db = Db::new_in_memory().await.expect("in-memory db");

    AppState::new(
        empty_db,
        seeded_state.jwt_secret.as_ref().clone(),
        seeded_state.config.clone(),
        seeded_state.metrics_exporter.clone(),
        seeded_state.metrics_collector.clone(),
        seeded_state.metrics_registry.clone(),
        seeded_state.uma_monitor.clone(),
    )
}

fn bootstrap_request(email: &str) -> BootstrapRequest {
    BootstrapRequest {
        email: email.to_string(),
        password: "bootstrap-password-123".to_string(),
        display_name: "Bootstrap Admin".to_string(),
    }
}

fn bootstrap_claims(user_id: &str, email: &str) -> adapteros_server_api::auth::Claims {
    let mut claims = test_admin_claims();
    claims.sub = user_id.to_string();
    claims.email = email.to_string();
    claims.tenant_id = "system".to_string();
    claims
}

fn totp_code_now(secret_b32: &str) -> String {
    let secret = BASE32_NOPAD
        .decode(secret_b32.as_bytes())
        .expect("decode base32 secret");

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("valid system clock")
        .as_secs();
    let counter = now / 30;

    let mut mac = HmacSha1::new_from_slice(&secret).expect("hmac key");
    mac.update(&counter.to_be_bytes());
    let result = mac.finalize().into_bytes();

    let offset = (result[19] & 0x0f) as usize;
    let binary: u32 = ((u32::from(result[offset] & 0x7f)) << 24)
        | ((u32::from(result[offset + 1])) << 16)
        | ((u32::from(result[offset + 2])) << 8)
        | u32::from(result[offset + 3]);

    format!("{:06}", binary % 1_000_000)
}

#[tokio::test]
async fn auth_enhanced_bootstrap_rejects_second_attempt() {
    let state = setup_bootstrap_state().await;
    let email = "bootstrap-admin@adapteros.local";

    let first = bootstrap_admin_handler(State(state.clone()), Json(bootstrap_request(email)))
        .await
        .expect("first bootstrap should succeed");

    assert!(!first.0.user_id.is_empty());

    let err = bootstrap_admin_handler(State(state), Json(bootstrap_request(email)))
        .await
        .expect_err("second bootstrap should fail");

    assert_eq!(err.0, StatusCode::CONFLICT);
    assert_eq!(err.1 .0.code, "BOOTSTRAP_ALREADY_COMPLETED");
}

#[tokio::test]
async fn auth_enhanced_bootstrap_then_mfa_start_verify_lifecycle() {
    let state = setup_bootstrap_state().await;
    let email = "bootstrap-mfa@adapteros.local";

    let bootstrap = bootstrap_admin_handler(State(state.clone()), Json(bootstrap_request(email)))
        .await
        .expect("bootstrap should succeed");

    let claims = bootstrap_claims(&bootstrap.0.user_id, email);

    let status_before = mfa_status_handler(State(state.clone()), Extension(claims.clone()))
        .await
        .expect("mfa status before setup");
    assert!(!status_before.0.mfa_enabled);
    assert!(status_before.0.enrolled_at.is_none());

    let start = mfa_start_handler(State(state.clone()), Extension(claims.clone()))
        .await
        .expect("mfa start should succeed");
    assert!(!start.0.secret.is_empty());
    assert!(start.0.otpauth_url.starts_with("otpauth://totp/"));

    let verify = mfa_verify_handler(
        State(state.clone()),
        Extension(claims.clone()),
        Json(MfaEnrollVerifyRequest {
            totp_code: totp_code_now(&start.0.secret),
        }),
    )
    .await
    .expect("mfa verify should succeed");

    assert!(!verify.0.backup_codes.is_empty());
    assert!(verify.0.backup_codes.iter().all(|code| !code.is_empty()));

    let status_after = mfa_status_handler(State(state), Extension(claims))
        .await
        .expect("mfa status after verify");
    assert!(status_after.0.mfa_enabled);
    assert!(status_after.0.enrolled_at.is_some());
}
