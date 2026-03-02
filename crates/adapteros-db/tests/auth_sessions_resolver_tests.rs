use adapteros_db::Db;
use tempfile::tempdir;

#[tokio::test]
async fn resolve_user_sessions_view_created() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().join("auth-fallback.sqlite");
    let db = Db::connect(db_path.to_str().unwrap()).await?;

    sqlx::query(
        r#"
        CREATE TABLE user_sessions (
            jti TEXT PRIMARY KEY,
            session_id TEXT,
            user_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            device_id TEXT,
            rot_id TEXT,
            refresh_hash TEXT,
            refresh_expires_at TEXT,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            ip_address TEXT,
            user_agent TEXT,
            last_activity TEXT NOT NULL,
            locked INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"
        INSERT INTO user_sessions (
            jti, session_id, user_id, tenant_id, device_id, rot_id, refresh_hash, refresh_expires_at,
            created_at, expires_at, ip_address, user_agent, last_activity, locked
        )
        VALUES (
            'j1', 's1', 'u1', 't1', 'dev-1', 'rot-1', 'hash-1', '2025-12-10T00:00:00Z',
            '2025-12-09T00:00:00Z', '2025-12-10T00:00:00Z', '127.0.0.1', 'agent', '2025-12-09T01:00:00Z', 0
        )
        "#,
    )
    .execute(db.pool())
    .await?;

    let table = db.resolve_session_table().await?;
    assert_eq!(table, "auth_sessions");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_sessions")
        .fetch_one(db.pool())
        .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[tokio::test]
async fn resolve_auth_sessions_preferred() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().join("auth-only.sqlite");
    let db = Db::connect(db_path.to_str().unwrap()).await?;

    sqlx::query(
        r#"
        CREATE TABLE auth_sessions (
            jti TEXT PRIMARY KEY,
            session_id TEXT,
            user_id TEXT NOT NULL,
            tenant_id TEXT NOT NULL,
            device_id TEXT,
            rot_id TEXT,
            refresh_hash TEXT,
            refresh_expires_at TEXT,
            ip_address TEXT,
            user_agent TEXT,
            created_at TEXT NOT NULL,
            last_activity TEXT NOT NULL,
            expires_at INTEGER NOT NULL,
            locked INTEGER NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"
        INSERT INTO auth_sessions (
            jti, session_id, user_id, tenant_id, device_id, rot_id, refresh_hash, refresh_expires_at,
            ip_address, user_agent, created_at, last_activity, expires_at, locked
        )
        VALUES (
            'j2', 's2', 'u2', 't2', 'dev-2', 'rot-2', 'hash-2', '2025-12-10T00:00:00Z',
            '127.0.0.2', 'agent2', '2025-12-09T00:00:00Z', '2025-12-09T02:00:00Z', 1733700000, 0
        )
        "#,
    )
    .execute(db.pool())
    .await?;

    let table = db.resolve_session_table().await?;
    assert_eq!(table, "auth_sessions");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_sessions")
        .fetch_one(db.pool())
        .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[tokio::test]
async fn resolve_missing_tables_errors() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().join("auth-missing.sqlite");
    let db = Db::connect(db_path.to_str().unwrap()).await?;

    let err = db.resolve_session_table().await.unwrap_err();
    assert!(err
        .to_string()
        .contains("Missing session table (auth_sessions or user_sessions)"));
    Ok(())
}
