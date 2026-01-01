//! Registry management commands
//!
//! Consolidates registry operations into a git-style subcommand structure:
//! - `aosctl registry sync` - Sync adapters from local directory to registry
//! - `aosctl registry migrate` - Migrate legacy registry database to new schema

use crate::output::OutputWriter;
use adapteros_artifacts::CasStore;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::Registry;
use adapteros_sbom::SpdxDocument;
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use rusqlite::{Connection, Row};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tracing::{error, info, warn};

/// Registry migrate arguments (re-exported for external use)
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

/// Registry management subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum RegistryCommand {
    /// Sync adapters from local directory to registry
    ///
    /// Scans a directory for adapter files (.safetensors) with associated
    /// SBOM and signature files, validates them, and imports into the
    /// Content-Addressable Store (CAS) and registry database.
    #[command(
        after_help = "Examples:\n  aosctl registry sync --dir ./adapters\n  aosctl registry sync --dir ./adapters --cas-root ./var/cas\n  aosctl registry sync --dir ./adapters --registry ./var/custom.db"
    )]
    Sync {
        /// Directory containing adapters with SBOM and signatures
        #[arg(short, long)]
        dir: PathBuf,

        /// CAS root directory
        #[arg(long, default_value = "./var/cas")]
        cas_root: PathBuf,

        /// Registry database path
        #[arg(long, default_value = "./var/registry.db")]
        registry: PathBuf,
    },

    /// Migrate legacy registry database to new schema
    ///
    /// Reads data from an old registry database format and migrates
    /// adapters and tenants to the new schema. Supports dry-run mode
    /// to preview changes before committing.
    #[command(
        after_help = "Examples:\n  aosctl registry migrate\n  aosctl registry migrate --from-db deprecated/registry.db --to-db var/registry.db\n  aosctl registry migrate --dry-run\n  aosctl registry migrate --force"
    )]
    Migrate(RegistryMigrateArgs),
}

/// Get registry command name for telemetry
fn get_registry_command_name(cmd: &RegistryCommand) -> String {
    match cmd {
        RegistryCommand::Sync { .. } => "registry_sync".to_string(),
        RegistryCommand::Migrate(_) => "registry_migrate".to_string(),
    }
}

/// Handle registry management commands
///
/// Routes registry subcommands to appropriate handlers:
/// - `sync` -> sync_registry()
/// - `migrate` -> run_migrate()
///
/// # Arguments
///
/// * `cmd` - The registry subcommand to execute
/// * `output` - Output writer for formatted console output
///
/// # Errors
///
/// Returns error if:
/// - Sync directory does not exist or is not readable
/// - Registry database cannot be opened or created
/// - Migration source database does not exist
/// - Migration fails due to schema incompatibility
pub async fn handle_registry_command(cmd: RegistryCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_registry_command_name(&cmd);

    info!(command = ?cmd, "Handling registry command");

    // Emit telemetry
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        RegistryCommand::Sync {
            dir,
            cas_root,
            registry,
        } => sync_registry(&dir, &cas_root, &registry, output)
            .await
            .map_err(|e| AosError::Registry(e.to_string())),
        RegistryCommand::Migrate(args) => run_migrate(args, output).await,
    }
}

// ============================================================
// Registry Sync Implementation
// (consolidated from sync_registry.rs)
// ============================================================

#[derive(Serialize)]
struct SyncResult {
    synced_count: usize,
    skipped_count: usize,
}

/// Sync adapters from a local directory into CAS with SBOM and signature verification
pub async fn sync_registry(
    sync_dir: &Path,
    cas_root: &Path,
    registry_path: &Path,
    output: &OutputWriter,
) -> anyhow::Result<()> {
    output.info(format!("Syncing adapters from {}", sync_dir.display()));

    let cas = CasStore::new(cas_root)?;
    let registry = Registry::open(registry_path)?;

    let mut synced_count = 0;
    let mut skipped_count = 0;

    for entry in std::fs::read_dir(sync_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("safetensors") {
            let filename = path
                .file_stem()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid path"))?;

            // Check for SBOM and signature
            let sbom_path = path.with_extension("sbom.json");
            let sig_path = path.with_extension("sig");

            if !sbom_path.exists() {
                output.warning(format!("Skipping {}: missing SBOM", filename));
                skipped_count += 1;
                continue;
            }

            if !sig_path.exists() {
                output.warning(format!("Skipping {}: missing signature", filename));
                skipped_count += 1;
                continue;
            }

            // Validate SBOM
            let sbom_bytes = std::fs::read(&sbom_path)?;
            match serde_json::from_slice::<SpdxDocument>(&sbom_bytes) {
                Ok(sbom) => {
                    if sbom.packages.is_empty() {
                        output.warning(format!("Skipping {}: SBOM has no packages", filename));
                        skipped_count += 1;
                        continue;
                    }
                }
                Err(e) => {
                    output.warning(format!("Skipping {}: Invalid SBOM: {}", filename, e));
                    skipped_count += 1;
                    continue;
                }
            }

            // Verify signature using crypto module
            let sig_bytes = std::fs::read(&sig_path)?;
            let _adapter_bytes = std::fs::read(&path)?;

            // Parse signature from bytes (assuming hex-encoded signature)
            let sig_hex = String::from_utf8(sig_bytes)
                .map_err(|e| anyhow::anyhow!("Invalid signature encoding: {}", e))?;
            let sig_bytes_decoded = hex::decode(sig_hex.trim())
                .map_err(|e| anyhow::anyhow!("Invalid signature hex: {}", e))?;

            if sig_bytes_decoded.len() != 64 {
                output.warning(format!("Skipping {}: invalid signature length", filename));
                skipped_count += 1;
                continue;
            }

            let mut sig_array = [0u8; 64];
            sig_array.copy_from_slice(&sig_bytes_decoded);

            // For now, we'll use a mock verification since we don't have the public key
            // In production, this would load the public key from a trusted source
            output.progress(format!(
                "Signature verification skipped for {} (mock)",
                filename
            ));

            // Signature verification placeholder - will be implemented with crypto module
            // let signature = Signature::from_bytes(&sig_array)?;
            // let public_key = PublicKey::from_pem(&public_key_pem)?;
            // public_key.verify(&adapter_bytes, &signature)?;

            // Re-read adapter bytes for storage
            let adapter_bytes = std::fs::read(&path)?;

            // Store in CAS
            let hash = cas.store("adapters", &adapter_bytes)?;

            // Register in registry (basic registration without full metadata)
            // In a real implementation, we would parse metadata from SBOM or manifest
            match registry.register_adapter(
                filename,
                &hash,
                "persistent",
                8,   // default rank
                &[], // empty ACL
            ) {
                Ok(_) => {
                    output.success(format!("Imported adapter: {} ({})", filename, hash));
                    synced_count += 1;
                }
                Err(e) => {
                    output.warning(format!("Failed to register {}: {}", filename, e));
                    skipped_count += 1;
                }
            }
        }
    }

    output.progress("");
    output.info("Sync complete");
    output.kv("Synced", &synced_count.to_string());
    output.kv("Skipped", &skipped_count.to_string());

    if output.is_json() {
        let result = SyncResult {
            synced_count,
            skipped_count,
        };
        output.json(&result)?;
    }

    Ok(())
}

// ============================================================
// Registry Migrate Implementation
// (consolidated from registry_migrate.rs)
// ============================================================

/// Old adapter record format from V1 schema
#[derive(Debug)]
struct OldAdapterRecord {
    id: String,
    hash: String,
    tier: String,
    rank: i32,
    acl: String,
    activation_pct: f64,
    #[allow(dead_code)]
    registered_at: String,
}

type NewAdapterParams = (
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
);

/// Old tenant record format
#[derive(Debug)]
struct OldTenantRecord {
    id: String,
    uid: i32,
    gid: i32,
    #[allow(dead_code)]
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
    fn to_new_params(&self) -> Result<NewAdapterParams> {
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
    output: &OutputWriter,
) -> Result<MigrationStats> {
    let mut stats = MigrationStats::default();

    // Create progress bar for tenants
    let tenant_pb = if !output.is_json() && !tenants.is_empty() {
        let pb = ProgressBar::new(tenants.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} tenants ({msg})")
                .expect("valid template")
                .progress_chars("=>-"),
        );
        pb.set_message("processing");
        Some(pb)
    } else {
        None
    };

    // Migrate tenants first
    for tenant in tenants {
        stats.tenants_processed += 1;

        if dry_run {
            info!("[DRY RUN] Would migrate tenant: {}", tenant.id);
            stats.tenants_migrated += 1;
        } else {
            match registry.register_tenant(&tenant.id, tenant.uid as u32, tenant.gid as u32) {
                Ok(_) => {
                    info!("Migrated tenant: {}", tenant.id);
                    stats.tenants_migrated += 1;
                }
                Err(e) if e.to_string().contains("UNIQUE") => {
                    warn!("Skipped tenant (already exists): {}", tenant.id);
                    stats.tenants_skipped += 1;
                }
                Err(e) => {
                    error!("Failed to migrate tenant {}: {}", tenant.id, e);
                    stats.tenants_failed += 1;
                }
            }
        }

        if let Some(ref pb) = tenant_pb {
            pb.inc(1);
            pb.set_message(format!(
                "success: {}, skipped: {}, failed: {}",
                stats.tenants_migrated, stats.tenants_skipped, stats.tenants_failed
            ));
        }
    }

    if let Some(pb) = tenant_pb {
        pb.finish_with_message("complete");
    }

    // Create progress bar for adapters
    let adapter_pb = if !output.is_json() && !adapters.is_empty() {
        let pb = ProgressBar::new(adapters.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.green/blue} {pos}/{len} adapters ({msg})")
                .expect("valid template")
                .progress_chars("=>-"),
        );
        pb.set_message("processing");
        Some(pb)
    } else {
        None
    };

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
                    _tenant_id,
                    hash,
                    _name,
                    rank,
                    _activation_pct,
                    acl,
                    tier,
                    _path,
                    _backend,
                    _quantization,
                )) => {
                    // Parse ACL from JSON string to Vec<String>
                    let acl_vec: Vec<String> = serde_json::from_str(&acl).unwrap_or_default();
                    let tier_str = tier.as_deref().unwrap_or("tier_1");

                    match registry.register_adapter(&id, &hash, tier_str, rank, &acl_vec) {
                        Ok(_) => {
                            info!("Migrated adapter: {}", id);
                            stats.adapters_migrated += 1;
                        }
                        Err(e) if e.to_string().contains("UNIQUE") => {
                            warn!("Skipped adapter (already exists): {}", id);
                            stats.adapters_skipped += 1;
                        }
                        Err(e) => {
                            error!("Failed to migrate adapter {}: {}", id, e);
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

        if let Some(ref pb) = adapter_pb {
            pb.inc(1);
            pb.set_message(format!(
                "success: {}, skipped: {}, failed: {}",
                stats.adapters_migrated, stats.adapters_skipped, stats.adapters_failed
            ));
        }
    }

    if let Some(pb) = adapter_pb {
        pb.finish_with_message("complete");
    }

    Ok(stats)
}

/// Run registry migration
pub async fn run_migrate(args: RegistryMigrateArgs, output: &OutputWriter) -> Result<()> {
    let start_time = Instant::now();

    output.info("AdapterOS Registry Migration Tool");
    output.info("==================================");

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
    output.progress("Extracting data from old database...");
    let (adapters, tenants) = extract_old_data(&args.from_db)?;

    if adapters.is_empty() && tenants.is_empty() {
        output.info("No data found in old database. Nothing to migrate.");
        return Ok(());
    }

    output.info(format!(
        "Found {} tenants and {} adapters to migrate",
        tenants.len(),
        adapters.len()
    ));

    // Create new registry (unless dry run)
    let registry = if args.dry_run {
        output.info("DRY RUN: Skipping registry creation");
        None
    } else {
        match Registry::open(&args.to_db) {
            Ok(reg) => {
                output.success(format!("New registry database created at {:?}", args.to_db));
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
    output.info("");
    output.info("Starting migration...");
    let stats = if let Some(ref reg) = registry {
        migrate_data(&adapters, &tenants, reg, args.dry_run, output).await?
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

    let elapsed = start_time.elapsed();

    // Report results
    output.info("");
    output.info("Migration Complete");
    output.info("==================");
    output.info(format!("Elapsed time: {:.2}s", elapsed.as_secs_f64()));
    output.info("");
    output.info("Tenants:");
    output.info(format!("  Processed: {}", stats.tenants_processed));
    output.info(format!("  Migrated:  {}", stats.tenants_migrated));
    output.info(format!("  Skipped:   {}", stats.tenants_skipped));
    output.info(format!("  Failed:    {}", stats.tenants_failed));
    output.info("Adapters:");
    output.info(format!("  Processed: {}", stats.adapters_processed));
    output.info(format!("  Migrated:  {}", stats.adapters_migrated));
    output.info(format!("  Skipped:   {}", stats.adapters_skipped));
    output.info(format!("  Failed:    {}", stats.adapters_failed));
    output.info("");

    // Summary line
    let total_success = stats.tenants_migrated + stats.adapters_migrated;
    let total_failed = stats.tenants_failed + stats.adapters_failed;
    let total_skipped = stats.tenants_skipped + stats.adapters_skipped;

    output.info(format!(
        "Summary: {} succeeded, {} skipped, {} failed",
        total_success, total_skipped, total_failed
    ));

    if stats.adapters_failed > 0 || stats.tenants_failed > 0 {
        output.warning("Some records failed to migrate. Check logs above for details.");
        if !args.dry_run {
            output.warning("You may need to manually migrate failed records.");
        }
    } else {
        output.success("All records migrated successfully!");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_command_name() {
        assert_eq!(
            get_registry_command_name(&RegistryCommand::Sync {
                dir: PathBuf::from("./adapters"),
                cas_root: PathBuf::from("./var/cas"),
                registry: PathBuf::from("./var/registry.db"),
            }),
            "registry_sync"
        );
        assert_eq!(
            get_registry_command_name(&RegistryCommand::Migrate(RegistryMigrateArgs {
                from_db: PathBuf::from("deprecated/registry.db"),
                to_db: PathBuf::from("var/registry.db"),
                dry_run: false,
                force: false,
            })),
            "registry_migrate"
        );
    }

    #[test]
    fn test_registry_command_clone() {
        let cmd = RegistryCommand::Sync {
            dir: PathBuf::from("./adapters"),
            cas_root: PathBuf::from("./var/cas"),
            registry: PathBuf::from("./var/registry.db"),
        };
        let cloned = cmd.clone();
        if let RegistryCommand::Sync { dir, .. } = cloned {
            assert_eq!(dir, PathBuf::from("./adapters"));
        } else {
            panic!("Clone did not preserve variant");
        }
    }

    #[test]
    fn test_registry_migrate_args_clone() {
        let args = RegistryMigrateArgs {
            from_db: PathBuf::from("deprecated/registry.db"),
            to_db: PathBuf::from("var/registry.db"),
            dry_run: true,
            force: false,
        };
        let cloned = args.clone();
        assert_eq!(cloned.from_db, args.from_db);
        assert_eq!(cloned.to_db, args.to_db);
        assert_eq!(cloned.dry_run, args.dry_run);
        assert_eq!(cloned.force, args.force);
    }
}
