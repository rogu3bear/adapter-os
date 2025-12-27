use adapteros_boot::jti_cache::JtiCacheStore;
use adapteros_core::time;
use adapteros_db::Db;
use adapteros_federation::peer::{DiscoveryAnnouncement, PeerRegistry};
use adapteros_secd::federation_auth::{validate_federation_token, FederationAuthError};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use rand::rngs::OsRng;
use std::sync::Arc;
use tempfile::TempDir;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

async fn setup_registry() -> TestResult<(PeerRegistry, TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("federation_security.db");
    let db = Db::connect(db_path.to_str().unwrap()).await?;
    db.migrate().await?;
    let registry = PeerRegistry::new(Arc::new(db));
    Ok((registry, temp_dir))
}

#[tokio::test]
async fn discovery_handshake_rejects_clock_drift() -> TestResult<()> {
    let (registry, _tmp) = setup_registry().await?;

    let announcement = DiscoveryAnnouncement {
        sender_id: "peer-a".to_string(),
        known_peers: vec![],
        // Intentionally skew by 10s (>5000ms tolerance)
        announcement_time: time::unix_timestamp_secs() + 10,
        federation_epoch: 1,
    };

    let err = registry
        .process_discovery_announcement(&announcement)
        .await
        .expect_err("handshake should be rejected for clock drift");

    assert!(
        err.to_string().contains("TimeSyncRequired"),
        "Expected TimeSyncRequired error, got {err}"
    );

    Ok(())
}

#[test]
fn replayed_handshake_token_is_rejected() -> TestResult<()> {
    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();

    let now = time::unix_timestamp_secs() as i64;
    let token = build_federation_token(&signing_key, "nonce-123", now + 60, now);

    let cache_dir = tempfile::tempdir()?;
    let cache_path = cache_dir.path().join("jti_cache.json");
    let mut jti_cache = JtiCacheStore::load_or_new_with_capacity(cache_path, 8);

    let claims = validate_federation_token(&token, &verifying_key, &mut jti_cache)?;
    assert_eq!(claims.jti, "nonce-123");

    let err = validate_federation_token(&token, &verifying_key, &mut jti_cache)
        .expect_err("replayed token should be rejected");
    assert!(
        matches!(err, FederationAuthError::ReplayDetected(_)),
        "Expected replay error, got {err:?}"
    );

    Ok(())
}

fn build_federation_token(signing_key: &SigningKey, jti: &str, exp: i64, iat: i64) -> String {
    let header = serde_json::json!({"alg": "EdDSA", "typ": "JWT"});
    let claims = serde_json::json!({
        "iss": "federation",
        "aud": "mesh",
        "jti": jti,
        "exp": exp,
        "iat": iat
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap());
    let claims_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap());

    let signing_input = format!("{}.{}", header_b64, claims_b64);
    let signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    format!("{}.{}", signing_input, sig_b64)
}
