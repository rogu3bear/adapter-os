//! Database management commands

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::output::OutputWriter;

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
}

/// Handle database commands
pub async fn handle_db_command(cmd: DbCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DbCommand::Migrate {
            db_path,
            verify_only,
        } => run_migrate(db_path, verify_only, output).await,
        DbCommand::Reset { db_path, force } => run_reset(db_path, force, output).await,
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
            anyhow::bail!("Migrations directory not found: {}", migrations_path.display());
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
    use std::io::{self, Write};

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
        io::stdin().read_line(&mut input)?;

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
