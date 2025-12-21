use adapteros_db::{users::Role, Db};
use blake3::Hasher;

async fn init_db() -> anyhow::Result<Db> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;

    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-1")
        .bind("Tenant One")
        .execute(db.pool())
        .await?;

    Ok(db)
}

#[tokio::test]
async fn api_keys_create_lookup_revoke() -> anyhow::Result<()> {
    let db = init_db().await?;
    let user_id = db
        .create_user(
            "user@example.com",
            "Example User",
            "pw_hash",
            Role::Operator,
            "tenant-1",
        )
        .await?;

    let mut hasher = Hasher::new();
    hasher.update(b"test-token");
    let hash = hasher.finalize().to_hex().to_string();

    let scopes = vec![Role::Operator];
    let key_id = db
        .create_api_key("tenant-1", &user_id, "test-key", &scopes, &hash)
        .await?;

    let listed = db.list_api_keys("tenant-1").await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, key_id);

    let fetched = db
        .get_api_key_by_hash(&hash, false)
        .await?
        .expect("key should exist");
    assert_eq!(fetched.tenant_id, "tenant-1");
    assert!(fetched.revoked_at.is_none());
    let parsed_scopes = fetched.parsed_scopes()?;
    assert_eq!(parsed_scopes, scopes);

    db.revoke_api_key("tenant-1", &key_id).await?;

    let after_revoke = db.get_api_key_by_hash(&hash, false).await?;
    assert!(after_revoke.is_none());

    let revoked_entry = db
        .get_api_key_by_hash(&hash, true)
        .await?
        .expect("revoked key should still be returned when include_revoked");
    assert!(revoked_entry.revoked_at.is_some());

    Ok(())
}
