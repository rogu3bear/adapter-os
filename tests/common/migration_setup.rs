//! Shared test data creation utilities for migration tests
//! 【2025-11-07†refactor(tests)†consolidate-test-setup】
//!
//! Consolidates duplicate test data creation code from:
//! - test_safe_migration.rs
//! - test_registry_migration_complete.rs

use rusqlite::Connection;
use std::path::Path;

/// Create comprehensive test data for migration tests
/// 【2025-11-07†refactor(tests)†consolidate-test-setup】
///
/// Creates old schema tables (adapters, tenants, models) with sample data.
/// This function replaces duplicate implementations in test files.
pub fn create_comprehensive_test_data(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating comprehensive test data...");

    let conn = Connection::open(db_path)?;

    // Create old schema tables
    conn.execute(
        "CREATE TABLE adapters (
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

    conn.execute(
        "CREATE TABLE tenants (
            id TEXT PRIMARY KEY,
            uid INTEGER NOT NULL,
            gid INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE models (
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

    // Insert diverse test tenants
    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["default", "1000", "1000", "2024-01-01T00:00:00Z"],
    )?;

    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["acme-corp", "1001", "1001", "2024-01-02T00:00:00Z"],
    )?;

    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["research-lab", "1002", "1002", "2024-01-03T00:00:00Z"],
    )?;

    // Insert diverse test adapters with various patterns
    let test_adapters = vec![
        (
            "default-test-adapter",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "persistent",
            "8",
            "acme-corp",
            "0.5",
            "2024-01-01T00:00:00Z",
        ),
        (
            "acme-corp-classifier",
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            "warm",
            "16",
            "",
            "0.8",
            "2024-01-02T00:00:00Z",
        ),
        (
            "research-lab-encoder",
            "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
            "ephemeral",
            "4",
            "research-lab,acme-corp",
            "0.3",
            "2024-01-03T00:00:00Z",
        ),
        (
            "default-qa-model",
            "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321",
            "persistent",
            "12",
            "research-lab",
            "0.9",
            "2024-01-04T00:00:00Z",
        ),
    ];

    for (id, hash, tier, rank, acl, activation_pct, registered_at) in &test_adapters {
        conn.execute(
            "INSERT INTO adapters (id, hash, tier, rank, acl, activation_pct, registered_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
            [id, hash, tier, rank, acl, activation_pct, registered_at],
        )?;
    }

    // Insert test models
    conn.execute(
        "INSERT INTO models (name, config_hash, tokenizer_hash, tokenizer_cfg_hash, weights_hash, license_hash, license_text, model_card_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ["test-model-1", "config123", "token123", "tokencfg123", "weights123", "license123", "MIT License", "card123", "1704067200"],
    )?;

    conn.execute(
        "INSERT INTO models (name, config_hash, tokenizer_hash, tokenizer_cfg_hash, weights_hash, license_hash, license_text, model_card_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8)",
        rusqlite::params![
            "test-model-2",
            "config456",
            "token456",
            "tokencfg456",
            "weights456",
            "license456",
            "Apache 2.0",
            "1704067300"
        ],
    )?;

    println!(
        "✓ Created {} tenants, {} adapters, {} models",
        3,
        test_adapters.len(),
        2
    );

    Ok(())
}

/// Create minimal test data for migration tests (simpler variant)
/// 【2025-11-07†refactor(tests)†consolidate-test-setup】
///
/// Creates old schema with minimal sample data.
/// Used by tests that need a simpler dataset.
pub fn create_minimal_test_data(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating minimal test data...");

    let conn = Connection::open(db_path)?;

    // Create old schema tables
    conn.execute(
        "CREATE TABLE adapters (
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

    conn.execute(
        "CREATE TABLE tenants (
            id TEXT PRIMARY KEY,
            uid INTEGER NOT NULL,
            gid INTEGER NOT NULL,
            created_at TEXT NOT NULL
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE models (
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

    // Insert minimal tenant data
    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["default", "1000", "1000", "2024-01-01T00:00:00Z"],
    )?;

    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["tenant-a", "1001", "1001", "2024-01-02T00:00:00Z"],
    )?;

    // Insert minimal adapter data
    let hash1 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let hash2 = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

    conn.execute(
        "INSERT INTO adapters (id, hash, tier, rank, acl, activation_pct, registered_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        ["default-test-adapter", hash1, "persistent", "8", "tenant-a", "0.5", "2024-01-01T00:00:00Z"],
    )?;

    conn.execute(
        "INSERT INTO adapters (id, hash, tier, rank, acl, activation_pct, registered_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        ["tenant-a-special-adapter", hash2, "warm", "16", "", "0.8", "2024-01-02T00:00:00Z"],
    )?;

    // Insert minimal model data
    conn.execute(
        "INSERT INTO models (name, config_hash, tokenizer_hash, tokenizer_cfg_hash, weights_hash, license_hash, license_text, model_card_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ["test-model", "config123", "token123", "tokencfg123", "weights123", "license123", "MIT License", "card123", "1704067200"],
    )?;

    println!("✓ Minimal test data created");
    Ok(())
}
