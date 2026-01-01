//! CodeGraph commands for call graph analysis and export
//!
//! Consolidates codegraph-related CLI commands:
//! - `codegraph export` - Export call graph to various formats (DOT, JSON, CSV)
//! - `codegraph stats` - Generate CodeGraph database statistics
//!
use crate::commands::{codegraph_stats, export_callgraph};
use crate::output::OutputWriter;
use adapteros_core::Result;
use clap::Subcommand;
use std::path::PathBuf;

/// Export format options for call graph export
#[derive(Debug, Clone, PartialEq)]
pub enum ExportFormat {
    /// DOT format for Graphviz
    Dot,
    /// JSON format
    Json,
    /// CSV format
    Csv,
}

impl std::str::FromStr for ExportFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "dot" | "graphviz" => Ok(ExportFormat::Dot),
            "json" => Ok(ExportFormat::Json),
            "csv" => Ok(ExportFormat::Csv),
            _ => Err(format!(
                "Unknown export format: {}. Valid formats: dot, json, csv",
                s
            )),
        }
    }
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Dot => write!(f, "dot"),
            ExportFormat::Json => write!(f, "json"),
            ExportFormat::Csv => write!(f, "csv"),
        }
    }
}

impl From<ExportFormat> for export_callgraph::ExportFormat {
    fn from(value: ExportFormat) -> Self {
        match value {
            ExportFormat::Dot => export_callgraph::ExportFormat::Dot,
            ExportFormat::Json => export_callgraph::ExportFormat::Json,
            ExportFormat::Csv => export_callgraph::ExportFormat::Csv,
        }
    }
}

/// CodeGraph subcommands for call graph analysis
#[derive(Debug, Clone, Subcommand)]
pub enum CodegraphCommand {
    /// Export call graph to various formats (DOT, JSON, CSV)
    #[command(after_help = r#"Examples:
  # Export to DOT format for Graphviz visualization
  aosctl codegraph export --codegraph-db ./var/codegraph.db --output graph.dot

  # Export to JSON format
  aosctl codegraph export --codegraph-db ./var/codegraph.db --output graph.json --format json

  # Export to CSV format for spreadsheet analysis
  aosctl codegraph export --codegraph-db ./var/codegraph.db --output edges.csv --format csv
"#)]
    Export {
        /// CodeGraph database path
        #[arg(short, long)]
        codegraph_db: PathBuf,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Export format (dot, json, csv)
        #[arg(short, long, default_value = "dot")]
        format: String,
    },

    /// Generate CodeGraph database statistics
    #[command(after_help = r#"Examples:
  # Generate statistics
  aosctl codegraph stats --codegraph-db ./var/codegraph.db

  # Export statistics to JSON
  aosctl codegraph stats --codegraph-db ./var/codegraph.db --json > stats.json
"#)]
    Stats {
        /// CodeGraph database path
        #[arg(short, long)]
        codegraph_db: PathBuf,
    },
}

/// Get command name for telemetry
fn get_codegraph_command_name(cmd: &CodegraphCommand) -> &'static str {
    match cmd {
        CodegraphCommand::Export { .. } => "codegraph_export",
        CodegraphCommand::Stats { .. } => "codegraph_stats",
    }
}

/// Handle codegraph commands
///
/// Routes codegraph subcommands to their respective handlers.
pub async fn handle_codegraph_command(cmd: CodegraphCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_codegraph_command_name(&cmd);

    // Emit telemetry for command execution
    if let Err(e) = crate::cli_telemetry::emit_cli_command(command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        CodegraphCommand::Export {
            codegraph_db,
            output: output_path,
            format,
        } => {
            let format = format
                .parse::<ExportFormat>()
                .map_err(adapteros_core::AosError::Validation)?;
            export_callgraph::export_callgraph(&codegraph_db, &output_path, format.into(), output)
                .await?;
            Ok(())
        }
        CodegraphCommand::Stats { codegraph_db } => {
            codegraph_stats::run(codegraph_db, output).await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_format_parsing() {
        assert_eq!(
            "dot".parse::<ExportFormat>().expect("dot should parse"),
            ExportFormat::Dot
        );
        assert_eq!(
            "graphviz"
                .parse::<ExportFormat>()
                .expect("graphviz should parse"),
            ExportFormat::Dot
        );
        assert_eq!(
            "json".parse::<ExportFormat>().expect("json should parse"),
            ExportFormat::Json
        );
        assert_eq!(
            "csv".parse::<ExportFormat>().expect("csv should parse"),
            ExportFormat::Csv
        );
        assert_eq!(
            "JSON"
                .parse::<ExportFormat>()
                .expect("JSON uppercase should parse"),
            ExportFormat::Json
        );
        assert_eq!(
            "DOT"
                .parse::<ExportFormat>()
                .expect("DOT uppercase should parse"),
            ExportFormat::Dot
        );

        assert!("unknown".parse::<ExportFormat>().is_err());
        assert!("xml".parse::<ExportFormat>().is_err());
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(ExportFormat::Dot.to_string(), "dot");
        assert_eq!(ExportFormat::Json.to_string(), "json");
        assert_eq!(ExportFormat::Csv.to_string(), "csv");
    }

    #[test]
    fn test_get_codegraph_command_name() {
        assert_eq!(
            get_codegraph_command_name(&CodegraphCommand::Export {
                codegraph_db: PathBuf::from("test.db"),
                output: PathBuf::from("out.dot"),
                format: "dot".to_string(),
            }),
            "codegraph_export"
        );
        assert_eq!(
            get_codegraph_command_name(&CodegraphCommand::Stats {
                codegraph_db: PathBuf::from("test.db"),
            }),
            "codegraph_stats"
        );
    }
}
