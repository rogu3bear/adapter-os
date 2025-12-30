//! Database management commands

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::output::OutputWriter;
use adapteros_core::policy::DriftPolicy;
use adapteros_db::{
    adapters::AdapterRegistrationBuilder,
    chat_sessions::{AddMessageParams, CreateChatSessionParams},
    migration_verify::MigrationVerifier,
    models::ModelRegistrationBuilder,
    policies::TenantPolicies,
    sqlx,
    users::Role,
    Db,
};
use argon2::{
    password_hash::{PasswordHasher, SaltString},
    Algorithm as Argon2Algorithm, Argon2, Params, Version,
};
use blake3;
use rand::rngs::OsRng;
use serde_json::json;

// Match server auth hashing parameters for deterministic fixture users
const ARGON2_MEMORY_KIB: u32 = 64 * 1024; // 64 MiB
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_PARALLELISM: u32 = 1;

#[derive(Debug, Clone, Subcommand)]
pub enum DbCommand {
    /// Run database migrations
    #[command(after_help = r#"Examples:
  # Run migrations on default database
  aosctl db migrate

  # Run migrations on custom database
  aosctl db migrate --db-path ./var/custom.db

  # Verify signatures only (don't run migrations)
  aosctl db migrate --verify-only
"#)]
    Migrate {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Verify signatures only (don't run migrations)
        #[arg(long)]
        verify_only: bool,
    },

    /// Clear a stuck migration lock and reset WAL/shm files
    #[command(after_help = r#"Examples:
  # Unlock default database
  aosctl db unlock

  # Unlock custom database
  aosctl db unlock --db-path ./var/custom.db
"#)]
    Unlock {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,
    },

    /// Reset database (DEVELOPMENT ONLY - destroys all data)
    #[command(after_help = r#"Examples:
  # Reset default database
  aosctl db reset

  # Reset custom database
  aosctl db reset --db-path ./var/custom.db

  # Skip confirmation prompt (dangerous!)
  aosctl db reset --force

WARNING: This command DELETES the database file and recreates it with all migrations.
         All data will be PERMANENTLY LOST. Only use in development environments.
"#)]
    Reset {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },

    /// Reset and seed deterministic test fixtures (development only)
    #[command(after_help = r#"Examples:
  # Reset DB and seed deterministic fixtures
  aosctl db seed-fixtures

  # Seed without dropping existing DB
  aosctl db seed-fixtures --skip-reset

  # Seed without chat history
  aosctl db seed-fixtures --skip-reset --no-chat
"#)]
    SeedFixtures {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Skip removing the database file before seeding
        #[arg(long)]
        skip_reset: bool,

        /// Include a starter chat session + single message
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        chat: bool,
    },

    /// Health check for migration signatures and DB integrity
    Health {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Emit JSON instead of human-readable output
        #[arg(long)]
        json: bool,
    },

    /// Verify seeded demo fixtures exist (development only)
    #[command(after_help = r#"Examples:
  # Verify default demo seed (tenant-test)
  aosctl db verify-seed

  # Verify custom database
  aosctl db verify-seed --db-path ./var/custom.db

  # Verify a different tenant id
  aosctl db verify-seed --tenant-id tenant-test
"#)]
    VerifySeed {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Tenant to verify (defaults to tenant-test)
        #[arg(long, default_value = "tenant-test")]
        tenant_id: String,
    },

    /// Validate and repair system bootstrap state
    #[command(after_help = r#"Examples:
  # Check bootstrap state (dry-run)
  aosctl db repair-bootstrap --dry-run

  # Repair bootstrap state if needed
  aosctl db repair-bootstrap

  # Check/repair custom database
  aosctl db repair-bootstrap --db-path ./var/custom.db

This command validates that the system tenant and core policies are properly
seeded. This is important for fresh installs or when KV and SQL stores may
be out of sync.

Issues detected:
- Missing system tenant
- Missing core policies (egress, determinism, isolation, evidence)
- KV/SQL inconsistency for system tenant
"#)]
    RepairBootstrap {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Check only, don't repair
        #[arg(long)]
        dry_run: bool,

        /// Output JSON instead of human-readable
        #[arg(long)]
        json: bool,
    },
}

/// Handle database commands
pub async fn handle_db_command(cmd: DbCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DbCommand::Migrate {
            db_path,
            verify_only,
        } => run_migrate(db_path, verify_only, output).await,
        DbCommand::Unlock { db_path } => run_unlock(db_path, output).await,
        DbCommand::Reset { db_path, force } => run_reset(db_path, force, output).await,
        DbCommand::SeedFixtures {
            db_path,
            skip_reset,
            chat,
        } => run_seed_fixtures(db_path, skip_reset, chat, output).await,
        DbCommand::Health { db_path, json } => run_health(db_path, json, output).await,
        DbCommand::VerifySeed { db_path, tenant_id } => {
            run_verify_seed(db_path, &tenant_id, output).await
        }
        DbCommand::RepairBootstrap {
            db_path,
            dry_run,
            json,
        } => run_repair_bootstrap(db_path, dry_run, json, output).await,
    }
}

async fn run_verify_seed(
    db_path: Option<PathBuf>,
    tenant_id: &str,
    output: &OutputWriter,
) -> Result<()> {
    // Resolve DB URL (matches migrate/reset behavior)
    let db_url = if let Some(path) = db_path.clone() {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    let db = Db::connect(&db_url).await?;
    let pool = db.pool();

    let models_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE tenant_id = ?")
        .bind(tenant_id)
        .fetch_one(pool)
        .await?;

    let repos_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM adapter_repositories WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_one(pool)
            .await?;

    let versions_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM adapter_versions WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_one(pool)
            .await?;

    let training_jobs_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM repository_training_jobs WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_one(pool)
            .await?;

    let stacks_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM adapter_stacks WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_one(pool)
            .await?;

    let mut failures = Vec::new();
    if models_count < 1 {
        failures.push(">=1 model");
    }
    if repos_count < 1 {
        failures.push(">=1 repo");
    }
    if versions_count < 1 {
        failures.push(">=1 adapter version");
    }
    if training_jobs_count < 1 {
        failures.push(">=1 training job");
    }
    if stacks_count < 1 {
        failures.push(">=1 stack");
    }

    if !failures.is_empty() {
        output.error("Seed verification failed");
        output.kv("DB", &db_url);
        output.kv("Tenant", tenant_id);
        output.kv("Models", &models_count.to_string());
        output.kv("Repos", &repos_count.to_string());
        output.kv("Adapter versions", &versions_count.to_string());
        output.kv("Training jobs", &training_jobs_count.to_string());
        output.kv("Stacks", &stacks_count.to_string());
        anyhow::bail!("Missing required seeded entities: {}", failures.join(", "));
    }

    output.success("Seed verification passed");
    output.kv("DB", &db_url);
    output.kv("Tenant", tenant_id);
    output.kv("Models", &models_count.to_string());
    output.kv("Repos", &repos_count.to_string());
    output.kv("Adapter versions", &versions_count.to_string());
    output.kv("Training jobs", &training_jobs_count.to_string());
    output.kv("Stacks", &stacks_count.to_string());

    Ok(())
}

async fn run_migrate(
    db_path: Option<PathBuf>,
    verify_only: bool,
    output: &OutputWriter,
) -> Result<()> {
    use adapteros_db::Db;

    // Determine database path
    let db_url = if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    if verify_only {
        output.info("Verifying migration signatures...");

        // Just verify signatures without running migrations
        // Use CARGO_MANIFEST_DIR to find migrations relative to workspace root
        use adapteros_db::migration_verify::MigrationVerifier;
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .ok_or_else(|| anyhow::anyhow!("Failed to find workspace root"))?;
        let migrations_path = workspace_root.join("migrations");

        if !migrations_path.exists() {
            anyhow::bail!(
                "Migrations directory not found: {}",
                migrations_path.display()
            );
        }

        let verifier = MigrationVerifier::new(&migrations_path)?;
        verifier.verify_all()?;

        output.success("All migration signatures verified");
        return Ok(());
    }

    output.info(format!("Running database migrations on: {}", db_url));

    // Connect to database and run migrations
    let db = Db::connect(&db_url).await?;
    db.migrate().await?;

    output.success("Database migrations completed successfully");
    Ok(())
}

async fn run_health(db_path: Option<PathBuf>, json: bool, output: &OutputWriter) -> Result<()> {
    // Resolve DB URL (matches migrate/reset behavior)
    let db_url = if let Some(path) = db_path.clone() {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    // Verify migration signatures (stale or missing signatures fail fast)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| anyhow::anyhow!("Failed to find workspace root"))?;
    let migrations_path = workspace_root.join("migrations");

    if !migrations_path.exists() {
        anyhow::bail!(
            "Migrations directory not found at {}",
            migrations_path.display()
        );
    }

    let verifier = MigrationVerifier::new(&migrations_path)?;
    verifier.verify_all()?;

    // Connectivity + integrity check
    let db = Db::connect(&db_url).await?;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(db.pool())
        .await?;

    let integrity_result = integrity.trim();
    let status = if integrity_result == "ok" {
        "healthy"
    } else {
        "unhealthy"
    };

    let payload = json!({
        "status": status,
        "integrity_check": integrity_result,
        "db_url": db_url,
    });

    if json {
        output.json(&payload)?;
    } else {
        output.info(format!(
            "Database URL: {}",
            payload["db_url"].as_str().unwrap_or_default()
        ));
        output.info(format!("Integrity check result: {}", integrity_result));
        output.success("Migration signatures verified");
    }

    if status != "healthy" {
        anyhow::bail!("Database integrity check failed: {}", integrity_result);
    }

    Ok(())
}

async fn run_unlock(db_path: Option<PathBuf>, output: &OutputWriter) -> Result<()> {
    use adapteros_db::Db;

    let db_url = if let Some(path) = db_path.clone() {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    let db_file_path = if let Some(path) = db_path.clone() {
        Some(path)
    } else if db_url.starts_with("sqlite://") {
        Some(PathBuf::from(db_url.trim_start_matches("sqlite://")))
    } else if db_url.starts_with("file:") {
        Some(PathBuf::from(db_url.trim_start_matches("file:")))
    } else {
        None
    };

    output.info(format!("Clearing migration locks on: {}", db_url));

    let db = Db::connect(&db_url).await?;
    let pool = db.pool();

    let has_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;

    if has_table > 0 {
        let cleared =
            sqlx::query("DELETE FROM _sqlx_migrations WHERE success = 0 OR success IS NULL")
                .execute(pool)
                .await?
                .rows_affected();
        output.info(format!("Removed {} dirty migration rows", cleared));

        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(pool)
            .await;
    } else {
        output.info("No _sqlx_migrations table found; nothing to unlock");
    }

    drop(db);

    if let Some(path) = db_file_path {
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let wal_path = path.with_file_name(format!("{}-wal", file_name));
        let shm_path = path.with_file_name(format!("{}-shm", file_name));

        for p in [wal_path, shm_path] {
            if p.exists() {
                match std::fs::remove_file(&p) {
                    Ok(_) => output.info(format!("Removed {}", p.display())),
                    Err(e) => output.warning(format!(
                        "Failed to remove {}: {} (close other processes and retry)",
                        p.display(),
                        e
                    )),
                }
            }
        }
    }

    output.success("Migration lock cleared; retry with `aosctl db migrate`");
    Ok(())
}

async fn run_reset(db_path: Option<PathBuf>, force: bool, output: &OutputWriter) -> Result<()> {
    use adapteros_db::Db;

    // Determine database path
    let db_file_path = if let Some(path) = db_path.clone() {
        path
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        // Extract path from sqlite:// URL
        PathBuf::from(url.trim_start_matches("sqlite://"))
    } else {
        PathBuf::from("./var/aos-cp.sqlite3")
    };

    // Safety check: ensure this is not being run in production
    let db_path_str = db_file_path.display().to_string();
    if db_path_str.contains("/prod") || db_path_str.contains("production") {
        output.error("Refusing to reset database with 'prod' or 'production' in path");
        output.error("This command is for DEVELOPMENT ONLY");
        return Err(anyhow::anyhow!("Cannot reset production database"));
    }

    // Confirmation prompt (unless --force)
    if !force {
        output.warning(format!("This will DELETE the database at: {}", db_path_str));
        output.warning("ALL DATA WILL BE PERMANENTLY LOST");
        output.info("");
        output.info("Type 'yes' to confirm: ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim() != "yes" {
            output.info("Reset cancelled");
            return Ok(());
        }
    }

    output.info("Resetting database...");

    // Step 1: Delete database file and WAL files
    if db_file_path.exists() {
        std::fs::remove_file(&db_file_path)?;
        output.info(format!("Deleted {}", db_path_str));
    }

    // Also remove WAL and SHM files if they exist
    let wal_path = db_file_path.with_extension("sqlite3-wal");
    let shm_path = db_file_path.with_extension("sqlite3-shm");

    if wal_path.exists() {
        std::fs::remove_file(&wal_path)?;
        output.info("Deleted WAL file");
    }

    if shm_path.exists() {
        std::fs::remove_file(&shm_path)?;
        output.info("Deleted SHM file");
    }

    // Step 2: Recreate database with all migrations
    output.info("Creating fresh database...");

    let db_url = if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    let db = Db::connect(&db_url).await?;
    db.migrate().await?;

    output.success("Database reset complete");
    output.info("All migrations (0001-0070) applied successfully");

    Ok(())
}

fn hash_fixture_password(password: &str) -> Result<String> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_PARALLELISM,
        None,
    )
    .map_err(|e| anyhow::anyhow!("invalid Argon2 params for seed fixtures: {}", e))?;

    let argon2 = Argon2::new(Argon2Algorithm::Argon2id, Version::V0x13, params);
    let salt = SaltString::generate(&mut OsRng);

    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|e| anyhow::anyhow!("failed to hash seed fixture password: {}", e))
}

async fn run_seed_fixtures(
    db_path: Option<PathBuf>,
    skip_reset: bool,
    chat: bool,
    output: &OutputWriter,
) -> Result<()> {
    // Deterministic fixtures to keep Cypress happy
    const TENANT_ID: &str = "tenant-test";
    const TENANT_NAME: &str = "Test Tenant";
    const MODEL_ID: &str = "model-qwen-test";
    const MODEL_NAME: &str = "qwen2.5-7b-test";
    const STACK_ID: &str = "stack-test";
    const STACK_NAME: &str = "stack.test";
    const ADAPTER_ID: &str = "adapter-test";
    const ADAPTER_NAME: &str = "Test Adapter";
    const E2E_USER_ID: &str = "user-e2e";
    const E2E_USER_EMAIL: &str = "test@example.com";
    const E2E_USER_NAME: &str = "E2E Test User";
    const E2E_USER_PASSWORD: &str = "password";
    const POLICY_ID: &str = "policy-e2e-default";
    const REPO_ID: &str = "repo-e2e";
    const REPO_NAME: &str = "e2e-repo";
    const VERSION_ID: &str = "adapter-version-e2e";
    const VERSION_LABEL: &str = "1.0.0";
    const VERSION_BRANCH: &str = "main";
    const VERSION_BRANCH_CLASS: &str = "protected";
    const VERSION_HISTORY_ID: &str = "avh-e2e";
    const GIT_REPO_ROW_ID: &str = "git-repo-e2e";
    const GIT_REPO_PATH: &str = "./var/demo_repos/e2e-repo";
    const TRAINING_JOB_ID: &str = "training-job-e2e";
    const CHAT_SESSION_ID: &str = "chat-session-test";
    const CHAT_MESSAGE_ID: &str = "chat-message-test";
    const FIXED_TS: &str = "2025-01-01T00:00:00Z";

    // Resolve database path
    let db_file_path = if let Some(path) = db_path.clone() {
        path
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        PathBuf::from(url.trim_start_matches("sqlite://"))
    } else {
        PathBuf::from("./var/aos-cp.sqlite3")
    };

    // Optionally reset DB before seeding
    if !skip_reset && db_file_path.exists() {
        std::fs::remove_file(&db_file_path)?;
        let wal_path = db_file_path.with_extension("sqlite3-wal");
        let shm_path = db_file_path.with_extension("sqlite3-shm");
        if wal_path.exists() {
            std::fs::remove_file(&wal_path).ok();
        }
        if shm_path.exists() {
            std::fs::remove_file(&shm_path).ok();
        }
        output.info(format!(
            "Removed existing database at {}",
            db_file_path.display()
        ));
    }

    let db_url = if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    output.info(format!("Seeding deterministic fixtures into {}", db_url));

    let db = Db::connect(&db_url).await?;
    db.migrate().await?;
    db.ensure_system_tenant().await?;

    let pool = db.pool();

    // Upsert tenant
    let pinned = serde_json::to_string(&vec![ADAPTER_ID])?;
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, itar_flag, status, created_at, updated_at, default_stack_id, default_pinned_adapter_ids, determinism_mode)
        VALUES (?, ?, 0, 'active', ?, ?, ?, ?, 'strict')
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            status = excluded.status,
            updated_at = excluded.updated_at,
            default_stack_id = excluded.default_stack_id,
            default_pinned_adapter_ids = excluded.default_pinned_adapter_ids
        "#,
    )
    .bind(TENANT_ID)
    .bind(TENANT_NAME)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(STACK_ID)
    .bind(&pinned)
    .execute(pool)
    .await?;

    // Seed deterministic admin user for Cypress flows
    sqlx::query("DELETE FROM users WHERE email = ? OR id = ?")
        .bind(E2E_USER_EMAIL)
        .bind(E2E_USER_ID)
        .execute(pool)
        .await?;

    let user_pw_hash = hash_fixture_password(E2E_USER_PASSWORD)?;
    let inserted_user_id = db
        .create_user(
            E2E_USER_EMAIL,
            E2E_USER_NAME,
            &user_pw_hash,
            Role::Admin,
            TENANT_ID,
        )
        .await?;

    if inserted_user_id != E2E_USER_ID {
        sqlx::query("UPDATE users SET id = ? WHERE id = ?")
            .bind(E2E_USER_ID)
            .bind(&inserted_user_id)
            .execute(pool)
            .await?;
    }

    sqlx::query(
        r#"
        UPDATE users SET
            tenant_id = ?,
            role = 'admin',
            display_name = ?,
            pw_hash = ?,
            disabled = 0,
            created_at = ?
        WHERE id = ?
        "#,
    )
    .bind(TENANT_ID)
    .bind(E2E_USER_NAME)
    .bind(&user_pw_hash)
    .bind(FIXED_TS)
    .bind(E2E_USER_ID)
    .execute(pool)
    .await?;

    // Register model (then normalize fields to deterministic values)
    let model_params = ModelRegistrationBuilder::new()
        .name(MODEL_NAME)
        .hash_b3("b3_model_qwen25_7b_test")
        .config_hash_b3("b3_model_config_test")
        .tokenizer_hash_b3("b3_model_tokenizer_test")
        .tokenizer_cfg_hash_b3("b3_model_tokenizer_cfg_test")
        .license_hash_b3(Some("b3_license_test"))
        .metadata_json(Some(
            json!({"size_bytes": 1024_i64, "quant": "q4_0"}).to_string(),
        ))
        .build()?;

    let inserted_model_id = db.register_model(model_params).await?;
    if inserted_model_id != MODEL_ID {
        sqlx::query("UPDATE models SET id = ? WHERE id = ?")
            .bind(MODEL_ID)
            .bind(&inserted_model_id)
            .execute(pool)
            .await?;
    }

    sqlx::query(
        r#"
        UPDATE models SET
            tenant_id = ?,
            backend = 'coreml',
            quantization = 'q4_0',
            format = 'safetensors',
            size_bytes = 1024,
            import_status = 'available',
            imported_at = ?,
            imported_by = 'seed-fixtures',
            updated_at = ?
        WHERE id = ?
        "#,
    )
    .bind(TENANT_ID)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(MODEL_ID)
    .execute(pool)
    .await?;

    // Register adapter with deterministic ID
    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(TENANT_ID)
        .adapter_id(ADAPTER_ID)
        .name(ADAPTER_NAME)
        .hash_b3("b3_adapter_seed_test")
        .rank(8)
        .tier("warm")
        .alpha(16.0)
        .lora_strength(Some(1.0))
        .targets_json(r#"["attn.q_proj","attn.v_proj"]"#)
        .category("code")
        .scope("global")
        .base_model_id(Some(MODEL_ID))
        .manifest_schema_version(Some("1.0.0"))
        .content_hash_b3(Some("b3_adapter_content_seed"))
        .metadata_json(Some(
            json!({"description": "Seed adapter for Cypress", "owner": "seed-fixtures"})
                .to_string(),
        ))
        .build()?;

    let inserted_adapter_id = db.register_adapter_extended(adapter_params).await?;
    if inserted_adapter_id != ADAPTER_ID {
        sqlx::query("UPDATE adapters SET id = ? WHERE id = ?")
            .bind(ADAPTER_ID)
            .bind(&inserted_adapter_id)
            .execute(pool)
            .await?;
    }

    // Seed stack referencing the adapter
    sqlx::query(
        r#"
        INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, determinism_mode, routing_determinism_mode, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, 'Parallel', '1.0.0', 'active', 'strict', 'deterministic', ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            adapter_ids_json = excluded.adapter_ids_json,
            updated_at = excluded.updated_at,
            determinism_mode = excluded.determinism_mode,
            routing_determinism_mode = excluded.routing_determinism_mode
        "#,
    )
    .bind(STACK_ID)
    .bind(TENANT_ID)
    .bind(STACK_NAME)
    .bind("Seed stack for Cypress chat flow")
    .bind(serde_json::to_string(&vec![ADAPTER_ID])?)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .execute(pool)
    .await?;

    // Ensure tenant default stack + pins are aligned with seeded stack/adapter
    sqlx::query(
        "UPDATE tenants SET default_stack_id = ?, default_pinned_adapter_ids = ? WHERE id = ?",
    )
    .bind(STACK_ID)
    .bind(&pinned)
    .bind(TENANT_ID)
    .execute(pool)
    .await?;

    // Seed deterministic tenant policy
    sqlx::query("UPDATE policies SET active = 0 WHERE tenant_id = ?")
        .bind(TENANT_ID)
        .execute(pool)
        .await?;

    let policy_body = serde_json::to_string(&TenantPolicies {
        drift: DriftPolicy::default(),
    })?;
    let policy_hash = blake3::hash(policy_body.as_bytes()).to_hex().to_string();

    sqlx::query("INSERT INTO policies (id, tenant_id, hash_b3, body_json, active, created_at) VALUES (?, ?, ?, ?, 1, ?) ON CONFLICT(id) DO UPDATE SET hash_b3 = excluded.hash_b3, body_json = excluded.body_json, active = excluded.active, created_at = excluded.created_at")
        .bind(POLICY_ID)
        .bind(TENANT_ID)
        .bind(&policy_hash)
        .bind(&policy_body)
        .bind(FIXED_TS)
        .execute(pool)
        .await?;

    // Seed deterministic adapter repository + version
    sqlx::query("DELETE FROM adapter_version_runtime_state WHERE version_id = ?")
        .bind(VERSION_ID)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM adapter_versions WHERE id = ? OR repo_id = ?")
        .bind(VERSION_ID)
        .bind(REPO_ID)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM adapter_repositories WHERE id = ?")
        .bind(REPO_ID)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO adapter_repositories (id, tenant_id, name, base_model_id, default_branch, archived, created_by, created_at, description)
        VALUES (?, ?, ?, ?, 'main', 0, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            tenant_id = excluded.tenant_id,
            name = excluded.name,
            base_model_id = excluded.base_model_id,
            default_branch = excluded.default_branch,
            archived = excluded.archived,
            created_by = excluded.created_by,
            created_at = excluded.created_at,
            description = excluded.description
        "#,
    )
    .bind(REPO_ID)
    .bind(TENANT_ID)
    .bind(REPO_NAME)
    .bind(MODEL_ID)
    .bind(E2E_USER_ID)
    .bind(FIXED_TS)
    .bind("Seeded E2E repository")
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO adapter_versions (
            id, repo_id, tenant_id, version, branch, branch_classification, aos_path, aos_hash,
            manifest_schema_version, parent_version_id, code_commit_sha, data_spec_hash,
            training_backend, coreml_used, coreml_device_type, adapter_trust_state, release_state,
            metrics_snapshot_id, evaluation_summary, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, NULL, ?, NULL, ?, 1, NULL, 'allowed', 'ready', NULL, 'seeded e2e version', ?)
        ON CONFLICT(id) DO UPDATE SET
            repo_id = excluded.repo_id,
            tenant_id = excluded.tenant_id,
            version = excluded.version,
            branch = excluded.branch,
            branch_classification = excluded.branch_classification,
            aos_hash = excluded.aos_hash,
            manifest_schema_version = excluded.manifest_schema_version,
            code_commit_sha = excluded.code_commit_sha,
            training_backend = excluded.training_backend,
            adapter_trust_state = excluded.adapter_trust_state,
            release_state = excluded.release_state,
            evaluation_summary = excluded.evaluation_summary,
            created_at = excluded.created_at
        "#,
    )
    .bind(VERSION_ID)
    .bind(REPO_ID)
    .bind(TENANT_ID)
    .bind(VERSION_LABEL)
    .bind(VERSION_BRANCH)
    .bind(VERSION_BRANCH_CLASS)
    .bind("b3_adapter_version_seed")
    .bind("1.0.0")
    .bind("deadbeef")
    .bind("coreml")
    .bind(FIXED_TS)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO adapter_version_runtime_state (version_id, runtime_state, updated_at, worker_id, last_error)
        VALUES (?, 'unloaded', ?, NULL, NULL)
        ON CONFLICT(version_id) DO UPDATE SET
            runtime_state = excluded.runtime_state,
            updated_at = excluded.updated_at,
            worker_id = excluded.worker_id,
            last_error = excluded.last_error
        "#,
    )
    .bind(VERSION_ID)
    .bind(FIXED_TS)
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO adapter_version_history (
            id, repo_id, tenant_id, version_id, branch, old_state, new_state, actor, reason, train_job_id, created_at
        )
        VALUES (?, ?, ?, ?, ?, NULL, 'ready', ?, 'seed-fixtures', NULL, ?)
        ON CONFLICT(id) DO UPDATE SET
            repo_id = excluded.repo_id,
            tenant_id = excluded.tenant_id,
            version_id = excluded.version_id,
            branch = excluded.branch,
            old_state = excluded.old_state,
            new_state = excluded.new_state,
            actor = excluded.actor,
            reason = excluded.reason,
            train_job_id = excluded.train_job_id,
            created_at = excluded.created_at
        "#,
    )
    .bind(VERSION_HISTORY_ID)
    .bind(REPO_ID)
    .bind(TENANT_ID)
    .bind(VERSION_ID)
    .bind(VERSION_BRANCH)
    .bind(E2E_USER_ID)
    .bind(FIXED_TS)
    .execute(pool)
    .await?;

    // Seed a deterministic git repository row (required FK for repository_training_jobs.repo_id)
    let git_analysis_json = json!({
        "summary": "Seeded git repository for demo training jobs",
        "languages": [{"name": "Rust", "files": 1, "lines": 42, "percentage": 100.0}],
        "frameworks": [],
    })
    .to_string();
    let git_evidence_json = json!([]).to_string();
    let git_security_scan_json = json!({"status": "ok", "violations": []}).to_string();

    sqlx::query(
        r#"
        INSERT INTO git_repositories (
            id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_at, created_by
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(repo_id) DO UPDATE SET
            id = excluded.id,
            path = excluded.path,
            branch = excluded.branch,
            analysis_json = excluded.analysis_json,
            evidence_json = excluded.evidence_json,
            security_scan_json = excluded.security_scan_json,
            status = excluded.status,
            created_at = excluded.created_at,
            created_by = excluded.created_by
        "#,
    )
    .bind(GIT_REPO_ROW_ID)
    .bind(REPO_ID)
    .bind(GIT_REPO_PATH)
    .bind(VERSION_BRANCH)
    .bind(&git_analysis_json)
    .bind(&git_evidence_json)
    .bind(&git_security_scan_json)
    .bind("ready")
    .bind(FIXED_TS)
    .bind(E2E_USER_ID)
    .execute(pool)
    .await?;

    // Seed a deterministic training job (so Training UI + lineage have at least one job)
    let training_config_json =
        r#"{"rank":8,"alpha":16,"epochs":1,"learning_rate":0.0005,"batch_size":4}"#;
    let training_config_hash = blake3::hash(training_config_json.as_bytes())
        .to_hex()
        .to_string();
    let progress_json = json!({
        "progress_pct": 100.0,
        "current_epoch": 1,
        "total_epochs": 1,
        "current_loss": 0.0,
        "learning_rate": 0.0005,
        "tokens_per_second": 10.0,
        "error_message": null
    })
    .to_string();

    sqlx::query(
        r#"
        INSERT INTO repository_training_jobs (
            id, repo_id, training_config_json, status, progress_json, created_by,
            adapter_name, tenant_id, adapter_id, base_model_id, stack_id,
            created_at, started_at, completed_at, config_hash_b3,
            synthetic_mode, data_lineage_mode, produced_version_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 'synthetic', ?)
        ON CONFLICT(id) DO UPDATE SET
            repo_id = excluded.repo_id,
            training_config_json = excluded.training_config_json,
            status = excluded.status,
            progress_json = excluded.progress_json,
            created_by = excluded.created_by,
            adapter_name = excluded.adapter_name,
            tenant_id = excluded.tenant_id,
            adapter_id = excluded.adapter_id,
            base_model_id = excluded.base_model_id,
            stack_id = excluded.stack_id,
            created_at = excluded.created_at,
            started_at = excluded.started_at,
            completed_at = excluded.completed_at,
            config_hash_b3 = excluded.config_hash_b3,
            data_lineage_mode = excluded.data_lineage_mode,
            produced_version_id = excluded.produced_version_id
        "#,
    )
    .bind(TRAINING_JOB_ID)
    .bind(REPO_ID)
    .bind(training_config_json)
    .bind("completed")
    .bind(&progress_json)
    .bind(E2E_USER_ID)
    .bind(ADAPTER_NAME)
    .bind(TENANT_ID)
    .bind(ADAPTER_ID)
    .bind(MODEL_ID)
    .bind(STACK_ID)
    .bind(FIXED_TS)
    .bind(FIXED_TS)
    .bind(Some(FIXED_TS))
    .bind(&training_config_hash)
    .bind(VERSION_ID)
    .execute(pool)
    .await?;

    // Link the seeded adapter to the seeded training job for UI lineage surfaces.
    sqlx::query("UPDATE adapters SET training_job_id = ? WHERE tenant_id = ? AND id = ?")
        .bind(TRAINING_JOB_ID)
        .bind(TENANT_ID)
        .bind(ADAPTER_ID)
        .execute(pool)
        .await?;

    // Optionally seed a starter chat session + message for assertions
    if chat {
        // Clear any prior seeded session/messages to keep the command idempotent
        sqlx::query("DELETE FROM chat_messages WHERE session_id = ? OR id = ?")
            .bind(CHAT_SESSION_ID)
            .bind(CHAT_MESSAGE_ID)
            .execute(pool)
            .await?;
        sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
            .bind(CHAT_SESSION_ID)
            .execute(pool)
            .await?;

        let session_params = CreateChatSessionParams {
            id: CHAT_SESSION_ID.to_string(),
            tenant_id: TENANT_ID.to_string(),
            user_id: None,
            created_by: Some("seed-fixtures".to_string()),
            stack_id: Some(STACK_ID.to_string()),
            collection_id: None,
            document_id: None,
            name: "Seeded Cypress Session".to_string(),
            title: Some("Seeded Cypress Session".to_string()),
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: Some(
                json!({"chat_session_config": {"stack_id": STACK_ID, "routing_determinism_mode": "deterministic"}})
                    .to_string(),
            ),
            tags_json: None,
            pinned_adapter_ids: Some(pinned.clone()),
            codebase_adapter_id: None,
        };
        db.create_chat_session(session_params).await?;

        let message_params = AddMessageParams {
            id: CHAT_MESSAGE_ID.to_string(),
            session_id: CHAT_SESSION_ID.to_string(),
            tenant_id: Some(TENANT_ID.to_string()),
            role: "user".to_string(),
            content: "Hello from seeded fixtures".to_string(),
            sequence: Some(1),
            created_at: Some(FIXED_TS.to_string()),
            metadata_json: Some(
                json!({"routerDecision": {"adapter_ids": [ADAPTER_ID], "stack_id": STACK_ID}})
                    .to_string(),
            ),
        };
        db.add_chat_message(message_params).await?;
    }

    output.success("Seed fixtures ready");
    output.kv("Tenant", TENANT_ID);
    output.kv("Model", MODEL_ID);
    output.kv("Adapter", ADAPTER_ID);
    output.kv("Stack", STACK_ID);
    output.kv("Adapter repo", REPO_ID);
    output.kv("Adapter version", VERSION_ID);
    output.kv("Training job", TRAINING_JOB_ID);
    output.kv("Policy", POLICY_ID);
    output.kv("User", E2E_USER_ID);
    if chat {
        output.kv("Chat session", CHAT_SESSION_ID);
    }

    Ok(())
}

/// Validate and optionally repair system bootstrap state
async fn run_repair_bootstrap(
    db_path: Option<PathBuf>,
    dry_run: bool,
    json_output: bool,
    output: &OutputWriter,
) -> Result<()> {
    // Resolve DB URL
    let db_url = if let Some(path) = db_path.clone() {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    let db = Db::connect(&db_url).await?;

    if !json_output {
        output.info(format!("Validating bootstrap state for {}", db_url));
    }

    // Validate current state
    let status = db.validate_bootstrap_state().await?;

    if json_output {
        let result = json!({
            "healthy": status.healthy,
            "issues": status.issues,
            "dry_run": dry_run,
            "repaired": !dry_run && !status.healthy,
        });
        println!("{}", serde_json::to_string_pretty(&result)?);

        if !status.healthy && !dry_run {
            // Repair
            db.ensure_system_tenant().await?;
        }

        return Ok(());
    }

    if status.healthy {
        output.success("Bootstrap state is healthy");
        output.kv("System tenant", "present");
        output.kv("Core policies", "all enabled");
        return Ok(());
    }

    // Report issues
    let _ = output.warn("Bootstrap state has issues:");
    for issue in &status.issues {
        output.info(format!("  - {}", issue));
    }

    if dry_run {
        output.info("\nDry run - no changes made");
        output.info("Run without --dry-run to repair");
        return Ok(());
    }

    // Repair
    output.info("\nRepairing bootstrap state...");
    db.ensure_system_tenant().await?;

    // Validate again
    let status_after = db.validate_bootstrap_state().await?;
    if status_after.healthy {
        output.success("Bootstrap state repaired successfully");
    } else {
        output.error("Some issues remain after repair:");
        for issue in &status_after.issues {
            output.info(format!("  - {}", issue));
        }
        anyhow::bail!("Failed to fully repair bootstrap state");
    }

    Ok(())
}
