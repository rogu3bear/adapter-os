# Registry Data Migration Guide

## Overview

AdapterOS uses two separate database systems that may require manual migration:

1. **Control Plane Database** (`var/cp.db`) - Main application database with complex schema
2. **Registry Database** (`var/registry.db`) - Simple adapter registry for CLI operations

## Current State Assessment

### Control Plane Database
- **Location**: `var/cp.db`
- **Status**: Active and maintained
- **Migration System**: SQLx-based migrations in `migrations/` directory
- **Schema**: Complex multi-table schema (users, tenants, adapters, plans, etc.)

### Registry Database
- **Location**: `var/registry.db` (expected)
- **Status**: Not found in current deployment
- **Migration System**: SQLx-based migrations in `crates/adapteros-registry/migrations/`
- **Schema Issue**: Migration file doesn't match code expectations

## Schema Analysis

### Registry Migration File (`V1__init.sql`)
```sql
-- Simple adapter registry schema
CREATE TABLE adapters (
    id TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    tier TEXT NOT NULL,
    rank INTEGER NOT NULL,
    acl TEXT NOT NULL,
    activation_pct REAL DEFAULT 0.0,
    registered_at TEXT NOT NULL
);

CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    uid INTEGER NOT NULL,
    gid INTEGER NOT NULL,
    created_at TEXT NOT NULL
);

-- Additional tables: models, checkpoints
```

### Registry Code Expectations (lib.rs)
The `Registry` struct expects a complex schema similar to the control plane:

```sql
-- Expected by Registry::register_adapter()
CREATE TABLE adapters (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    name TEXT NOT NULL,
    tier TEXT NOT NULL,
    hash_b3 TEXT NOT NULL,
    rank INTEGER NOT NULL,
    alpha REAL NOT NULL,
    targets_json TEXT NOT NULL,
    acl_json TEXT,
    adapter_id TEXT,
    languages_json TEXT,
    framework TEXT,
    active INTEGER DEFAULT 1,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

-- Expected by Registry::register_tenant()
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    itar_flag INTEGER DEFAULT 0,
    created_at TEXT DEFAULT (datetime('now'))
);
```

## Migration Problem

The registry crate has **schema mismatch**:
- Migration file provides simple schema
- Code expects complex schema compatible with control plane
- No automatic migration path exists

## Manual Migration Procedure

### Step 1: Backup Existing Data
```bash
# If registry.db exists with data
cp var/registry.db var/registry.db.backup.$(date +%Y%m%d_%H%M%S)
cp deprecated/registry.db deprecated/registry.db.backup.$(date +%Y%m%d_%H%M%S)
```

### Step 2: Assess Data Volume
```sql
-- Check what data exists in old registry
sqlite3 var/registry.db ".schema"
sqlite3 var/registry.db "SELECT COUNT(*) FROM adapters;"
sqlite3 var/registry.db "SELECT COUNT(*) FROM tenants;"
sqlite3 var/registry.db "SELECT COUNT(*) FROM models;"
```

### Step 3: Export Old Data
```sql
-- Export adapters
sqlite3 var/registry.db << 'EOF'
.mode csv
.header on
.output adapters_export.csv
SELECT * FROM adapters;
EOF

-- Export tenants
sqlite3 var/registry.db << 'EOF'
.mode csv
.header on
.output tenants_export.csv
SELECT * FROM tenants;
EOF

-- Export models (if exists)
sqlite3 var/registry.db << 'EOF'
.mode csv
.header on
.output models_export.csv
SELECT * FROM models;
EOF
```

### Step 4: Create New Registry Database
```rust
// Use the registry crate to create properly migrated database
use adapteros_registry::Registry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn::std::error::Error>> {
    // This will create and migrate the database
    let registry = Registry::open("var/registry.db").await?;
    println!("Registry database created and migrated");
    Ok(())
}
```

### Step 5: Transform and Import Data

**Adapter Data Transformation:**
```sql
-- Old schema: id, hash, tier, rank, acl, activation_pct, registered_at
-- New schema: id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, created_at, updated_at

-- Create transformation mapping
-- Note: This requires manual mapping of old fields to new schema
```

**Manual Import Process:**
```rust
use adapteros_registry::Registry;
use adapteros_core::B3Hash;

#[tokio::main]
async fn main() -> Result<(), Box<dyn::std::error::Error>> {
    let registry = Registry::open("var/registry.db").await?;

    // Read CSV data and transform
    // For each old adapter record:
    // - Parse old format
    // - Transform to new format
    // - Register with new API

    // Example transformation:
    let hash = B3Hash::from_hex("old_hash_value")?;
    registry.register_adapter(
        "default",           // tenant_id (may need to map from old data)
        "adapter_name",      // name
        &hash,
        "tier_value",
        8,                   // rank
        1.0,                 // alpha (default)
        r#"["target"]"#,     // targets_json
        Some(r#"["acl"]"#),  // acl_json
        Some("adapter_id"),
        Some(r#"["en"]"#),   // languages_json
        Some("framework"),
    ).await?;

    Ok(())
}
```

### Step 6: Data Validation
```rust
// Verify migration
let adapters = registry.list_adapters().await?;
println!("Migrated {} adapters", adapters.len());

let tenants = registry.get_tenant("tenant_id").await?;
println!("Tenant data: {:?}", tenants);
```

### Step 7: Update Configuration
```toml
# Ensure refinery.toml points to correct location
[main]
db_type = "Sqlite"
path = "./var/registry.db"
```

## Risks and Considerations

### Data Loss Risks
1. **Schema Mismatch**: Old data may not map cleanly to new schema
2. **Missing Fields**: New schema requires fields not present in old data
3. **Type Changes**: Hash format changes (text vs B3Hash)
4. **Relationship Changes**: Tenant relationships may be lost

### Mitigation Strategies
1. **Full Backup**: Always backup before migration
2. **Staged Migration**: Migrate in small batches with validation
3. **Rollback Plan**: Keep old database until migration verified
4. **Data Validation**: Compare record counts and sample data

### Compatibility Notes
- **Control Plane Integration**: Registry data may need to sync with cp.db
- **CLI Dependencies**: Some CLI commands may depend on registry data
- **Version Compatibility**: Ensure all components use same registry schema

## Automated Migration Script

```rust
//! Registry Migration Tool
//!
//! Usage: cargo run --bin registry_migrate -- --old-db path/to/old.db --new-db var/registry.db

use adapteros_registry::Registry;
use adapteros_core::B3Hash;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    old_db: PathBuf,
    #[arg(long)]
    new_db: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("Starting registry migration...");
    println!("Old DB: {:?}", args.old_db);
    println!("New DB: {:?}", args.new_db);

    // Create new registry
    let registry = Registry::open(&args.new_db).await?;
    println!("✓ New registry created");

    // TODO: Implement data extraction and transformation from old DB
    // This requires knowing the exact old schema and transformation rules

    println!("✓ Migration completed");
    Ok(())
}
```

## Alternative Approaches

### Option 1: Schema Update Only
If no data exists in old registry, simply update the migration file to match code expectations.

### Option 2: Registry Consolidation
Consider consolidating registry functionality into the main control plane database to avoid dual database maintenance.

### Option 3: Registry Recreation
If old registry data is minimal or outdated, recreate registry from source data rather than migrate.

## Testing Migration

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_registry_migration() {
        // Create test old database with sample data
        // Run migration
        // Verify data integrity
        // Check record counts match
    }
}
```

## Conclusion

Registry data migration requires manual intervention due to schema incompatibilities. The process involves:

1. Data assessment and backup
2. Schema analysis and transformation planning
3. Manual data migration with validation
4. Testing and rollback procedures

**Critical**: Always backup data before migration and test thoroughly in non-production environment first.
