//! Test Safe Registry Migration
//!
//! Demonstrates the complete safe migration process with comprehensive validation.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::Registry;
use registry_migration_analysis::SchemaAnalysis;
use registry_migration_safe::{MigrationConfig, MigrationEngine, TenantExtractionStrategy, MigrationDefaults, ValidationRules};
use rusqlite::Connection;
use std::path::Path;
use tempfile::tempdir;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("Testing Safe Registry Migration");
    println!("===============================");

    let temp_dir = tempdir()?;
    let old_db_path = temp_dir.path().join("old_registry.db");
    let new_db_path = temp_dir.path().join("new_registry.db");

    // Create comprehensive test data
    create_comprehensive_test_data(&old_db_path)?;

    // Test analysis
    test_analysis(&old_db_path).await?;

    // Test safe migration
    test_safe_migration(&old_db_path, &new_db_path).await?;

    // Test error handling
    test_error_handling().await?;

    println!("✓ All safe migration tests passed!");

    Ok(())
}

fn create_comprehensive_test_data(db_path: &Path) -> Result<()> {
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
        ("default-test-adapter", "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "persistent", "8", "acme-corp", "0.5", "2024-01-01T00:00:00Z"),
        ("acme-corp-classifier", "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789", "warm", "16", "", "0.8", "2024-01-02T00:00:00Z"),
        ("research-lab-encoder", "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0", "ephemeral", "4", "research-lab,acme-corp", "0.3", "2024-01-03T00:00:00Z"),
        ("default-qa-model", "fedcba0987654321fedcba0987654321fedcba0987654321fedcba0987654321", "persistent", "12", "research-lab", "0.9", "2024-01-04T00:00:00Z"),
    ];

    for (id, hash, tier, rank, acl, activation_pct, registered_at) in test_adapters {
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
        "INSERT INTO models (name, config_hash, tokenizer_hash, tokenizer_cfg_hash, weights_hash, license_hash, license_text, model_card_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ["test-model-2", "config456", "token456", "tokencfg456", "weights456", "license456", "Apache 2.0", None::<String>, "1704067300"],
    )?;

    println!("✓ Created {} tenants, {} adapters, {} models",
             3, test_adapters.len(), 2);

    Ok(())
}

async fn test_analysis(db_path: &Path) -> Result<()> {
    println!("Testing schema analysis...");

    let analysis = SchemaAnalysis::analyze(db_path)?;

    // Verify analysis captured our test data
    assert_eq!(analysis.tables.len(), 3, "Should find 3 tables");

    let adapters_table = analysis.tables.iter().find(|t| t.name == "adapters").unwrap();
    assert_eq!(adapters_table.row_count, 4, "Should find 4 adapters");

    let tenants_table = analysis.tables.iter().find(|t| t.name == "tenants").unwrap();
    assert_eq!(tenants_table.row_count, 3, "Should find 3 tenants");

    // Check data patterns
    assert!(analysis.data_patterns.adapter_id_patterns.contains(&"default-*".to_string()));
    assert!(analysis.data_patterns.adapter_id_patterns.contains(&"acme-corp-*".to_string()));
    assert!(analysis.data_patterns.hash_formats.contains(&"hex-64".to_string()));

    println!("✓ Analysis correctly identified schema and patterns");
    Ok(())
}

async fn test_safe_migration(old_db_path: &Path, new_db_path: &Path) -> Result<()> {
    println!("Testing safe migration...");

    // Create migration config
    let config = MigrationConfig {
        tenant_extraction: TenantExtractionStrategy::SplitOnDash,
        defaults: MigrationDefaults {
            alpha: 1.0,
            targets_json: r#"["auto-migrated"]"#.to_string(),
            languages_json: r#"["en"]"#.to_string(),
            framework: "auto-migrated".to_string(),
            active: true,
        },
        validation: ValidationRules {
            validate_tenant_refs: true,
            validate_hash_formats: true,
            validate_acl_transforms: true,
        },
    };

    let mut engine = MigrationEngine::new(config);

    // Simulate Args for testing
    struct TestArgs {
        old_db: std::path::PathBuf,
        new_db: std::path::PathBuf,
        dry_run: bool,
        force: bool,
        backup: bool,
        max_errors: usize,
        config: Option<std::path::PathBuf>,
    }

    let args = TestArgs {
        old_db: old_db_path.to_path_buf(),
        new_db: new_db_path.to_path_buf(),
        dry_run: false,
        force: true, // Force migration for testing
        backup: true,
        max_errors: 10,
        config: None,
    };

    // Run migration
    engine.execute(&args).await?;

    // Verify results
    let registry = Registry::open(new_db_path).await?;
    let adapters = registry.list_adapters().await?;
    assert_eq!(adapters.len(), 4, "Should have migrated 4 adapters");

    // Check specific adapter transformations
    let test_adapter = adapters.iter().find(|a| a.id == "default-test-adapter").unwrap();
    assert_eq!(test_adapter.tenant_id, "default");
    assert_eq!(test_adapter.name, "test-adapter");
    assert!(test_adapter.acl_json.is_some(), "Should have ACL");

    let encoder_adapter = adapters.iter().find(|a| a.id == "research-lab-encoder").unwrap();
    assert_eq!(encoder_adapter.tenant_id, "research-lab");
    assert_eq!(encoder_adapter.name, "encoder");

    println!("✓ Safe migration completed successfully");
    Ok(())
}

async fn test_error_handling() -> Result<()> {
    println!("Testing error handling...");

    let temp_dir = tempdir()?;
    let old_db_path = temp_dir.path().join("error_test.db");
    let new_db_path = temp_dir.path().join("error_test_new.db");

    // Create database with invalid data
    let conn = Connection::open(&old_db_path)?;
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

    // Insert adapter with invalid hash (not hex)
    conn.execute(
        "INSERT INTO adapters (id, hash, tier, rank, acl, activation_pct, registered_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        ["test-invalid-hash", "not-a-hex-hash", "persistent", "8", "", "0.0", "2024-01-01T00:00:00Z"],
    )?;

    // Test migration with invalid data
    let config = MigrationConfig::default();
    let mut engine = MigrationEngine::new(config);

    struct TestArgs {
        old_db: std::path::PathBuf,
        new_db: std::path::PathBuf,
        dry_run: bool,
        force: bool,
        backup: bool,
        max_errors: usize,
        config: Option<std::path::PathBuf>,
    }

    let args = TestArgs {
        old_db: old_db_path,
        new_db: new_db_path,
        dry_run: false,
        force: true,
        backup: false, // Skip backup for test
        max_errors: 5,
        config: None,
    };

    // Migration should handle errors gracefully
    let result = engine.execute(&args).await;

    // Should fail due to invalid hash
    assert!(result.is_err(), "Migration should fail with invalid data");

    let stats = engine.get_stats().await;
    assert_eq!(stats.adapters_failed, 1, "Should record adapter failure");
    assert!(!stats.validation_errors.is_empty(), "Should have validation errors");

    println!("✓ Error handling works correctly");
    Ok(())
}
