//! Registry management commands
//!
//! Consolidates registry operations into a git-style subcommand structure:
//! - `aosctl registry sync` - Sync adapters from local directory to registry
//! - `aosctl registry migrate` - Migrate legacy registry database to new schema

use crate::output::OutputWriter;
use adapteros_core::Result;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::info;

// Re-export the RegistryMigrateArgs for external use
pub use crate::commands::registry_migrate::RegistryMigrateArgs;

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
/// - `sync` -> sync_registry::sync_registry()
/// - `migrate` -> registry_migrate::run()
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
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await;

    match cmd {
        RegistryCommand::Sync {
            dir,
            cas_root,
            registry,
        } => {
            crate::commands::sync_registry::sync_registry(&dir, &cas_root, &registry, output)
                .await
                .map_err(|e| adapteros_core::AosError::Registry(e.to_string()))
        }
        RegistryCommand::Migrate(args) => {
            crate::commands::registry_migrate::run(args, output).await
        }
    }
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
