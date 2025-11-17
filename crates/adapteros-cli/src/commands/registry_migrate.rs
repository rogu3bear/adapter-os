//! Registry database migration command
//!
//! Migrates old registry.db data to new schema format.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::Registry;
use clap::Parser;
use rusqlite::{Connection, Row};
use std::path::PathBuf;
use tracing::{error, info, warn};

use crate::output::OutputWriter;

#[derive(Parser, Debug, Clone)]
pub struct RegistryMigrateArgs {
    /// Path to old registry database
    #[arg(long, default_value = "deprecated/registry.db")]
    pub from_db: PathBuf,

    /// Path to new registry database (will be created if doesn't exist)
    #[arg(long, default_value = "var/registry.db")]
    pub to_db: PathBuf,

    /// Dry run - show what would be migrated without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Force migration even if new database exists
    #[arg(long)]
    pub force: bool,
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
    fn to_new_params(
        &self,
    ) -> Result<(
        String,
        String,
        B3Hash,
        String,
        u32,
        f32,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        // Parse hash - assume it's hex format, convert to B3Hash
        let hash = B3Hash::from_hex(&self.hash).map_err(|e| {
            AosError::Registry(format!(
                "Invalid hash format for adapter {}: {}",
                self.id, e
            ))
        })?;

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
            "[]".to_string()
        } else {
            format!("[\"{}\"]", self.acl)
        };

        Ok((
            self.id.clone(),
            tenant_id,
            hash,
            name,
            self.rank as u32,
            self.activation_pct as f32,
            acl_json,
            Some(self.tier.clone()),
            None, // path
            None, // backend
            None, // quantization
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
}

fn extract_old_data(
    old_db_path: &PathBuf,
) -> Result<(Vec<OldAdapterRecord>, Vec<OldTenantRecord>)> {
    let conn = Connection::open(old_db_path)
        .map_err(|e| AosError::Registry(format!("Failed to open old database: {}", e)))?;

    // Extract adapters
    let adapters = {
        let mut stmt = conn
            .prepare(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at FROM adapters",
            )
            .map_err(|e| AosError::Registry(format!("Failed to prepare adapter query: {}", e)))?;

        let adapter_iter = stmt
            .query_map([], OldAdapterRecord::from_row)
            .map_err(|e| AosError::Registry(format!("Failed to query adapters: {}", e)))?;

        adapter_iter
            .filter_map(|r| match r {
                Ok(adapter) => Some(adapter),
                Err(e) => {
                    warn!("Failed to parse adapter record: {}", e);
                    None
                }
            })
            .collect()
    };

    // Extract tenants
    let tenants = {
        let mut stmt = conn
            .prepare("SELECT id, uid, gid, created_at FROM tenants")
            .map_err(|e| AosError::Registry(format!("Failed to prepare tenant query: {}", e)))?;

        let tenant_iter = stmt
            .query_map([], OldTenantRecord::from_row)
            .map_err(|e| AosError::Registry(format!("Failed to query tenants: {}", e)))?;

        tenant_iter
            .filter_map(|r| match r {
                Ok(tenant) => Some(tenant),
                Err(e) => {
                    warn!("Failed to parse tenant record: {}", e);
                    None
                }
            })
            .collect()
    };

    Ok((adapters, tenants))
}

async fn migrate_data(
    adapters: &[OldAdapterRecord],
    tenants: &[OldTenantRecord],
    registry: &Registry,
    dry_run: bool,
) -> Result<MigrationStats> {
    let mut stats = MigrationStats::default();

    // Migrate tenants first
    for tenant in tenants {
        stats.tenants_processed += 1;

        if dry_run {
            info!("[DRY RUN] Would migrate tenant: {}", tenant.id);
            stats.tenants_migrated += 1;
        } else {
            match registry
                .register_tenant(&tenant.id, tenant.uid as u32, tenant.gid as u32)
                .await
            {
                Ok(_) => {
                    info!("✓ Migrated tenant: {}", tenant.id);
                    stats.tenants_migrated += 1;
                }
                Err(e) if e.to_string().contains("UNIQUE") => {
                    warn!("⊘ Skipped tenant (already exists): {}", tenant.id);
                    stats.tenants_skipped += 1;
                }
                Err(e) => {
                    error!("✗ Failed to migrate tenant {}: {}", tenant.id, e);
                    stats.tenants_failed += 1;
                }
            }
        }
    }

    // Migrate adapters
    for adapter in adapters {
        stats.adapters_processed += 1;

        if dry_run {
            info!("[DRY RUN] Would migrate adapter: {}", adapter.id);
            stats.adapters_migrated += 1;
        } else {
            match adapter.to_new_params() {
                Ok((
                    id,
                    tenant_id,
                    hash,
                    name,
                    rank,
                    activation_pct,
                    acl,
                    tier,
                    path,
                    backend,
                    quantization,
                )) => {
                    match registry
                        .register_adapter_full(
                            &id,
                            &tenant_id,
                            &hash,
                            &name,
                            rank,
                            activation_pct,
                            &acl,
                            tier.as_deref(),
                            path.as_deref(),
                            backend.as_deref(),
                            quantization.as_deref(),
                        )
                        .await
                    {
                        Ok(_) => {
                            info!("✓ Migrated adapter: {}", id);
                            stats.adapters_migrated += 1;
                        }
                        Err(e) if e.to_string().contains("UNIQUE") => {
                            warn!("⊘ Skipped adapter (already exists): {}", id);
                            stats.adapters_skipped += 1;
                        }
                        Err(e) => {
                            error!("✗ Failed to migrate adapter {}: {}", id, e);
                            stats.adapters_failed += 1;
                        }
                    }
                }
                Err(e) => {
                    error!("✗ Failed to transform adapter {}: {}", adapter.id, e);
                    stats.adapters_failed += 1;
                }
            }
        }
    }

    Ok(stats)
}

/// Run registry migration
pub async fn run(args: RegistryMigrateArgs, output: &OutputWriter) -> Result<()> {
    output.info("AdapterOS Registry Migration Tool")?;
    output.info("==================================")?;

    // Validate inputs
    if !args.from_db.exists() {
        return Err(AosError::Registry(format!(
            "Old database does not exist: {:?}",
            args.from_db
        )));
    }

    if args.to_db.exists() && !args.force && !args.dry_run {
        return Err(AosError::Registry(format!(
            "New database already exists: {:?}. Use --force to overwrite or --dry-run to preview.",
            args.to_db
        )));
    }

    // Extract data from old database
    let (adapters, tenants) = extract_old_data(&args.from_db)?;

    if adapters.is_empty() && tenants.is_empty() {
        output.info("No data found in old database. Nothing to migrate.")?;
        return Ok(());
    }

    // Create new registry (unless dry run)
    let registry = if args.dry_run {
        output.info("DRY RUN: Skipping registry creation")?;
        None
    } else {
        match Registry::open(&args.to_db).await {
            Ok(reg) => {
                output.success(&format!(
                    "✓ New registry database created at {:?}",
                    args.to_db
                ))?;
                Some(reg)
            }
            Err(e) => {
                return Err(AosError::Registry(format!(
                    "Failed to create new registry: {}",
                    e
                )));
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
    output.info("")?;
    output.info("Migration Complete")?;
    output.info("==================")?;
    output.info(&format!("Tenants:"))?;
    output.info(&format!("  Processed: {}", stats.tenants_processed))?;
    output.info(&format!("  Migrated:  {}", stats.tenants_migrated))?;
    output.info(&format!("  Skipped:   {}", stats.tenants_skipped))?;
    output.info(&format!("  Failed:    {}", stats.tenants_failed))?;
    output.info(&format!("Adapters:"))?;
    output.info(&format!("  Processed: {}", stats.adapters_processed))?;
    output.info(&format!("  Migrated:  {}", stats.adapters_migrated))?;
    output.info(&format!("  Skipped:   {}", stats.adapters_skipped))?;
    output.info(&format!("  Failed:    {}", stats.adapters_failed))?;

    if stats.adapters_failed > 0 || stats.tenants_failed > 0 {
        output.warn("Some records failed to migrate. Check logs above for details.")?;
        if !args.dry_run {
            output.warn("You may need to manually migrate failed records.")?;
        }
    } else {
        output.success("✓ All records migrated successfully!")?;
    }

    Ok(())
}
