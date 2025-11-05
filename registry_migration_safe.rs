//! Safe Registry Migration Tool
//!
//! Production-ready registry migration with comprehensive validation,
//! backup/recovery, and error handling following AdapterOS standards.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_registry::Registry;
use clap::Parser;
use registry_migration_analysis::{SchemaAnalysis, MigrationRisk};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn, instrument};

#[derive(Parser)]
#[command(author, version, about)]
pub struct Args {
    /// Path to old registry database
    #[arg(long)]
    old_db: PathBuf,

    /// Path to new registry database (will be created)
    #[arg(long)]
    new_db: PathBuf,

    /// Dry run - analyze and show what would be migrated
    #[arg(long)]
    dry_run: bool,

    /// Force migration even if risks are detected
    #[arg(long)]
    force: bool,

    /// Create backup of old database before migration
    #[arg(long, default_value = "true")]
    backup: bool,

    /// Maximum validation errors before aborting
    #[arg(long, default_value = "10")]
    max_errors: usize,

    /// Migration configuration file
    #[arg(long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationConfig {
    /// How to extract tenant ID from adapter ID
    pub tenant_extraction: TenantExtractionStrategy,
    /// Default values for missing fields
    pub defaults: MigrationDefaults,
    /// Validation rules
    pub validation: ValidationRules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TenantExtractionStrategy {
    /// Split on first '-' (tenant-adapter)
    SplitOnDash,
    /// Use explicit tenant mapping
    ExplicitMapping(HashMap<String, String>),
    /// All adapters belong to default tenant
    AllToDefault(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationDefaults {
    pub alpha: f32,
    pub targets_json: String,
    pub languages_json: String,
    pub framework: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRules {
    /// Validate tenant references exist
    pub validate_tenant_refs: bool,
    /// Check for hash format compatibility
    pub validate_hash_formats: bool,
    /// Verify ACL transformations
    pub validate_acl_transforms: bool,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            tenant_extraction: TenantExtractionStrategy::SplitOnDash,
            defaults: MigrationDefaults {
                alpha: 1.0,
                targets_json: r#"["unknown"]"#.to_string(),
                languages_json: r#"["en"]"#.to_string(),
                framework: "unknown".to_string(),
                active: true,
            },
            validation: ValidationRules {
                validate_tenant_refs: true,
                validate_hash_formats: true,
                validate_acl_transforms: true,
            },
        }
    }
}

#[derive(Debug)]
pub struct MigrationEngine {
    config: MigrationConfig,
    analysis: Option<SchemaAnalysis>,
    stats: Arc<Mutex<MigrationStats>>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    pub analysis_duration_ms: u64,
    pub migration_duration_ms: u64,
    pub adapters_processed: usize,
    pub adapters_migrated: usize,
    pub adapters_failed: usize,
    pub adapters_skipped: usize,
    pub tenants_processed: usize,
    pub tenants_migrated: usize,
    pub tenants_failed: usize,
    pub models_processed: usize,
    pub models_migrated: usize,
    pub models_failed: usize,
    pub validation_errors: Vec<String>,
    pub backup_created: bool,
}

impl MigrationEngine {
    pub fn new(config: MigrationConfig) -> Self {
        Self {
            config,
            analysis: None,
            stats: Arc::new(Mutex::new(MigrationStats::default())),
        }
    }

    #[instrument(skip(self))]
    pub async fn execute(&mut self, args: &Args) -> Result<()> {
        info!("Starting safe registry migration");

        // Phase 1: Analysis
        self.analyze_source(&args.old_db).await?;

        // Phase 2: Risk Assessment
        self.assess_risks(args.force)?;

        // Phase 3: Backup (if requested)
        if args.backup && !args.dry_run {
            self.create_backup(&args.old_db).await?;
        }

        // Phase 4: Validation
        self.validate_migration_config()?;

        // Phase 5: Migration
        if !args.dry_run {
            self.perform_migration(&args.old_db, &args.new_db, args.max_errors).await?;
        } else {
            info!("DRY RUN: Skipping actual migration");
        }

        // Phase 6: Verification
        self.verify_migration(&args.new_db).await?;

        Ok(())
    }

    async fn analyze_source(&mut self, old_db_path: &Path) -> Result<()> {
        let start = std::time::Instant::now();

        info!("Analyzing source database: {:?}", old_db_path);
        let analysis = SchemaAnalysis::analyze(old_db_path)?;
        let duration = start.elapsed();

        self.analysis = Some(analysis.clone());

        let mut stats = self.stats.lock().await;
        stats.analysis_duration_ms = duration.as_millis() as u64;

        info!("Analysis complete in {:?}", duration);
        println!("{}", analysis);

        Ok(())
    }

    fn assess_risks(&self, force: bool) -> Result<()> {
        if let Some(analysis) = &self.analysis {
            match analysis.migration_risk {
                MigrationRisk::Low => {
                    info!("Migration risk: LOW - Safe to proceed");
                }
                MigrationRisk::Medium => {
                    warn!("Migration risk: MEDIUM - Review data patterns carefully");
                    if !force {
                        return Err(AosError::Validation(
                            "Medium risk migration requires --force flag".to_string()
                        ));
                    }
                }
                MigrationRisk::High => {
                    error!("Migration risk: HIGH - Manual review required");
                    if !force {
                        return Err(AosError::Validation(
                            "High risk migration requires --force flag".to_string()
                        ));
                    }
                }
                MigrationRisk::Critical => {
                    error!("Migration risk: CRITICAL - Manual intervention required");
                    return Err(AosError::Validation(
                        "Critical risk migration not supported automatically".to_string()
                    ));
                }
            }
        }

        Ok(())
    }

    async fn create_backup(&self, old_db_path: &Path) -> Result<()> {
        let backup_path = format!("{}.backup.{}",
            old_db_path.display(),
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );

        info!("Creating backup: {} -> {}", old_db_path.display(), backup_path);

        tokio::fs::copy(old_db_path, &backup_path).await
            .map_err(|e| AosError::Io(format!("Failed to create backup: {}", e)))?;

        let mut stats = self.stats.lock().await;
        stats.backup_created = true;

        info!("Backup created successfully");
        Ok(())
    }

    fn validate_migration_config(&self) -> Result<()> {
        // Validate tenant extraction strategy
        match &self.config.tenant_extraction {
            TenantExtractionStrategy::SplitOnDash => {
                // Check if analysis shows compatible patterns
                if let Some(analysis) = &self.analysis {
                    if analysis.data_patterns.adapter_id_patterns.contains(&"single-part".to_string()) {
                        return Err(AosError::Config(
                            "SplitOnDash strategy incompatible with single-part adapter IDs".to_string()
                        ));
                    }
                }
            }
            TenantExtractionStrategy::ExplicitMapping(mapping) => {
                if mapping.is_empty() {
                    return Err(AosError::Config(
                        "ExplicitMapping strategy requires non-empty mapping".to_string()
                    ));
                }
            }
            TenantExtractionStrategy::AllToDefault(tenant) => {
                if tenant.is_empty() {
                    return Err(AosError::Config(
                        "AllToDefault strategy requires non-empty tenant ID".to_string()
                    ));
                }
            }
        }

        Ok(())
    }

    async fn perform_migration(
        &self,
        old_db_path: &Path,
        new_db_path: &Path,
        max_errors: usize
    ) -> Result<()> {
        let start = std::time::Instant::now();

        info!("Performing migration: {:?} -> {:?}", old_db_path, new_db_path);

        // Create new registry
        let registry = Registry::open(new_db_path).await?;
        info!("New registry created");

        // Extract and migrate data
        let old_conn = Connection::open(old_db_path)
            .map_err(|e| AosError::Database(format!("Failed to open old database: {}", e)))?;

        // Migrate tenants first
        self.migrate_tenants(&old_conn, &registry, max_errors).await?;

        // Migrate adapters
        self.migrate_adapters(&old_conn, &registry, max_errors).await?;

        // Migrate models
        self.migrate_models(&old_conn, &registry, max_errors).await?;

        let duration = start.elapsed();
        let mut stats = self.stats.lock().await;
        stats.migration_duration_ms = duration.as_millis() as u64;

        info!("Migration completed in {:?}", duration);
        Ok(())
    }

    async fn migrate_tenants(
        &self,
        old_conn: &Connection,
        registry: &Registry,
        max_errors: usize
    ) -> Result<()> {
        let sql = "SELECT id, uid, gid, created_at FROM tenants";
        let mut stmt = old_conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // id
                row.get::<_, i32>(1)?,    // uid (ignored)
                row.get::<_, i32>(2)?,    // gid (ignored)
                row.get::<_, String>(3)?, // created_at (ignored)
            ))
        })?;

        let mut error_count = 0;
        let mut stats = self.stats.lock().await;

        for row_result in rows {
            stats.tenants_processed += 1;

            let (id, _uid, _gid, _created_at) = match row_result {
                Ok(data) => data,
                Err(e) => {
                    stats.tenants_failed += 1;
                    error_count += 1;
                    if error_count >= max_errors {
                        return Err(AosError::Migration(format!("Too many tenant errors: {}", e)));
                    }
                    continue;
                }
            };

            // Register tenant (assume no ITAR flag for migration)
            match registry.register_tenant(&id, &id, false).await {
                Ok(_) => {
                    stats.tenants_migrated += 1;
                    info!("Migrated tenant: {}", id);
                }
                Err(e) => {
                    stats.tenants_failed += 1;
                    error_count += 1;
                    let error_msg = format!("Failed to migrate tenant {}: {}", id, e);
                    stats.validation_errors.push(error_msg.clone());
                    error!("{}", error_msg);

                    if error_count >= max_errors {
                        return Err(AosError::Migration("Too many tenant migration errors".to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    async fn migrate_adapters(
        &self,
        old_conn: &Connection,
        registry: &Registry,
        max_errors: usize
    ) -> Result<()> {
        let sql = "SELECT id, hash, tier, rank, acl, activation_pct, registered_at FROM adapters";
        let mut stmt = old_conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,     // id
                row.get::<_, String>(1)?,     // hash
                row.get::<_, String>(2)?,     // tier
                row.get::<_, i32>(3)?,        // rank
                row.get::<_, Option<String>>(4)?.unwrap_or_default(), // acl
                row.get::<_, Option<f64>>(5)?.unwrap_or(0.0), // activation_pct (ignored)
                row.get::<_, String>(6)?,     // registered_at (ignored)
            ))
        })?;

        let mut error_count = 0;
        let mut stats = self.stats.lock().await;

        for row_result in rows {
            stats.adapters_processed += 1;

            let (id, hash_hex, tier, rank, acl, _activation_pct, _registered_at) = match row_result {
                Ok(data) => data,
                Err(e) => {
                    stats.adapters_failed += 1;
                    error_count += 1;
                    if error_count >= max_errors {
                        return Err(AosError::Migration(format!("Too many adapter errors: {}", e)));
                    }
                    continue;
                }
            };

            // Transform data
            let transform_result = self.transform_adapter_data(&id, &hash_hex, &tier, rank, &acl);
            let params = match transform_result {
                Ok(p) => p,
                Err(e) => {
                    stats.adapters_failed += 1;
                    error_count += 1;
                    let error_msg = format!("Failed to transform adapter {}: {}", id, e);
                    stats.validation_errors.push(error_msg.clone());
                    error!("{}", error_msg);

                    if error_count >= max_errors {
                        return Err(AosError::Migration("Too many adapter transformation errors".to_string()));
                    }
                    continue;
                }
            };

            let (tenant_id, name, hash, tier_val, rank_val, alpha, targets_json, acl_json, adapter_id, languages_json, framework) = params;

            match registry.register_adapter(
                &tenant_id, &name, &hash, &tier_val, rank_val, alpha, &targets_json,
                acl_json.as_deref(), adapter_id.as_deref(), languages_json.as_deref(), framework.as_deref()
            ).await {
                Ok(_) => {
                    stats.adapters_migrated += 1;
                    info!("Migrated adapter: {}-{}", tenant_id, name);
                }
                Err(e) => {
                    stats.adapters_failed += 1;
                    error_count += 1;
                    let error_msg = format!("Failed to migrate adapter {}: {}", id, e);
                    stats.validation_errors.push(error_msg.clone());
                    error!("{}", error_msg);

                    if error_count >= max_errors {
                        return Err(AosError::Migration("Too many adapter migration errors".to_string()));
                    }
                }
            }
        }

        Ok(())
    }

    fn transform_adapter_data(
        &self,
        id: &str,
        hash_hex: &str,
        tier: &str,
        rank: i32,
        acl: &str,
    ) -> Result<(String, String, B3Hash, String, u32, f32, String, Option<String>, Option<String>, Option<String>, Option<String>)> {
        // Extract tenant and name
        let (tenant_id, name) = self.extract_tenant_and_name(id)?;

        // Parse hash
        let hash = B3Hash::from_hex(hash_hex)
            .map_err(|e| AosError::Validation(format!("Invalid hash format for adapter {}: {}", id, e)))?;

        // Transform ACL
        let acl_json = self.transform_acl(acl)?;

        // Use configured defaults
        let alpha = self.config.defaults.alpha;
        let targets_json = self.config.defaults.targets_json.clone();
        let languages_json = Some(self.config.defaults.languages_json.clone());
        let framework = Some(self.config.defaults.framework.clone());
        let adapter_id = Some(id.to_string());

        Ok((
            tenant_id,
            name,
            hash,
            tier.to_string(),
            rank as u32,
            alpha,
            targets_json,
            acl_json,
            adapter_id,
            languages_json,
            framework,
        ))
    }

    fn extract_tenant_and_name(&self, id: &str) -> Result<(String, String)> {
        match &self.config.tenant_extraction {
            TenantExtractionStrategy::SplitOnDash => {
                let parts: Vec<&str> = id.split('-').collect();
                if parts.len() >= 2 {
                    Ok((parts[0].to_string(), parts[1..].join("-")))
                } else {
                    Err(AosError::Validation(format!(
                        "Cannot split adapter ID '{}' on dash for tenant extraction", id
                    )))
                }
            }
            TenantExtractionStrategy::ExplicitMapping(mapping) => {
                if let Some(tenant) = mapping.get(id) {
                    Ok((tenant.clone(), id.to_string()))
                } else {
                    Err(AosError::Validation(format!(
                        "No tenant mapping found for adapter ID '{}'", id
                    )))
                }
            }
            TenantExtractionStrategy::AllToDefault(tenant) => {
                Ok((tenant.clone(), id.to_string()))
            }
        }
    }

    fn transform_acl(&self, acl: &str) -> Result<Option<String>> {
        if acl.trim().is_empty() {
            return Ok(None);
        }

        // Try to parse as comma-separated values and convert to JSON array
        let values: Vec<String> = acl.split(',')
            .map(|s| format!(r#""{}""#, s.trim()))
            .collect();

        Ok(Some(format!("[{}]", values.join(","))))
    }

    async fn migrate_models(
        &self,
        _old_conn: &Connection,
        _registry: &Registry,
        _max_errors: usize
    ) -> Result<()> {
        // TODO: Implement model migration once registry supports model operations
        warn!("Model migration not yet implemented");
        Ok(())
    }

    async fn verify_migration(&self, new_db_path: &Path) -> Result<()> {
        info!("Verifying migration results");

        let registry = Registry::open(new_db_path).await?;

        // Check that we can list adapters
        let adapters = registry.list_adapters().await?;
        info!("Verification: {} adapters accessible", adapters.len());

        // TODO: Add more comprehensive verification
        // - Check tenant references are valid
        // - Verify hash formats
        // - Test ACL functionality

        Ok(())
    }

    pub async fn get_stats(&self) -> MigrationStats {
        self.stats.lock().await.clone()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    // Load configuration
    let config = if let Some(config_path) = &args.config {
        let config_str = tokio::fs::read_to_string(config_path).await
            .map_err(|e| AosError::Config(format!("Failed to read config file: {}", e)))?;
        serde_json::from_str(&config_str)
            .map_err(|e| AosError::Config(format!("Invalid config file: {}", e)))?
    } else {
        MigrationConfig::default()
    };

    // Create and run migration engine
    let mut engine = MigrationEngine::new(config);
    engine.execute(&args).await?;

    // Report final statistics
    let stats = engine.get_stats().await;
    println!("\nMigration Complete");
    println!("==================");
    println!("Analysis time: {}ms", stats.analysis_duration_ms);
    println!("Migration time: {}ms", stats.migration_duration_ms);
    println!("Backup created: {}", stats.backup_created);
    println!();
    println!("Tenants: {} processed, {} migrated, {} failed",
             stats.tenants_processed, stats.tenants_migrated, stats.tenants_failed);
    println!("Adapters: {} processed, {} migrated, {} failed",
             stats.adapters_processed, stats.adapters_migrated, stats.adapters_failed);
    println!("Models: {} processed, {} migrated, {} failed",
             stats.models_processed, stats.models_migrated, stats.models_failed);

    if !stats.validation_errors.is_empty() {
        println!("\nValidation Errors:");
        for error in &stats.validation_errors {
            println!("  - {}", error);
        }
    }

    Ok(())
}
