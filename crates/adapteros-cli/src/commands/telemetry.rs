//! Telemetry management commands
//!
//! Provides git-style subcommands for telemetry operations:
//! - `aosctl telemetry list` - List telemetry events
//! - `aosctl telemetry verify` - Verify telemetry bundle chain integrity

use crate::output::OutputWriter;
use adapteros_core::Result;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::info;

/// Telemetry subcommands
#[derive(Debug, Subcommand, Clone)]
pub enum TelemetryCommand {
    /// List telemetry events with optional filtering
    #[command(
        after_help = r#"Examples:
  # List all events
  aosctl telemetry list

  # Filter by stack
  aosctl telemetry list --by-stack stack-prod-001

  # Filter by event type
  aosctl telemetry list --event-type router.decision

  # Combine filters with JSON output
  aosctl telemetry list --by-stack stack-prod-001 --limit 100 --json > events.json
"#
    )]
    List {
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,

        /// Filter by stack ID (PRD-03)
        #[arg(long)]
        by_stack: Option<String>,

        /// Filter by event type
        #[arg(long)]
        event_type: Option<String>,

        /// Maximum number of events to return
        #[arg(long, default_value = "50")]
        limit: u32,
    },

    /// Verify telemetry bundle chain integrity
    #[command(
        after_help = r#"Examples:
  aosctl telemetry verify --bundle-dir ./var/telemetry
  aosctl telemetry verify --bundle-dir ./var/telemetry --json > verify.json
"#
    )]
    Verify {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,
    },
}

/// Get telemetry command name for telemetry emission
fn get_telemetry_command_name(cmd: &TelemetryCommand) -> String {
    match cmd {
        TelemetryCommand::List { .. } => "telemetry_list".to_string(),
        TelemetryCommand::Verify { .. } => "telemetry_verify".to_string(),
    }
}

/// Handle telemetry subcommands
///
/// Routes telemetry commands to appropriate handlers
pub async fn handle_telemetry_command(cmd: TelemetryCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_telemetry_command_name(&cmd);

    info!(command = ?cmd, "Handling telemetry command");

    // Emit CLI telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await;

    match cmd {
        TelemetryCommand::List {
            database,
            by_stack,
            event_type,
            limit,
        } => {
            crate::commands::telemetry_list::list_telemetry_events(
                &database,
                by_stack.as_deref(),
                event_type.as_deref(),
                limit,
                output,
            )
            .await
            .map_err(|e| adapteros_core::AosError::Other(e.to_string()))
        }
        TelemetryCommand::Verify { bundle_dir } => {
            crate::commands::verify_telemetry::verify_telemetry_chain(&bundle_dir, output).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputMode;

    #[test]
    fn test_get_telemetry_command_name() {
        assert_eq!(
            get_telemetry_command_name(&TelemetryCommand::List {
                database: PathBuf::from("test.db"),
                by_stack: None,
                event_type: None,
                limit: 50
            }),
            "telemetry_list"
        );
        assert_eq!(
            get_telemetry_command_name(&TelemetryCommand::Verify {
                bundle_dir: PathBuf::from("./bundles")
            }),
            "telemetry_verify"
        );
    }

    #[test]
    fn test_telemetry_command_clone() {
        let cmd = TelemetryCommand::List {
            database: PathBuf::from("./var/aos-cp.sqlite3"),
            by_stack: Some("stack-prod-001".to_string()),
            event_type: Some("router.decision".to_string()),
            limit: 100,
        };

        let cloned = cmd.clone();
        match cloned {
            TelemetryCommand::List {
                database,
                by_stack,
                event_type,
                limit,
            } => {
                assert_eq!(database, PathBuf::from("./var/aos-cp.sqlite3"));
                assert_eq!(by_stack, Some("stack-prod-001".to_string()));
                assert_eq!(event_type, Some("router.decision".to_string()));
                assert_eq!(limit, 100);
            }
            _ => panic!("Expected List variant"),
        }
    }

    #[test]
    fn test_verify_command_clone() {
        let cmd = TelemetryCommand::Verify {
            bundle_dir: PathBuf::from("./var/telemetry"),
        };

        let cloned = cmd.clone();
        match cloned {
            TelemetryCommand::Verify { bundle_dir } => {
                assert_eq!(bundle_dir, PathBuf::from("./var/telemetry"));
            }
            _ => panic!("Expected Verify variant"),
        }
    }
}
