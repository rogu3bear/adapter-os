use adapteros_db::users::Role;
use adapteros_server_api::handlers::auth::auth_me;
use axum::{extract::State, Extension};

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn auth_me_returns_admin_tenants_from_claims() {
    let state = setup_state(None).await.expect("state");

    // Create a real user so auth_me can load it
    let user_id = state
        .db
        .create_user(
            "admin@example.com",
            "Admin User",
            "pw-hash",
            Role::Admin,
            "tenant-1",
        )
        .await
        .expect("user");

    let mut claims = test_admin_claims();
    claims.sub = user_id.clone();
    claims.admin_tenants = vec!["tenant-1".to_string(), "tenant-2".to_string()];

    let response = auth_me(State(state), Extension(claims))
        .await
        .expect("auth_me should succeed");
    assert_eq!(
        response.0.admin_tenants,
        vec!["tenant-1".to_string(), "tenant-2".to_string()]
    );
}

#[tokio::test]
async fn auth_me_dev_bypass_returns_wildcard_admin() {
    // Enable dev bypass for this test (debug builds only)
    std::env::set_var("AOS_DEV_NO_AUTH", "1");

    let state = setup_state(None).await.expect("state");
    let mut claims = test_admin_claims();
    claims.admin_tenants = vec!["*".to_string()];

    let response = auth_me(State(state), Extension(claims))
        .await
        .expect("auth_me should succeed");
    assert_eq!(response.0.admin_tenants, vec!["*".to_string()]);

    // Cleanup env var to avoid leaking to other tests
    std::env::remove_var("AOS_DEV_NO_AUTH");
}
