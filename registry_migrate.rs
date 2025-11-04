//! Registry Database Migration Tool
//!
//! Migrates old registry.db data to new schema format.
//!
//! Usage:
//!     cargo run --bin registry_migrate -- --old-db deprecated/registry.db --new-db var/registry.db

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::Registry;
use clap::Parser;
use rusqlite::{Connection, Row};
use std::path::PathBuf;
use tracing::{info, warn, error};

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    /// Path to old registry database
    #[arg(long)]
    old_db: PathBuf,

    /// Path to new registry database (will be created if doesn't exist)
    #[arg(long)]
    new_db: PathBuf,

    /// Dry run - show what would be migrated without making changes
    #[arg(long)]
    dry_run: bool,

    /// Force migration even if new database exists
    #[arg(long)]
    force: bool,
}

/// Old adapter record format from V1 schema
#[derive(Debug)]
struct OldAdapterRecord {
    id: String,
    hash: String,
    tier: String,
    rank: i32,
    acl: String,
    activation_pct: f64,
    registered_at: String,
}

/// Old tenant record format
#[derive(Debug)]
struct OldTenantRecord {
    id: String,
    uid: i32,
    gid: i32,
    created_at: String,
}

/// Migration statistics
#[derive(Debug, Default)]
struct MigrationStats {
    adapters_processed: usize,
    adapters_migrated: usize,
    adapters_skipped: usize,
    adapters_failed: usize,
    tenants_processed: usize,
    tenants_migrated: usize,
    tenants_skipped: usize,
    tenants_failed: usize,
}

impl OldAdapterRecord {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            hash: row.get(1)?,
            tier: row.get(2)?,
            rank: row.get(3)?,
            acl: row.get(4)?,
            activation_pct: row.get(5)?,
            registered_at: row.get(6)?,
        })
    }

    /// Transform old adapter record to new format parameters
    fn to_new_params(&self) -> Result<(String, String, B3Hash, String, u32, f32, String, Option<String>, Option<String>, Option<String>, Option<String>)> {
        // Parse hash - assume it's hex format, convert to B3Hash
        let hash = B3Hash::from_hex(&self.hash)
            .map_err(|e| AosError::Registry(format!("Invalid hash format for adapter {}: {}", self.id, e)))?;

        // Extract tenant_id and name from id (assume format: "tenant-name")
        let parts: Vec<&str> = self.id.split('-').collect();
        let (tenant_id, name) = if parts.len() >= 2 {
            let tenant_id = parts[0].to_string();
            let name = parts[1..].join("-");
            (tenant_id, name)
        } else {
            // Default fallback
            ("default".to_string(), self.id.clone())
        };

        // Transform ACL - assume simple format, convert to JSON
        let acl_json = if self.acl.trim().is_empty() {
            None
        } else {
            Some(format!(r#"["{}"]"#, self.acl.replace(',', r#"",""#)))
        };

        // Default values for fields not in old schema
        let targets_json = r#"["unknown"]"#.to_string();
        let languages_json = Some(r#"["en"]"#.to_string());
        let framework = Some("unknown".to_string());
        let adapter_id = Some(self.id.clone());

        Ok((
            tenant_id,
            name,
            hash,
            self.tier.clone(),
            self.rank as u32,
            1.0, // alpha default
            targets_json,
            acl_json,
            adapter_id,
            languages_json,
            framework,
        ))
    }
}

impl OldTenantRecord {
    fn from_row(row: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            uid: row.get(1)?,
            gid: row.get(2)?,
            created_at: row.get(3)?,
        })
    }

    /// Transform old tenant record to new format parameters
    fn to_new_params(&self) -> (String, String, bool) {
        // Use id as name, assume no ITAR flag (can be updated manually if needed)
        (self.id.clone(), self.id.clone(), false)
    }
}

fn extract_old_data(old_db_path: &PathBuf) -> Result<(Vec<OldAdapterRecord>, Vec<OldTenantRecord>)> {
    info!("Opening old registry database: {:?}", old_db_path);
    let conn = Connection::open(old_db_path)
        .map_err(|e| AosError::Registry(format!("Failed to open old database: {}", e)))?;

    // Extract adapters
    let mut adapters = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, hash, tier, rank, acl, activation_pct, registered_at FROM adapters")?;
        let adapter_iter = stmt.query_map([], |row| OldAdapterRecord::from_row(row))?;

        for adapter_result in adapter_iter {
            match adapter_result {
                Ok(adapter) => adapters.push(adapter),
                Err(e) => warn!("Failed to read adapter record: {}", e),
            }
        }
    }

    // Extract tenants
    let mut tenants = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT id, uid, gid, created_at FROM tenants")?;
        let tenant_iter = stmt.query_map([], |row| OldTenantRecord::from_row(row))?;

        for tenant_result in tenant_iter {
            match tenant_result {
                Ok(tenant) => tenants.push(tenant),
                Err(e) => warn!("Failed to read tenant record: {}", e),
            }
        }
    }

    info!("Extracted {} adapters and {} tenants from old database", adapters.len(), tenants.len());
    Ok((adapters, tenants))
}

async fn migrate_data(
    adapters: &[OldAdapterRecord],
    tenants: &[OldTenantRecord],
    new_registry: &Registry,
    dry_run: bool,
) -> Result<MigrationStats> {
    let mut stats = MigrationStats::default();

    // Migrate tenants first
    for tenant in tenants {
        stats.tenants_processed += 1;

        if dry_run {
            info!("DRY RUN: Would migrate tenant {}", tenant.id);
            stats.tenants_migrated += 1;
            continue;
        }

        let (id, name, itar_flag) = tenant.to_new_params();
        match new_registry.register_tenant(&id, &name, itar_flag).await {
            Ok(_) => {
                info!("Migrated tenant: {}", id);
                stats.tenants_migrated += 1;
            }
            Err(e) => {
                // Check if tenant already exists (OK for idempotent migration)
                if let Some(existing) = new_registry.get_tenant(&id).await? {
                    if existing.name == name {
                        info!("Tenant {} already exists, skipping", id);
                        stats.tenants_skipped += 1;
                        continue;
                    }
                }
                error!("Failed to migrate tenant {}: {}", id, e);
                stats.tenants_failed += 1;
            }
        }
    }

    // Migrate adapters
    for adapter in adapters {
        stats.adapters_processed += 1;

        if dry_run {
            info!("DRY RUN: Would migrate adapter {}", adapter.id);
            stats.adapters_migrated += 1;
            continue;
        }

        match adapter.to_new_params() {
            Ok(params) => {
                let (tenant_id, name, hash, tier, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework) = params;

                match new_registry.register_adapter(
                    &tenant_id, &name, &hash, &tier, rank, alpha, &targets_json,
                    acl_json.as_deref(), adapter_id.as_deref(), languages_json.as_deref(), framework.as_deref()
                ).await {
                    Ok(_) => {
                        info!("Migrated adapter: {}-{}", tenant_id, name);
                        stats.adapters_migrated += 1;
                    }
                    Err(e) => {
                        error!("Failed to migrate adapter {}: {}", adapter.id, e);
                        stats.adapters_failed += 1;
                    }
                }
            }
            Err(e) => {
                error!("Failed to transform adapter {}: {}", adapter.id, e);
                stats.adapters_failed += 1;
            }
        }
    }

    Ok(stats)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::init();

    let args = Args::parse();

    info!("AdapterOS Registry Migration Tool");
    info!("==================================");

    // Validate inputs
    if !args.old_db.exists() {
        error!("Old database does not exist: {:?}", args.old_db);
        std::process::exit(1);
    }

    if args.new_db.exists() && !args.force && !args.dry_run {
        error!("New database already exists: {:?}. Use --force to overwrite or --dry-run to preview.", args.new_db);
        std::process::exit(1);
    }

    // Extract data from old database
    let (adapters, tenants) = match extract_old_data(&args.old_db) {
        Ok(data) => data,
        Err(e) => {
            error!("Failed to extract data from old database: {}", e);
            std::process::exit(1);
        }
    };

    if adapters.is_empty() && tenants.is_empty() {
        info!("No data found in old database. Nothing to migrate.");
        return Ok(());
    }

    // Create new registry (unless dry run)
    let registry = if args.dry_run {
        info!("DRY RUN: Skipping registry creation");
        None
    } else {
        match Registry::open(&args.new_db).await {
            Ok(reg) => {
                info!("✓ New registry database created at {:?}", args.new_db);
                Some(reg)
            }
            Err(e) => {
                error!("Failed to create new registry: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Perform migration
    let stats = if let Some(ref reg) = registry {
        migrate_data(&adapters, &tenants, reg, args.dry_run).await?
    } else {
        // Dry run stats
        MigrationStats {
            adapters_processed: adapters.len(),
            adapters_migrated: adapters.len(),
            tenants_processed: tenants.len(),
            tenants_migrated: tenants.len(),
            ..Default::default()
        }
    };

    // Report results
    println!("\nMigration Complete");
    println!("==================");
    println!("Tenants:");
    println!("  Processed: {}", stats.tenants_processed);
    println!("  Migrated:  {}", stats.tenants_migrated);
    println!("  Skipped:   {}", stats.tenants_skipped);
    println!("  Failed:    {}", stats.tenants_failed);
    println!("Adapters:");
    println!("  Processed: {}", stats.adapters_processed);
    println!("  Migrated:  {}", stats.adapters_migrated);
    println!("  Skipped:   {}", stats.adapters_skipped);
    println!("  Failed:    {}", stats.adapters_failed);

    if stats.adapters_failed > 0 || stats.tenants_failed > 0 {
        warn!("Some records failed to migrate. Check logs above for details.");
        if !args.dry_run {
            warn!("You may need to manually migrate failed records.");
        }
    } else {
        info!("✓ All records migrated successfully!");
    }

    Ok(())
}
