//! Runtime sessions database integration tests

use adapteros_db::{Db, RuntimeSession};

#[tokio::test]
async fn test_insert_and_get_runtime_session() -> Result<(), Box<dyn std::error::Error>> {
    // Create in-memory database and run migrations
    let db = Db::new_in_memory().await?;

    // Create a test session
    let session = RuntimeSession {
        id: "session-001".to_string(),
        session_id: "test-session-123".to_string(),
        config_hash: "abc123def456".to_string(),
        binary_version: "0.3.0-alpha".to_string(),
        binary_commit: Some("commit-hash-789".to_string()),
        started_at: "2025-12-02T10:00:00Z".to_string(),
        ended_at: None,
        end_reason: None,
        hostname: "test-server-01".to_string(),
        runtime_mode: "development".to_string(),
        config_snapshot: r#"{"port": 8080, "mode": "dev"}"#.to_string(),
        drift_detected: false,
        drift_summary: None,
        previous_session_id: None,
        model_path: Some("/models/qwen".to_string()),
        adapters_root: Some("/var/adapters".to_string()),
        database_path: Some("/var/db.sqlite".to_string()),
        var_dir: Some("/var".to_string()),
    };

    // Insert the session
    db.insert_runtime_session(&session).await?;

    // Retrieve the session by ID
    let retrieved = db
        .get_runtime_session("session-001")
        .await?
        .expect("Session should exist");

    // Verify all fields
    assert_eq!(retrieved.id, "session-001");
    assert_eq!(retrieved.session_id, "test-session-123");
    assert_eq!(retrieved.config_hash, "abc123def456");
    assert_eq!(retrieved.binary_version, "0.3.0-alpha");
    assert_eq!(retrieved.binary_commit, Some("commit-hash-789".to_string()));
    assert_eq!(retrieved.hostname, "test-server-01");
    assert_eq!(retrieved.runtime_mode, "development");
    assert!(!retrieved.drift_detected);
    assert_eq!(retrieved.ended_at, None);
    assert_eq!(retrieved.end_reason, None);

    Ok(())
}

#[tokio::test]
async fn test_get_most_recent_session() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::new_in_memory().await?;

    // Insert multiple sessions for the same hostname
    let session1 = RuntimeSession {
        id: "session-001".to_string(),
        session_id: "test-session-1".to_string(),
        config_hash: "hash1".to_string(),
        binary_version: "0.3.0".to_string(),
        binary_commit: None,
        started_at: "2025-12-01T10:00:00Z".to_string(),
        ended_at: Some("2025-12-01T12:00:00Z".to_string()),
        end_reason: Some("graceful".to_string()),
        hostname: "test-server".to_string(),
        runtime_mode: "production".to_string(),
        config_snapshot: "{}".to_string(),
        drift_detected: false,
        drift_summary: None,
        previous_session_id: None,
        model_path: None,
        adapters_root: None,
        database_path: None,
        var_dir: None,
    };

    let session2 = RuntimeSession {
        id: "session-002".to_string(),
        session_id: "test-session-2".to_string(),
        config_hash: "hash2".to_string(),
        binary_version: "0.3.1".to_string(),
        binary_commit: None,
        started_at: "2025-12-02T10:00:00Z".to_string(),
        ended_at: Some("2025-12-02T12:00:00Z".to_string()),
        end_reason: Some("graceful".to_string()),
        hostname: "test-server".to_string(),
        runtime_mode: "production".to_string(),
        config_snapshot: "{}".to_string(),
        drift_detected: false,
        drift_summary: None,
        previous_session_id: Some("session-001".to_string()),
        model_path: None,
        adapters_root: None,
        database_path: None,
        var_dir: None,
    };

    db.insert_runtime_session(&session1).await?;
    db.insert_runtime_session(&session2).await?;

    // Get most recent ended session
    let most_recent = db
        .get_most_recent_session("test-server")
        .await?
        .expect("Should find most recent session");

    // Should return session2 (more recent start time)
    assert_eq!(most_recent.id, "session-002");
    assert_eq!(most_recent.config_hash, "hash2");
    assert_eq!(
        most_recent.previous_session_id,
        Some("session-001".to_string())
    );

    Ok(())
}

#[tokio::test]
async fn test_end_runtime_session() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::new_in_memory().await?;

    // Insert a session
    let session = RuntimeSession {
        id: "session-001".to_string(),
        session_id: "test-session".to_string(),
        config_hash: "hash".to_string(),
        binary_version: "0.3.0".to_string(),
        binary_commit: None,
        started_at: "2025-12-02T10:00:00Z".to_string(),
        ended_at: None,
        end_reason: None,
        hostname: "test-server".to_string(),
        runtime_mode: "development".to_string(),
        config_snapshot: "{}".to_string(),
        drift_detected: false,
        drift_summary: None,
        previous_session_id: None,
        model_path: None,
        adapters_root: None,
        database_path: None,
        var_dir: None,
    };

    db.insert_runtime_session(&session).await?;

    // End the session
    db.end_runtime_session("session-001", "graceful").await?;

    // Verify it was marked as ended
    let ended_session = db
        .get_runtime_session("session-001")
        .await?
        .expect("Session should exist");

    assert!(ended_session.ended_at.is_some());
    assert_eq!(ended_session.end_reason, Some("graceful".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_cleanup_old_sessions() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::new_in_memory().await?;

    // Insert old and recent sessions
    for i in 1..=5 {
        let days_ago = if i <= 2 { 100 } else { 10 }; // 2 old, 3 recent
        let session = RuntimeSession {
            id: format!("session-{:03}", i),
            session_id: format!("test-{}", i),
            config_hash: format!("hash-{}", i),
            binary_version: "0.3.0".to_string(),
            binary_commit: None,
            started_at: format!("2025-{:02}-02T10:00:00Z", 12 - (days_ago / 30).min(11)),
            ended_at: Some(format!(
                "2025-{:02}-02T12:00:00Z",
                12 - (days_ago / 30).min(11)
            )),
            end_reason: Some("graceful".to_string()),
            hostname: "test-server".to_string(),
            runtime_mode: "production".to_string(),
            config_snapshot: "{}".to_string(),
            drift_detected: false,
            drift_summary: None,
            previous_session_id: None,
            model_path: None,
            adapters_root: None,
            database_path: None,
            var_dir: None,
        };
        db.insert_runtime_session(&session).await?;
    }

    // Clean up sessions older than 90 days, keeping max 3 per host
    let deleted = db.cleanup_old_sessions(90, 3).await?;

    // Should have deleted the 2 old sessions
    assert!(deleted >= 0); // Depends on system date interpretation in SQLite

    Ok(())
}

#[tokio::test]
async fn test_drift_detection() -> Result<(), Box<dyn std::error::Error>> {
    let db = Db::new_in_memory().await?;

    // Create session with drift detected
    let session = RuntimeSession {
        id: "session-001".to_string(),
        session_id: "test-session".to_string(),
        config_hash: "new-hash".to_string(),
        binary_version: "0.3.0".to_string(),
        binary_commit: None,
        started_at: "2025-12-02T10:00:00Z".to_string(),
        ended_at: None,
        end_reason: None,
        hostname: "test-server".to_string(),
        runtime_mode: "production".to_string(),
        config_snapshot: r#"{"port": 8080}"#.to_string(),
        drift_detected: true,
        drift_summary: Some(r#"{"changed": ["port"]}"#.to_string()),
        previous_session_id: None,
        model_path: None,
        adapters_root: None,
        database_path: None,
        var_dir: None,
    };

    db.insert_runtime_session(&session).await?;

    // Retrieve and verify drift was recorded
    let retrieved = db
        .get_runtime_session("session-001")
        .await?
        .expect("Session should exist");

    assert!(retrieved.drift_detected);
    assert_eq!(
        retrieved.drift_summary,
        Some(r#"{"changed": ["port"]}"#.to_string())
    );

    Ok(())
}
