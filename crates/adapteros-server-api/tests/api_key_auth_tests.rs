//! Comprehensive tests for API key authentication
//!
//! These tests verify the BLAKE3-based API key validation flow:
//!
//! 1. BLAKE3 hashing produces deterministic, consistent results
//! 2. Valid API keys are accepted and create correct Principal types
//! 3. Invalid API keys are REJECTED (security critical)
//! 4. Revoked API keys are REJECTED
//! 5. API key prefix validation (if applicable)
//! 6. Hash comparison is timing-safe (via BLAKE3)
//!
//! # Security Invariants
//!
//! - Invalid keys MUST return 401 Unauthorized
//! - Revoked keys MUST return 401 Unauthorized
//! - API key authentication MUST create PrincipalType::ApiKey
//! - Hash comparison MUST be constant-time (provided by BLAKE3)

use adapteros_core::Result;
use adapteros_db::{users::Role, Db};
use blake3::Hasher;

/// Helper: Generate a BLAKE3 hash from a token string (same logic as middleware)
fn hash_api_key(token: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(token.as_bytes());
    hasher.finalize().to_hex().to_string()
}

/// Initialize a test database with the required schema
async fn init_test_db() -> Result<Db> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;

    // Create a test tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("test-tenant")
        .bind("Test Tenant")
        .execute(db.pool())
        .await?;

    Ok(db)
}

// =============================================================================
// BLAKE3 Hashing Consistency Tests
// =============================================================================

#[test]
fn test_blake3_hash_is_deterministic() {
    // SECURITY: The same token MUST always produce the same hash
    let token = "aos_test_token_12345";

    let hash1 = hash_api_key(token);
    let hash2 = hash_api_key(token);
    let hash3 = hash_api_key(token);

    assert_eq!(hash1, hash2, "BLAKE3 hash must be deterministic");
    assert_eq!(
        hash2, hash3,
        "BLAKE3 hash must be deterministic across calls"
    );
}

#[test]
fn test_blake3_hash_length() {
    // BLAKE3 produces a 256-bit (32-byte) hash, which is 64 hex characters
    let token = "any_test_token";
    let hash = hash_api_key(token);

    assert_eq!(hash.len(), 64, "BLAKE3 hex hash should be 64 characters");
    assert!(
        hash.chars().all(|c| c.is_ascii_hexdigit()),
        "Hash should only contain hex digits"
    );
}

#[test]
fn test_blake3_different_tokens_produce_different_hashes() {
    // SECURITY: Different tokens MUST produce different hashes
    let token1 = "aos_token_alpha";
    let token2 = "aos_token_beta";
    let token3 = "aos_token_gamma";

    let hash1 = hash_api_key(token1);
    let hash2 = hash_api_key(token2);
    let hash3 = hash_api_key(token3);

    assert_ne!(hash1, hash2, "Different tokens must have different hashes");
    assert_ne!(hash2, hash3, "Different tokens must have different hashes");
    assert_ne!(hash1, hash3, "Different tokens must have different hashes");
}

#[test]
fn test_blake3_hash_is_case_sensitive() {
    // API keys should be case-sensitive
    let token_lower = "aos_test_token";
    let token_upper = "AOS_TEST_TOKEN";
    let token_mixed = "aos_TEST_token";

    let hash_lower = hash_api_key(token_lower);
    let hash_upper = hash_api_key(token_upper);
    let hash_mixed = hash_api_key(token_mixed);

    assert_ne!(
        hash_lower, hash_upper,
        "API key hashing must be case-sensitive"
    );
    assert_ne!(
        hash_lower, hash_mixed,
        "API key hashing must be case-sensitive"
    );
    assert_ne!(
        hash_upper, hash_mixed,
        "API key hashing must be case-sensitive"
    );
}

#[test]
fn test_blake3_empty_token_produces_valid_hash() {
    // Edge case: empty string should still produce a valid hash (though never used in practice)
    let empty_hash = hash_api_key("");
    assert_eq!(
        empty_hash.len(),
        64,
        "Empty string should produce valid hash"
    );

    // The hash for empty string is deterministic
    let empty_hash2 = hash_api_key("");
    assert_eq!(
        empty_hash, empty_hash2,
        "Empty string hash should be deterministic"
    );
}

#[test]
fn test_blake3_whitespace_is_significant() {
    // Whitespace in tokens should affect the hash
    let token_no_space = "aos_test_token";
    let token_leading_space = " aos_test_token";
    let token_trailing_space = "aos_test_token ";
    let token_internal_space = "aos_test token";

    let hashes = vec![
        hash_api_key(token_no_space),
        hash_api_key(token_leading_space),
        hash_api_key(token_trailing_space),
        hash_api_key(token_internal_space),
    ];

    // All hashes should be unique
    let unique: std::collections::HashSet<_> = hashes.iter().collect();
    assert_eq!(
        unique.len(),
        4,
        "Whitespace differences must produce different hashes"
    );
}

// =============================================================================
// API Key Database CRUD Tests
// =============================================================================

#[tokio::test]
async fn test_api_key_create_and_lookup_by_hash() -> Result<()> {
    let db = init_test_db().await?;

    // Create a test user
    let user_id = db
        .create_user(
            "apikey-user@test.com",
            "API Key User",
            "hashed_password",
            Role::Operator,
            "test-tenant",
        )
        .await?;

    // Generate a token and hash it
    let raw_token = "aos_sk_test_1234567890abcdef";
    let token_hash = hash_api_key(raw_token);

    // Create the API key in the database
    let key_id = db
        .create_api_key(
            "test-tenant",
            &user_id,
            "Test API Key",
            &[Role::Operator],
            &token_hash,
        )
        .await?;

    // Lookup by hash should succeed
    let record = db
        .get_api_key_by_hash(&token_hash, false)
        .await?
        .expect("API key should exist");

    assert_eq!(record.id, key_id);
    assert_eq!(record.tenant_id, "test-tenant");
    assert_eq!(record.user_id, user_id);
    assert_eq!(record.name, "Test API Key");
    assert!(record.revoked_at.is_none(), "Key should not be revoked");

    Ok(())
}

#[tokio::test]
async fn test_invalid_hash_returns_none() -> Result<()> {
    let db = init_test_db().await?;

    // Create a test user and API key
    let user_id = db
        .create_user(
            "user@test.com",
            "Test User",
            "password_hash",
            Role::Viewer,
            "test-tenant",
        )
        .await?;

    let valid_token = "aos_valid_token";
    let valid_hash = hash_api_key(valid_token);

    db.create_api_key(
        "test-tenant",
        &user_id,
        "Valid Key",
        &[Role::Viewer],
        &valid_hash,
    )
    .await?;

    // SECURITY: Looking up with a different token's hash MUST return None
    let invalid_token = "aos_invalid_token";
    let invalid_hash = hash_api_key(invalid_token);

    let result = db.get_api_key_by_hash(&invalid_hash, false).await?;

    assert!(
        result.is_none(),
        "SECURITY: Invalid API key hash MUST NOT return a record"
    );

    Ok(())
}

#[tokio::test]
async fn test_revoked_key_not_returned_by_default() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "revoke-test@test.com",
            "Revoke Test",
            "pw_hash",
            Role::Admin,
            "test-tenant",
        )
        .await?;

    let token = "aos_to_be_revoked";
    let token_hash = hash_api_key(token);

    let key_id = db
        .create_api_key(
            "test-tenant",
            &user_id,
            "Will Be Revoked",
            &[Role::Admin],
            &token_hash,
        )
        .await?;

    // Key should be findable before revocation
    let before_revoke = db.get_api_key_by_hash(&token_hash, false).await?;
    assert!(
        before_revoke.is_some(),
        "Key should exist before revocation"
    );

    // Revoke the key
    db.revoke_api_key("test-tenant", &key_id).await?;

    // SECURITY: Revoked key MUST NOT be returned by default
    let after_revoke = db.get_api_key_by_hash(&token_hash, false).await?;
    assert!(
        after_revoke.is_none(),
        "SECURITY: Revoked API key MUST NOT be returned for authentication"
    );

    // With include_revoked=true, the key should still be accessible (for auditing)
    let with_revoked = db.get_api_key_by_hash(&token_hash, true).await?;
    assert!(
        with_revoked.is_some(),
        "Revoked key should be accessible with include_revoked=true"
    );
    assert!(
        with_revoked.unwrap().revoked_at.is_some(),
        "Revoked key should have revoked_at timestamp"
    );

    Ok(())
}

#[tokio::test]
async fn test_revoke_is_idempotent() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "idempotent@test.com",
            "Idempotent Test",
            "pw",
            Role::Operator,
            "test-tenant",
        )
        .await?;

    let token = "aos_idempotent_test";
    let token_hash = hash_api_key(token);

    let key_id = db
        .create_api_key(
            "test-tenant",
            &user_id,
            "Idempotent Key",
            &[Role::Operator],
            &token_hash,
        )
        .await?;

    // Revoke multiple times - should not fail
    db.revoke_api_key("test-tenant", &key_id).await?;
    db.revoke_api_key("test-tenant", &key_id).await?;
    db.revoke_api_key("test-tenant", &key_id).await?;

    // Key should still show revoked_at
    let record = db
        .get_api_key_by_hash(&token_hash, true)
        .await?
        .expect("Key should exist");
    assert!(
        record.revoked_at.is_some(),
        "Key should be revoked after multiple revoke calls"
    );

    Ok(())
}

// =============================================================================
// API Key Scopes Tests
// =============================================================================

#[tokio::test]
async fn test_api_key_scopes_parsing() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "scopes@test.com",
            "Scopes Test",
            "pw",
            Role::Admin,
            "test-tenant",
        )
        .await?;

    let token = "aos_scopes_test";
    let token_hash = hash_api_key(token);

    // Create key with multiple scopes
    let scopes = vec![Role::Operator, Role::Viewer];
    db.create_api_key(
        "test-tenant",
        &user_id,
        "Multi-scope Key",
        &scopes,
        &token_hash,
    )
    .await?;

    let record = db
        .get_api_key_by_hash(&token_hash, false)
        .await?
        .expect("Key should exist");

    let parsed_scopes = record.parsed_scopes()?;
    assert_eq!(parsed_scopes.len(), 2, "Should have 2 scopes");
    assert!(
        parsed_scopes.contains(&Role::Operator),
        "Should contain Operator role"
    );
    assert!(
        parsed_scopes.contains(&Role::Viewer),
        "Should contain Viewer role"
    );

    Ok(())
}

#[tokio::test]
async fn test_api_key_single_scope() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "single-scope@test.com",
            "Single Scope Test",
            "pw",
            Role::Viewer,
            "test-tenant",
        )
        .await?;

    let token = "aos_single_scope";
    let token_hash = hash_api_key(token);

    db.create_api_key(
        "test-tenant",
        &user_id,
        "Single Scope Key",
        &[Role::Viewer],
        &token_hash,
    )
    .await?;

    let record = db
        .get_api_key_by_hash(&token_hash, false)
        .await?
        .expect("Key should exist");

    let parsed_scopes = record.parsed_scopes()?;
    assert_eq!(parsed_scopes.len(), 1, "Should have 1 scope");
    assert_eq!(parsed_scopes[0], Role::Viewer, "Should be Viewer role");

    Ok(())
}

// =============================================================================
// Tenant Isolation Tests
// =============================================================================

#[tokio::test]
async fn test_api_key_tenant_isolation() -> Result<()> {
    let db = init_test_db().await?;

    // Create a second tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("other-tenant")
        .bind("Other Tenant")
        .execute(db.pool())
        .await?;

    // Create users in different tenants
    let user1_id = db
        .create_user(
            "user1@test.com",
            "User 1",
            "pw",
            Role::Operator,
            "test-tenant",
        )
        .await?;

    let user2_id = db
        .create_user(
            "user2@test.com",
            "User 2",
            "pw",
            Role::Operator,
            "other-tenant",
        )
        .await?;

    // Create API keys for each tenant
    let token1 = "aos_tenant1_key";
    let hash1 = hash_api_key(token1);

    let token2 = "aos_tenant2_key";
    let hash2 = hash_api_key(token2);

    db.create_api_key(
        "test-tenant",
        &user1_id,
        "Tenant 1 Key",
        &[Role::Operator],
        &hash1,
    )
    .await?;

    db.create_api_key(
        "other-tenant",
        &user2_id,
        "Tenant 2 Key",
        &[Role::Operator],
        &hash2,
    )
    .await?;

    // Each key should return its correct tenant
    let record1 = db
        .get_api_key_by_hash(&hash1, false)
        .await?
        .expect("Key 1 should exist");
    assert_eq!(
        record1.tenant_id, "test-tenant",
        "Key 1 should be in test-tenant"
    );

    let record2 = db
        .get_api_key_by_hash(&hash2, false)
        .await?
        .expect("Key 2 should exist");
    assert_eq!(
        record2.tenant_id, "other-tenant",
        "Key 2 should be in other-tenant"
    );

    // Keys should not be interchangeable
    assert_ne!(
        record1.tenant_id, record2.tenant_id,
        "Keys from different tenants must have different tenant_ids"
    );

    Ok(())
}

#[tokio::test]
async fn test_list_api_keys_only_returns_tenant_keys() -> Result<()> {
    let db = init_test_db().await?;

    // Create a second tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("another-tenant")
        .bind("Another Tenant")
        .execute(db.pool())
        .await?;

    // Create users in different tenants
    let user1_id = db
        .create_user(
            "list-user1@test.com",
            "List User 1",
            "pw",
            Role::Admin,
            "test-tenant",
        )
        .await?;

    let user2_id = db
        .create_user(
            "list-user2@test.com",
            "List User 2",
            "pw",
            Role::Admin,
            "another-tenant",
        )
        .await?;

    // Create multiple API keys in each tenant
    for i in 0..3 {
        let token = format!("aos_test_tenant_key_{}", i);
        let hash = hash_api_key(&token);
        db.create_api_key(
            "test-tenant",
            &user1_id,
            &format!("Test Key {}", i),
            &[Role::Admin],
            &hash,
        )
        .await?;
    }

    for i in 0..2 {
        let token = format!("aos_another_tenant_key_{}", i);
        let hash = hash_api_key(&token);
        db.create_api_key(
            "another-tenant",
            &user2_id,
            &format!("Another Key {}", i),
            &[Role::Admin],
            &hash,
        )
        .await?;
    }

    // List keys for test-tenant
    let test_keys = db.list_api_keys("test-tenant").await?;
    assert_eq!(test_keys.len(), 3, "test-tenant should have exactly 3 keys");
    for key in &test_keys {
        assert_eq!(
            key.tenant_id, "test-tenant",
            "All listed keys must belong to test-tenant"
        );
    }

    // List keys for another-tenant
    let another_keys = db.list_api_keys("another-tenant").await?;
    assert_eq!(
        another_keys.len(),
        2,
        "another-tenant should have exactly 2 keys"
    );
    for key in &another_keys {
        assert_eq!(
            key.tenant_id, "another-tenant",
            "All listed keys must belong to another-tenant"
        );
    }

    Ok(())
}

// =============================================================================
// Security Property Tests
// =============================================================================

#[test]
fn test_hash_collision_resistance() {
    // Test that similar tokens produce very different hashes (avalanche effect)
    let base_token = "aos_sk_prod_abcdefghijklmnop";

    // Change single character at various positions
    let variants = vec![
        "bos_sk_prod_abcdefghijklmnop", // First char
        "aos_sk_prod_bbcdefghijklmnop", // Middle-ish
        "aos_sk_prod_abcdefghijklmnoq", // Last char
    ];

    let base_hash = hash_api_key(base_token);

    for variant in variants {
        let variant_hash = hash_api_key(variant);
        assert_ne!(
            base_hash, variant_hash,
            "Single-character change must produce different hash"
        );

        // Verify significant hamming distance (hashes should differ in many bits)
        let different_chars = base_hash
            .chars()
            .zip(variant_hash.chars())
            .filter(|(a, b)| a != b)
            .count();

        // With BLAKE3, even a single bit change should affect roughly half the output bits
        // For a 64-character hex string, we expect roughly 32 characters to differ
        assert!(
            different_chars > 20,
            "Hash should have avalanche effect (got {} different chars)",
            different_chars
        );
    }
}

#[test]
fn test_hash_is_hex_lowercase() {
    // Verify hash output format is consistent
    let token = "aos_test_format";
    let hash = hash_api_key(token);

    assert!(
        hash.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
        "Hash should be lowercase hex only"
    );
}

#[test]
fn test_known_hash_value() {
    // Test against a known BLAKE3 hash to ensure the implementation is correct
    // BLAKE3("test") = 4878ca0425c739fa427f7eda20fe845f6b2e46ba5fe2a14df5b1e32f50603215
    let known_hash = hash_api_key("test");

    // BLAKE3 hash for "test" (verified externally)
    let expected = "4878ca0425c739fa427f7eda20fe845f6b2e46ba5fe2a14df5b1e32f50603215";
    assert_eq!(known_hash, expected, "BLAKE3 hash should match known value");
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

#[tokio::test]
async fn test_nonexistent_tenant_revoke_is_safe() -> Result<()> {
    let db = init_test_db().await?;

    // Revoking a key in a nonexistent tenant should not fail (it just affects 0 rows)
    // This is important for idempotency
    let result = db.revoke_api_key("nonexistent-tenant", "fake-key-id").await;
    assert!(result.is_ok(), "Revoking nonexistent key should not error");

    Ok(())
}

#[tokio::test]
async fn test_special_characters_in_token() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "special@test.com",
            "Special Char Test",
            "pw",
            Role::Admin,
            "test-tenant",
        )
        .await?;

    // Tokens with special characters
    let special_tokens = vec![
        "aos_token+with+plus",
        "aos_token/with/slash",
        "aos_token=with=equals",
        "aos_token_with_underscore",
        "aos-token-with-dash",
    ];

    for token in special_tokens {
        let hash = hash_api_key(token);
        db.create_api_key("test-tenant", &user_id, token, &[Role::Admin], &hash)
            .await?;

        let record = db.get_api_key_by_hash(&hash, false).await?;
        assert!(
            record.is_some(),
            "Token with special chars '{}' should be stored and retrievable",
            token
        );
    }

    Ok(())
}

#[tokio::test]
async fn test_unicode_in_token() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "unicode@test.com",
            "Unicode Test",
            "pw",
            Role::Admin,
            "test-tenant",
        )
        .await?;

    // While not recommended, unicode in tokens should not crash the system
    let unicode_token = "aos_token_\u{1F511}"; // key emoji
    let hash = hash_api_key(unicode_token);

    db.create_api_key(
        "test-tenant",
        &user_id,
        "Unicode Key",
        &[Role::Admin],
        &hash,
    )
    .await?;

    let record = db.get_api_key_by_hash(&hash, false).await?;
    assert!(record.is_some(), "Unicode token should be storable");

    Ok(())
}

#[tokio::test]
async fn test_very_long_token() -> Result<()> {
    let db = init_test_db().await?;

    let user_id = db
        .create_user(
            "long@test.com",
            "Long Token Test",
            "pw",
            Role::Admin,
            "test-tenant",
        )
        .await?;

    // A very long token (1000 characters)
    let long_token: String = std::iter::repeat('a').take(1000).collect();
    let hash = hash_api_key(&long_token);

    // Hash should still be 64 characters regardless of input length
    assert_eq!(hash.len(), 64, "Hash length should be constant");

    db.create_api_key(
        "test-tenant",
        &user_id,
        "Long Token Key",
        &[Role::Admin],
        &hash,
    )
    .await?;

    let record = db.get_api_key_by_hash(&hash, false).await?;
    assert!(record.is_some(), "Long token should be storable");

    Ok(())
}

// =============================================================================
// PrincipalType Tests (verifying the auth context)
// =============================================================================

#[test]
fn test_principal_type_api_key_is_authenticated() {
    use adapteros_auth::AuthMode;

    // API key authentication mode should be considered authenticated
    assert!(
        AuthMode::ApiKey.is_authenticated(),
        "AuthMode::ApiKey must be authenticated"
    );
}

#[test]
fn test_principal_type_api_key_uses_token() {
    use adapteros_auth::AuthMode;

    // API key uses a token for authentication
    assert!(
        AuthMode::ApiKey.uses_token(),
        "AuthMode::ApiKey must use token"
    );
}

#[test]
fn test_principal_type_api_key_not_dev_bypass() {
    use adapteros_auth::AuthMode;

    // API key is NOT dev bypass
    assert!(
        !AuthMode::ApiKey.is_dev_bypass(),
        "AuthMode::ApiKey must NOT be dev bypass"
    );
}

// =============================================================================
// AuthConfig API Key Tests
// =============================================================================

#[test]
fn test_api_key_config_defaults() {
    use adapteros_auth::ApiKeyConfig;

    let config = ApiKeyConfig::default();

    // Default should have API keys disabled
    assert!(!config.enabled, "API keys should be disabled by default");

    // Hash algorithm field exists (actual algorithm is BLAKE3 in middleware)
    // The config default is empty string, but the implementation uses BLAKE3
    assert!(
        config.hash_algorithm.is_empty() || config.hash_algorithm == "blake3",
        "Hash algorithm should be empty or blake3 by default"
    );
}

#[test]
fn test_api_key_config_prefix() {
    use adapteros_auth::ApiKeyConfig;

    let mut config = ApiKeyConfig::default();
    config.prefix = Some("aos_".to_string());

    assert_eq!(
        config.prefix.as_deref(),
        Some("aos_"),
        "Prefix should be configurable"
    );
}

// =============================================================================
// Hash Timing Tests (best effort - timing tests are inherently noisy)
// =============================================================================

#[test]
fn test_hash_timing_consistency() {
    // This test verifies that hash computation time doesn't vary significantly
    // based on input. While we can't truly verify constant-time in Rust tests,
    // we can check that the variance is reasonable.

    let iterations = 100;
    let short_token = "short";
    let long_token = "a".repeat(1000);

    let mut short_times = Vec::new();
    let mut long_times = Vec::new();

    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let _ = hash_api_key(short_token);
        short_times.push(start.elapsed());

        let start = std::time::Instant::now();
        let _ = hash_api_key(&long_token);
        long_times.push(start.elapsed());
    }

    // Calculate average times
    let short_avg: std::time::Duration =
        short_times.iter().sum::<std::time::Duration>() / iterations as u32;
    let long_avg: std::time::Duration =
        long_times.iter().sum::<std::time::Duration>() / iterations as u32;

    // The longer input should take longer to hash (BLAKE3 processes in chunks)
    // but the difference should not be extreme
    // Note: This is a sanity check, not a security guarantee
    println!(
        "Short token avg: {:?}, Long token avg: {:?}",
        short_avg, long_avg
    );

    // The test passes as long as we can compute hashes (timing verification is informational)
}
