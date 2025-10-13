//! Database migrations

use rusqlite::Connection;

pub fn run_migrations(conn: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    // Create adapters table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS adapters (
            id TEXT PRIMARY KEY,
            hash TEXT NOT NULL,
            tier TEXT NOT NULL,
            rank INTEGER NOT NULL,
            acl TEXT NOT NULL,
            activation_pct REAL DEFAULT 0.0,
            registered_at TEXT NOT NULL
        )",
        [],
    )?;

    // Create tenants table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tenants (
            id TEXT PRIMARY KEY,
            uid INTEGER NOT NULL,
            gid INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        [],
    )?;

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
    )?;

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
    )?;

    // Create indices
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_adapters_tier ON adapters(tier)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_adapters_activation ON adapters(activation_pct)",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_config_hash ON models(config_hash)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_tokenizer_hash ON models(tokenizer_hash)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_weights_hash ON models(weights_hash)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_models_created_at ON models(created_at)",
        [],
    )?;

    Ok(())
}
