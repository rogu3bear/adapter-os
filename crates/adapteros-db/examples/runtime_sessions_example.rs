//! Example demonstrating runtime sessions API usage
//!
//! Run with: cargo run --example runtime_sessions_example -p adapteros-db

use adapteros_db::{Db, RuntimeSession};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Runtime Sessions API Example\n");

    // Create in-memory database with migrations
    let db = Db::new_in_memory().await?;
    println!("✓ Database initialized with migrations");

    // Example 1: Create and insert a runtime session
    println!("\n1. Creating a new runtime session...");
    let session = RuntimeSession {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: "example-session-001".to_string(),
        config_hash: "abc123def456".to_string(),
        binary_version: "0.3.0-alpha".to_string(),
        binary_commit: Some("commit-hash-789".to_string()),
        started_at: chrono::Utc::now().to_rfc3339(),
        ended_at: None,
        end_reason: None,
        hostname: "example-server-01".to_string(),
        runtime_mode: "production".to_string(),
        config_snapshot: serde_json::json!({
            "server_port": 8080,
            "model_path": "/models/qwen2.5-7b",
            "adapters_root": "/var/adapters"
        })
        .to_string(),
        drift_detected: false,
        drift_summary: None,
        previous_session_id: None,
        model_path: Some("/models/qwen2.5-7b".to_string()),
        adapters_root: Some("/var/adapters".to_string()),
        database_path: Some("/var/db.sqlite".to_string()),
        var_dir: Some("/var".to_string()),
    };

    db.insert_runtime_session(&session).await?;
    println!("✓ Session created: {}", session.session_id);

    // Example 2: Retrieve the session
    println!("\n2. Retrieving session by ID...");
    let retrieved = db
        .get_runtime_session(&session.id)
        .await?
        .expect("Session should exist");
    println!("✓ Retrieved session: {}", retrieved.session_id);
    println!("  - Binary version: {}", retrieved.binary_version);
    println!("  - Hostname: {}", retrieved.hostname);
    println!("  - Runtime mode: {}", retrieved.runtime_mode);

    // Example 3: End the session gracefully
    println!("\n3. Ending session gracefully...");
    db.end_runtime_session(&session.id, "graceful").await?;
    println!("✓ Session ended");

    // Example 4: Create a new session with drift detection
    println!("\n4. Creating a new session with configuration drift...");
    let previous_session = db.get_most_recent_session("example-server-01").await?;

    let new_session = RuntimeSession {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: "example-session-002".to_string(),
        config_hash: "new-hash-xyz789".to_string(),
        binary_version: "0.3.0-alpha".to_string(),
        binary_commit: Some("commit-hash-789".to_string()),
        started_at: chrono::Utc::now().to_rfc3339(),
        ended_at: None,
        end_reason: None,
        hostname: "example-server-01".to_string(),
        runtime_mode: "production".to_string(),
        config_snapshot: serde_json::json!({
            "server_port": 9090,  // Changed!
            "model_path": "/models/qwen2.5-7b",
            "adapters_root": "/var/adapters"
        })
        .to_string(),
        drift_detected: true,
        drift_summary: Some(
            serde_json::json!({
                "changed_fields": ["server_port"],
                "old_value": 8080,
                "new_value": 9090
            })
            .to_string(),
        ),
        previous_session_id: previous_session.map(|s| s.id),
        model_path: Some("/models/qwen2.5-7b".to_string()),
        adapters_root: Some("/var/adapters".to_string()),
        database_path: Some("/var/db.sqlite".to_string()),
        var_dir: Some("/var".to_string()),
    };

    db.insert_runtime_session(&new_session).await?;
    println!("✓ New session created with drift detection");
    if let Some(drift) = &new_session.drift_summary {
        println!("  - Drift detected: {}", drift);
    }

    // Example 5: Get most recent session
    println!("\n5. Getting most recent session for hostname...");
    let most_recent = db.get_most_recent_session("example-server-01").await?;

    if let Some(session) = most_recent {
        println!("✓ Most recent session: {}", session.session_id);
        println!("  - Config hash: {}", session.config_hash);
        if session.drift_detected {
            println!("  - ⚠ Configuration drift detected");
        }
    }

    // Example 6: Cleanup old sessions
    println!("\n6. Cleaning up old sessions...");
    let deleted = db.cleanup_old_sessions(90, 100).await?;
    println!("✓ Cleaned up {} old sessions", deleted);

    println!("\n✓ All examples completed successfully!");

    Ok(())
}
