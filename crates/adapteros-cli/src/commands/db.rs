//! Database management commands

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::output::OutputWriter;
use adapteros_db::{
    adapters::AdapterRegistrationBuilder,
    chat_sessions::{AddMessageParams, CreateChatSessionParams},
    models::ModelRegistrationBuilder,
    Db,
};
use serde_json::json;

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
}

/// Handle database commands
pub async fn handle_db_command(cmd: DbCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DbCommand::Migrate {
            db_path,
            verify_only,
        } => run_migrate(db_path, verify_only, output).await,
        DbCommand::Reset { db_path, force } => run_reset(db_path, force, output).await,
        DbCommand::SeedFixtures {
            db_path,
            skip_reset,
            chat,
        } => run_seed_fixtures(db_path, skip_reset, chat, output).await,
    }
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

    output.info(&format!("Running database migrations on: {}", db_url));

    // Connect to database and run migrations
    let db = Db::connect(&db_url).await?;
    db.migrate().await?;

    output.success("Database migrations completed successfully");
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
        output.warning(&format!(
            "This will DELETE the database at: {}",
            db_path_str
        ));
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
        output.info(&format!("Deleted {}", db_path_str));
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
        output.info(&format!(
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

    output.info(&format!("Seeding deterministic fixtures into {}", db_url));

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
    .execute(&*pool)
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
            .execute(&*pool)
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
    .execute(&*pool)
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
            .execute(&*pool)
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
    .execute(&*pool)
    .await?;

    // Ensure tenant default stack + pins are aligned with seeded stack/adapter
    sqlx::query(
        "UPDATE tenants SET default_stack_id = ?, default_pinned_adapter_ids = ? WHERE id = ?",
    )
    .bind(STACK_ID)
    .bind(&pinned)
    .bind(TENANT_ID)
    .execute(&*pool)
    .await?;

    // Optionally seed a starter chat session + message for assertions
    if chat {
        // Clear any prior seeded session/messages to keep the command idempotent
        sqlx::query("DELETE FROM chat_messages WHERE session_id = ? OR id = ?")
            .bind(CHAT_SESSION_ID)
            .bind(CHAT_MESSAGE_ID)
            .execute(&*pool)
            .await?;
        sqlx::query("DELETE FROM chat_sessions WHERE id = ?")
            .bind(CHAT_SESSION_ID)
            .execute(&*pool)
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
    if chat {
        output.kv("Chat session", CHAT_SESSION_ID);
    }

    Ok(())
}
