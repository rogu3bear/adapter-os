//! Database migrations

use adapteros_core::{AosError, Result};
use rusqlite::Connection;
use std::collections::HashSet;

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    // Create adapters table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS adapters (
            id TEXT PRIMARY KEY,
            hash TEXT NOT NULL,
            tier TEXT NOT NULL,
            rank INTEGER NOT NULL,
            acl TEXT NOT NULL,
            activation_pct REAL DEFAULT 0.0,
            registered_at TEXT NOT NULL,
            adapter_name TEXT,
            tenant_namespace TEXT,
            domain TEXT,
            purpose TEXT,
            revision INTEGER,
            parent_id TEXT,
            fork_type TEXT,
            fork_reason TEXT
        )",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create adapters table: {}", e)))?;

    ensure_adapter_columns(conn)?;

    // Create tenants table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY,
            uid INTEGER NOT NULL,
            gid INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create tenants table: {}", e)))?;

    // Create checkpoints table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS checkpoints (
            cpid TEXT PRIMARY KEY,
            plan_id TEXT NOT NULL,
            manifest_hash TEXT NOT NULL,
            promoted_at TEXT NOT NULL,
            status TEXT NOT NULL
        )",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create checkpoints table: {}", e)))?;

    // Create models table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS models (
            name TEXT PRIMARY KEY,
            config_hash TEXT NOT NULL,
            tokenizer_hash TEXT NOT NULL,
            tokenizer_cfg_hash TEXT NOT NULL,
            weights_hash TEXT NOT NULL,
            license_hash TEXT NOT NULL,
            license_text TEXT NOT NULL,
            model_card_hash TEXT,
            created_at INTEGER NOT NULL
        )",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create models table: {}", e)))?;

    // Create indices
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_adapters_tier ON adapters(tier)",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create adapters tier index: {}", e)))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_adapters_activation ON adapters(activation_pct)",
        [],
    )
    .map_err(|e| {
        AosError::Registry(format!("Failed to create adapters activation index: {}", e))
    })?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_config_hash ON models(config_hash)",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create models config hash index: {}", e)))?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_tokenizer_hash ON models(tokenizer_hash)",
        [],
    )
    .map_err(|e| {
        AosError::Registry(format!(
            "Failed to create models tokenizer hash index: {}",
            e
        ))
    })?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_weights_hash ON models(weights_hash)",
        [],
    )
    .map_err(|e| {
        AosError::Registry(format!("Failed to create models weights hash index: {}", e))
    })?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_created_at ON models(created_at)",
        [],
    )
    .map_err(|e| AosError::Registry(format!("Failed to create models created_at index: {}", e)))?;

    Ok(())
}

fn ensure_adapter_columns(conn: &Connection) -> Result<()> {
    let mut columns = existing_columns(conn, "adapters")?;
    for (name, sql_type) in [
        ("adapter_name", "TEXT"),
        ("tenant_namespace", "TEXT"),
        ("domain", "TEXT"),
        ("purpose", "TEXT"),
        ("revision", "INTEGER"),
        ("parent_id", "TEXT"),
        ("fork_type", "TEXT"),
        ("fork_reason", "TEXT"),
    ] {
        if !columns.contains(name) {
            conn.execute(
                &format!("ALTER TABLE adapters ADD COLUMN {} {}", name, sql_type),
                [],
            )
            .map_err(|e| AosError::Registry(format!("Failed to add adapters.{}: {}", name, e)))?;
            columns.insert(name.to_string());
        }
    }

    Ok(())
}

fn existing_columns(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| AosError::Registry(format!("Failed to read {} schema: {}", table, e)))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| AosError::Registry(format!("Failed to read {} schema: {}", table, e)))?;

    let mut columns = HashSet::new();
    for row in rows {
        columns.insert(
            row.map_err(|e| AosError::Registry(format!("Failed to read {} schema: {}", table, e)))?,
        );
    }

    Ok(columns)
}
