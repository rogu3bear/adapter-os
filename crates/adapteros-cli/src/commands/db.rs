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
}

/// Handle database commands
pub async fn handle_db_command(cmd: DbCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        DbCommand::Migrate {
            db_path,
            verify_only,
        } => run_migrate(db_path, verify_only, output).await,
    }
}

async fn run_migrate(
    db_path: Option<PathBuf>,
    verify_only: bool,
    output: &OutputWriter,
) -> Result<()> {
    use adapteros_db::Database;

    // Determine database path
    let db_url = if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    if verify_only {
        output.info("Verifying migration signatures...")?;

        // Just verify signatures without running migrations
        use adapteros_db::migration_verify::MigrationVerifier;
        let verifier = MigrationVerifier::new("migrations")?;
        verifier.verify_all()?;

        output.success("All migration signatures verified")?;
        return Ok(());
    }

    output.info(&format!("Running database migrations on: {}", db_url))?;

    // Connect to database and run migrations
    let db = Database::connect(&db_url).await?;
    db.migrate().await?;

    output.success("Database migrations completed successfully")?;
    Ok(())
}
