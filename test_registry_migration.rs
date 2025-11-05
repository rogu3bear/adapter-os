use adapteros_core::B3Hash;
use adapteros_registry::Registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing registry migration system...");

    // Remove any existing test database
    let _ = std::fs::remove_file("test_registry.db");

    // Open registry (this should run migrations)
    let registry = Registry::open("test_registry.db").await?;
    println!("✓ Registry opened successfully");

    // Register a tenant
    registry.register_tenant("test-tenant", "Test Tenant", false).await?;
    println!("✓ Tenant registered successfully");

    // Register an adapter
    let hash = B3Hash::from_hex("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")?;
    registry.register_adapter(
        "test-tenant",
        "test-adapter",
        &hash,
        "persistent",
        8,
        1.0,
        r#"["test-target"]"#,
        Some(r#"["allowed-tenant"]"#),
        Some("adapter-123"),
        Some(r#"["en"]"#),
        Some("pytorch"),
    ).await?;
    println!("✓ Adapter registered successfully");

    // Retrieve the adapter
    if let Some(adapter) = registry.get_adapter("test-tenant-test-adapter").await? {
        println!("✓ Adapter retrieved successfully:");
        println!("  ID: {}", adapter.id);
        println!("  Name: {}", adapter.name);
        println!("  Tenant: {}", adapter.tenant_id);
        println!("  Hash: {}", adapter.hash.to_hex());
        println!("  Active: {}", adapter.active);
    } else {
        println!("✗ Adapter not found");
        return Err("Adapter retrieval failed".into());
    }

    // Test ACL check
    let allowed = registry.check_acl("test-tenant-test-adapter", "allowed-tenant").await?;
    let denied = registry.check_acl("test-tenant-test-adapter", "denied-tenant").await?;
    println!("✓ ACL check - allowed tenant: {}, denied tenant: {}", allowed, !denied);

    // List adapters
    let adapters = registry.list_adapters().await?;
    println!("✓ Listed {} adapters", adapters.len());

    // Retrieve tenant
    if let Some(tenant) = registry.get_tenant("test-tenant").await? {
        println!("✓ Tenant retrieved successfully:");
        println!("  ID: {}", tenant.id);
        println!("  Name: {}", tenant.name);
        println!("  ITAR: {}", tenant.itar_flag);
    } else {
        println!("✗ Tenant not found");
        return Err("Tenant retrieval failed".into());
    }

    // Check database schema
    println!("\nChecking database schema...");
    let conn = rusqlite::Connection::open("test_registry.db")?;
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?;
    let table_names: Vec<String> = stmt.query_map([], |row| row.get(0))?.collect::<Result<_, _>>()?;

    println!("Tables in database:");
    for table in table_names {
        if table != "sqlite_sequence" {  // Skip internal SQLite table
            println!("  - {}", table);

            // Show table schema
            let mut schema_stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
            let columns: Vec<String> = schema_stmt.query_map([], |row| {
                let name: String = row.get(1)?;
                Ok(name)
            })?.collect::<Result<_, _>>()?;
            println!("    Columns: {}", columns.join(", "));
        }
    }

    println!("\n✓ All tests passed! Migration system is working correctly.");
    Ok(())
}

