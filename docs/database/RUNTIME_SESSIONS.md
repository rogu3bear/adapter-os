# Runtime Sessions Database API

**Module:** `adapteros-db::runtime_sessions`
**Migration:** `0125_runtime_sessions.sql`
**Purpose:** Track server runtime sessions for configuration drift detection and lifecycle analysis

---

## Overview

The runtime sessions module provides database methods for tracking AdapterOS server instances from startup to shutdown. Each runtime session captures the server's configuration, binary version, and operational metadata, enabling:

1. **Configuration Drift Detection** - Compare current config with previous sessions
2. **Runtime Continuity** - Link sessions to track server restarts
3. **Audit Trail** - Record all server lifecycle events
4. **Retention Management** - Automatically clean up old session data

---

## Data Model

### RuntimeSession Struct

```rust
pub struct RuntimeSession {
    /// Unique ID for this runtime session record
    pub id: String,

    /// Session ID (generated at startup, used for correlation)
    pub session_id: String,

    /// Hash of the configuration (for drift detection)
    pub config_hash: String,

    /// Binary version (e.g., "0.3.0-alpha")
    pub binary_version: String,

    /// Git commit hash of the binary
    pub binary_commit: Option<String>,

    /// When this session started
    pub started_at: String,

    /// When this session ended (NULL if still running)
    pub ended_at: Option<String>,

    /// Reason for session ending ('graceful', 'crash', 'terminated', or NULL)
    pub end_reason: Option<String>,

    /// Hostname where this session ran
    pub hostname: String,

    /// Runtime mode ('development' or 'production')
    pub runtime_mode: String,

    /// Full configuration snapshot (JSON)
    pub config_snapshot: String,

    /// Whether configuration drift was detected (boolean)
    pub drift_detected: bool,

    /// Summary of detected drift (JSON, NULL if no drift)
    pub drift_summary: Option<String>,

    /// Reference to previous session ID on this host (for continuity tracking)
    pub previous_session_id: Option<String>,

    /// Model path used in this session
    pub model_path: Option<String>,

    /// Adapters root directory
    pub adapters_root: Option<String>,

    /// Database path
    pub database_path: Option<String>,

    /// Var directory path
    pub var_dir: Option<String>,
}
```

---

## Database Methods

### insert_runtime_session

Insert a new runtime session record.

**Signature:**
```rust
pub async fn insert_runtime_session(&self, session: &RuntimeSession) -> Result<()>
```

**Usage:**
```rust
let session = RuntimeSession {
    id: uuid::Uuid::new_v4().to_string(),
    session_id: "session-abc123".to_string(),
    config_hash: compute_config_hash(&config),
    binary_version: env!("CARGO_PKG_VERSION").to_string(),
    binary_commit: option_env!("GIT_COMMIT").map(String::from),
    started_at: chrono::Utc::now().to_rfc3339(),
    ended_at: None,
    end_reason: None,
    hostname: hostname::get()?.to_string_lossy().to_string(),
    runtime_mode: if cfg!(debug_assertions) { "development" } else { "production" }.to_string(),
    config_snapshot: serde_json::to_string(&config)?,
    drift_detected: false,
    drift_summary: None,
    previous_session_id: None,
    model_path: Some(config.model_path.clone()),
    adapters_root: Some(config.adapters_root.clone()),
    database_path: Some(config.database_path.clone()),
    var_dir: Some(config.var_dir.clone()),
};

db.insert_runtime_session(&session).await?;
```

**Returns:** `Result<()>` - Success or database error

---

### get_runtime_session

Retrieve a runtime session by ID.

**Signature:**
```rust
pub async fn get_runtime_session(&self, id: &str) -> Result<Option<RuntimeSession>>
```

**Usage:**
```rust
if let Some(session) = db.get_runtime_session("session-001").await? {
    println!("Session started at: {}", session.started_at);
    println!("Config hash: {}", session.config_hash);
}
```

**Returns:** `Result<Option<RuntimeSession>>` - Session if found, None otherwise

---

### get_most_recent_session

Get the most recent ended session for a hostname.

Used for configuration drift detection by comparing the current configuration with the previous session's configuration.

**Signature:**
```rust
pub async fn get_most_recent_session(&self, hostname: &str) -> Result<Option<RuntimeSession>>
```

**Usage:**
```rust
// At startup, check for configuration drift
let previous = db.get_most_recent_session(&current_hostname).await?;

if let Some(prev) = previous {
    if prev.config_hash != current_config_hash {
        warn!("Configuration drift detected!");
        // Store drift summary in new session
        let drift_summary = compare_configs(&prev.config_snapshot, &current_config);
        new_session.drift_detected = true;
        new_session.drift_summary = Some(serde_json::to_string(&drift_summary)?);
        new_session.previous_session_id = Some(prev.id);
    }
}
```

**Returns:** `Result<Option<RuntimeSession>>` - Most recent session if found

---

### end_runtime_session

Mark a session as ended with a reason.

**Signature:**
```rust
pub async fn end_runtime_session(&self, id: &str, reason: &str) -> Result<()>
```

**Arguments:**
- `id` - The session ID to mark as ended
- `reason` - One of: `"graceful"`, `"crash"`, or `"terminated"`

**Usage:**
```rust
// During graceful shutdown
db.end_runtime_session(&session_id, "graceful").await?;

// In crash recovery handler
db.end_runtime_session(&last_session_id, "crash").await?;

// On SIGTERM
db.end_runtime_session(&session_id, "terminated").await?;
```

**Returns:** `Result<()>` - Success or database error

---

### cleanup_old_sessions

Remove old runtime sessions based on retention policy.

Keeps:
1. Sessions within the retention period (retention_days)
2. The N most recent sessions per hostname (max_per_host)

**Signature:**
```rust
pub async fn cleanup_old_sessions(&self, retention_days: i64, max_per_host: i64) -> Result<i64>
```

**Arguments:**
- `retention_days` - Number of days to retain sessions (e.g., 90)
- `max_per_host` - Maximum sessions to keep per hostname (e.g., 100)

**Usage:**
```rust
// Daily cleanup task
let deleted = db.cleanup_old_sessions(90, 100).await?;
info!("Cleaned up {} old runtime sessions", deleted);
```

**Returns:** `Result<i64>` - Number of sessions deleted

---

## Database Schema

### Table: runtime_sessions

```sql
CREATE TABLE runtime_sessions (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    config_hash TEXT NOT NULL,
    binary_version TEXT NOT NULL,
    binary_commit TEXT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    end_reason TEXT CHECK (end_reason IS NULL OR end_reason IN ('graceful', 'crash', 'terminated')),
    hostname TEXT NOT NULL,
    runtime_mode TEXT NOT NULL CHECK (runtime_mode IN ('development', 'production')),
    config_snapshot TEXT NOT NULL,
    drift_detected INTEGER DEFAULT 0 CHECK (drift_detected IN (0, 1)),
    drift_summary TEXT,
    previous_session_id TEXT,
    model_path TEXT,
    adapters_root TEXT,
    database_path TEXT,
    var_dir TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (previous_session_id) REFERENCES runtime_sessions(id) ON DELETE SET NULL
);
```

### Indexes

- `idx_runtime_sessions_started_at` - Efficient ordering by start time
- `idx_runtime_sessions_hostname` - Fast hostname lookups
- `idx_runtime_sessions_config_hash` - Configuration drift queries
- `idx_runtime_sessions_ended_at` - Active session filtering

### Views

**active_sessions** - Currently running sessions:
```sql
SELECT id, session_id, config_hash, binary_version, started_at, hostname,
       julianday('now') - julianday(started_at) AS uptime_days
FROM runtime_sessions
WHERE ended_at IS NULL
ORDER BY started_at DESC;
```

**config_drift_history** - Sessions with detected configuration drift:
```sql
SELECT rs.id, rs.session_id, rs.config_hash, rs.drift_summary,
       prev.config_hash AS previous_config_hash
FROM runtime_sessions rs
LEFT JOIN runtime_sessions prev ON rs.previous_session_id = prev.id
WHERE rs.drift_detected = 1
ORDER BY rs.started_at DESC;
```

---

## Integration Example

### Server Startup

```rust
use adapteros_db::{Db, RuntimeSession};
use chrono::Utc;
use uuid::Uuid;

async fn initialize_runtime_session(
    db: &Db,
    config: &ServerConfig,
) -> Result<RuntimeSession> {
    let hostname = hostname::get()?.to_string_lossy().to_string();

    // Get previous session for drift detection
    let previous = db.get_most_recent_session(&hostname).await?;

    // Compute current config hash
    let config_json = serde_json::to_string(config)?;
    let config_hash = compute_blake3_hash(&config_json);

    // Detect drift
    let (drift_detected, drift_summary, previous_session_id) = if let Some(prev) = previous {
        if prev.config_hash != config_hash {
            let drift = compare_configs(&prev.config_snapshot, &config_json);
            (true, Some(serde_json::to_string(&drift)?), Some(prev.id))
        } else {
            (false, None, Some(prev.id))
        }
    } else {
        (false, None, None)
    };

    // Create new session
    let session = RuntimeSession {
        id: Uuid::new_v4().to_string(),
        session_id: format!("session-{}", Uuid::new_v4()),
        config_hash,
        binary_version: env!("CARGO_PKG_VERSION").to_string(),
        binary_commit: option_env!("GIT_COMMIT").map(String::from),
        started_at: Utc::now().to_rfc3339(),
        ended_at: None,
        end_reason: None,
        hostname,
        runtime_mode: if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        }.to_string(),
        config_snapshot: config_json,
        drift_detected,
        drift_summary,
        previous_session_id,
        model_path: Some(config.model_path.clone()),
        adapters_root: Some(config.adapters_root.clone()),
        database_path: Some(config.database_path.clone()),
        var_dir: Some(config.var_dir.clone()),
    };

    db.insert_runtime_session(&session).await?;

    if drift_detected {
        warn!("Configuration drift detected on startup!");
    }

    Ok(session)
}
```

### Server Shutdown

```rust
async fn shutdown_handler(db: &Db, session_id: String) -> Result<()> {
    db.end_runtime_session(&session_id, "graceful").await?;
    info!("Runtime session ended gracefully");
    Ok(())
}
```

### Background Cleanup Task

```rust
use tokio::time::{interval, Duration};

async fn runtime_session_cleanup_task(db: Db) {
    let mut ticker = interval(Duration::from_secs(86400)); // Daily

    loop {
        ticker.tick().await;

        match db.cleanup_old_sessions(90, 100).await {
            Ok(deleted) => {
                if deleted > 0 {
                    info!("Cleaned up {} old runtime sessions", deleted);
                }
            }
            Err(e) => {
                error!("Failed to cleanup runtime sessions: {}", e);
            }
        }
    }
}
```

---

## Configuration Drift Detection

### Computing Config Hash

```rust
use blake3::Hasher;

fn compute_config_hash(config: &ServerConfig) -> String {
    let config_json = serde_json::to_string(config).unwrap();
    let mut hasher = Hasher::new();
    hasher.update(config_json.as_bytes());
    hasher.finalize().to_hex().to_string()
}
```

### Comparing Configurations

```rust
use serde_json::Value;

fn compare_configs(old_json: &str, new_json: &str) -> Value {
    let old: Value = serde_json::from_str(old_json).unwrap();
    let new: Value = serde_json::from_str(new_json).unwrap();

    let mut changes = vec![];

    if let (Value::Object(old_map), Value::Object(new_map)) = (old, new) {
        for (key, old_val) in old_map.iter() {
            if let Some(new_val) = new_map.get(key) {
                if old_val != new_val {
                    changes.push(serde_json::json!({
                        "field": key,
                        "old": old_val,
                        "new": new_val
                    }));
                }
            } else {
                changes.push(serde_json::json!({
                    "field": key,
                    "change": "removed"
                }));
            }
        }

        for (key, new_val) in new_map.iter() {
            if !old_map.contains_key(key) {
                changes.push(serde_json::json!({
                    "field": key,
                    "change": "added",
                    "value": new_val
                }));
            }
        }
    }

    serde_json::json!({
        "changes": changes,
        "total_changed": changes.len()
    })
}
```

---

## Testing

See `crates/adapteros-db/tests/runtime_sessions_test.rs` for comprehensive test examples.

**Test coverage:**
- Insert and retrieve sessions
- Get most recent session
- End session with reason
- Cleanup old sessions
- Configuration drift detection
- Session continuity (previous_session_id)

Run tests:
```bash
cargo test -p adapteros-db runtime_sessions
```

---

## Error Handling

All methods use `adapteros_core::Result<T>` and `AosError` for error handling.

Common errors:
- **Database errors** - Connection failures, constraint violations
- **Serialization errors** - Invalid JSON in config_snapshot or drift_summary
- **Not found** - Session ID doesn't exist (returns `Ok(None)`)

Example error handling:
```rust
match db.get_runtime_session(&id).await {
    Ok(Some(session)) => {
        // Session found
        println!("Found session: {}", session.session_id);
    }
    Ok(None) => {
        // Session not found
        warn!("Session {} not found", id);
    }
    Err(e) => {
        // Database error
        error!("Failed to retrieve session: {}", e);
        return Err(e);
    }
}
```

---

## Best Practices

1. **Always create a session at startup** - Track every server instance
2. **End sessions on shutdown** - Use appropriate reason codes
3. **Detect drift early** - Compare config hashes before starting
4. **Clean up regularly** - Schedule daily cleanup task
5. **Log drift events** - Alert operators when drift is detected
6. **Store full snapshots** - Include complete config for debugging
7. **Link sessions** - Use previous_session_id for continuity

---

## Related Documentation

- [Database Reference](../DATABASE_REFERENCE.md) - Complete schema documentation
- [Migration Guide](SQLX_OFFLINE_MODE.md) - Database migration procedures
- [Boot System](../../docs/BOOT_SYSTEM.md) - Server startup sequence

---

**Copyright:** 2025 JKCA / James KC Auchterlonie
**Version:** v0.3-alpha
**Last Updated:** 2025-12-02
