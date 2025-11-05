//! Test Registry Migration with Sample Data
//!
//! Creates sample old registry data and tests the migration process.

use adapteros_core::B3Hash;
use adapteros_registry::Registry;
use rusqlite::Connection;
use std::path::Path;
use tempfile::tempdir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Registry Migration Process");
    println!("==================================");

    let temp_dir = tempdir()?;
    let old_db_path = temp_dir.path().join("old_registry.db");
    let new_db_path = temp_dir.path().join("new_registry.db");

    // Create old registry database with sample data
    create_old_registry_sample(&old_db_path)?;

    // Verify old data
    verify_old_data(&old_db_path)?;

    // Run migration (simulate the migration script)
    migrate_registry_data(&old_db_path, &new_db_path).await?;

    // Verify new data
    verify_new_data(&new_db_path).await?;

    println!("✓ All migration tests passed!");

    Ok(())
}

fn create_old_registry_sample(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Creating sample old registry database...");

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

    // Insert sample tenant data
    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["default", "1000", "1000", "2024-01-01T00:00:00Z"],
    )?;

    conn.execute(
        "INSERT INTO tenants (id, uid, gid, created_at) VALUES (?, ?, ?, ?)",
        ["tenant-a", "1001", "1001", "2024-01-02T00:00:00Z"],
    )?;

    // Insert sample adapter data
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

    // Insert sample model data
    conn.execute(
        "INSERT INTO models (name, config_hash, tokenizer_hash, tokenizer_cfg_hash, weights_hash, license_hash, license_text, model_card_hash, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        ["test-model", "config123", "token123", "tokencfg123", "weights123", "license123", "MIT License", "card123", "1704067200"],
    )?;

    println!("✓ Sample old registry database created");
    Ok(())
}

fn verify_old_data(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Verifying old registry data...");

    let conn = Connection::open(db_path)?;

    // Check tenants
    let tenant_count: i64 = conn.query_row("SELECT COUNT(*) FROM tenants", [], |row| row.get(0))?;
    assert_eq!(tenant_count, 2, "Expected 2 tenants");

    // Check adapters
    let adapter_count: i64 = conn.query_row("SELECT COUNT(*) FROM adapters", [], |row| row.get(0))?;
    assert_eq!(adapter_count, 2, "Expected 2 adapters");

    // Check models
    let model_count: i64 = conn.query_row("SELECT COUNT(*) FROM models", [], |row| row.get(0))?;
    assert_eq!(model_count, 1, "Expected 1 model");

    println!("✓ Old data verified: {} tenants, {} adapters, {} models",
             tenant_count, adapter_count, model_count);

    Ok(())
}

async fn migrate_registry_data(
    old_db_path: &Path,
    new_db_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Migrating registry data...");

    // Create new registry
    let registry = Registry::open(new_db_path).await?;
    println!("✓ New registry created");

    // Extract and migrate tenants
    let old_conn = Connection::open(old_db_path)?;

    // Migrate tenants
    {
        let mut stmt = old_conn.prepare("SELECT id, uid, gid, created_at FROM tenants")?;
        let tenant_iter = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        for tenant_result in tenant_iter {
            let (id, _uid, _gid, _created_at) = tenant_result?;
            // Use id as both id and name, assume no ITAR
            registry.register_tenant(&id, &id, false).await?;
            println!("✓ Migrated tenant: {}", id);
        }
    }

    // Migrate adapters
    {
        let mut stmt = old_conn.prepare("SELECT id, hash, tier, rank, acl, activation_pct, registered_at FROM adapters")?;
        let adapter_iter = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        for adapter_result in adapter_iter {
            let (id, hash_hex, tier, rank, acl, _activation_pct, _registered_at) = adapter_result?;

            // Parse hash
            let hash = B3Hash::from_hex(&hash_hex)?;

            // Extract tenant and name from id
            let parts: Vec<&str> = id.split('-').collect();
            let (tenant_id, name) = if parts.len() >= 2 {
                (parts[0].to_string(), parts[1..].join("-"))
            } else {
                ("default".to_string(), id.clone())
            };

            // Transform ACL
            let acl_json = if acl.trim().is_empty() {
                None
            } else {
                Some(format!(r#"["{}"]"#, acl))
            };

            registry.register_adapter(
                &tenant_id,
                &name,
                &hash,
                &tier,
                rank as u32,
                1.0, // default alpha
                r#"["unknown"]"#,
                acl_json.as_deref(),
                Some(&id),
                Some(r#"["en"]"#),
                Some("unknown"),
            ).await?;

            println!("✓ Migrated adapter: {}-{}", tenant_id, name);
        }
    }

    println!("✓ Registry data migration completed");
    Ok(())
}

async fn verify_new_data(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    println!("Verifying new registry data...");

    let registry = Registry::open(db_path).await?;

    // Check tenants
    let tenant_count = if registry.get_tenant("default").await?.is_some() { 1 } else { 0 }
                     + if registry.get_tenant("tenant-a").await?.is_some() { 1 } else { 0 };
    assert_eq!(tenant_count, 2, "Expected 2 tenants in new registry");

    // Check adapters
    let adapters = registry.list_adapters().await?;
    assert_eq!(adapters.len(), 2, "Expected 2 adapters in new registry");

    // Check specific adapters
    let adapter1 = registry.get_adapter("default-test-adapter").await?;
    assert!(adapter1.is_some(), "Expected default-test-adapter to exist");

    let adapter2 = registry.get_adapter("tenant-a-special-adapter").await?;
    assert!(adapter2.is_some(), "Expected tenant-a-special-adapter to exist");

    // Check ACL
    let acl_check = registry.check_acl("default-test-adapter", "tenant-a").await?;
    assert!(acl_check, "Expected tenant-a to have access to default-test-adapter");

    println!("✓ New data verified: {} tenants, {} adapters",
             tenant_count, adapters.len());

    Ok(())
}
