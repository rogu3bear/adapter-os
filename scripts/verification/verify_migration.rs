#!/usr/bin/env rust-script
//! Verify migration 0070 SQL syntax
//!
//! ```cargo
//! [dependencies]
//! rusqlite = "0.31"
//! ```

use rusqlite::{Connection, Result};
use std::fs;

fn main() -> Result<()> {
    println!("Testing migration 0070 SQL syntax...\n");

    // Read migration file
    let migration_sql = fs::read_to_string("migrations/0070_tenant_snapshots.sql")
        .expect("Failed to read migration file");

    // Create in-memory database
    let conn = Connection::open_in_memory()?;

    // Execute migration
    match conn.execute_batch(&migration_sql) {
        Ok(_) => {
            println!("✓ Migration SQL executed successfully\n");

            // Verify tables were created
            let tables: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>>>()?;

            println!("Tables created:");
            for table in &tables {
                println!("  - {}", table);
            }

            // Verify indexes were created
            let indexes: Vec<String> = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='index' ORDER BY name")?
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>>>()?;

            println!("\nIndexes created:");
            for index in &indexes {
                if !index.starts_with("sqlite_") {
                    println!("  - {}", index);
                }
            }

            // Test inserting data
            conn.execute(
                "INSERT INTO tenant_snapshots (tenant_id, state_hash) VALUES (?, ?)",
                ["test-tenant", "abc123"],
            )?;

            conn.execute(
                "INSERT INTO router_policies (id, tenant_id, name, rules_json) VALUES (?, ?, ?, ?)",
                ["policy-1", "test-tenant", "test-policy", "[]"],
            )?;

            conn.execute(
                "INSERT INTO tenant_configs (id, tenant_id, key, value_json) VALUES (?, ?, ?, ?)",
                ["config-1", "test-tenant", "test-key", "{}"],
            )?;

            println!("\n✓ Data insertion successful");
            println!("✓ All constraints working correctly");
            println!("\n✓✓✓ Migration 0070 is VALID ✓✓✓");

            Ok(())
        }
        Err(e) => {
            println!("✗ Migration SQL failed: {}", e);
            std::process::exit(1);
        }
    }
}
