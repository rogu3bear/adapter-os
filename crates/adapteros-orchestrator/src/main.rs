//! Orchestrator CLI

use adapteros_orchestrator::{Orchestrator, OrchestratorConfig, ReportFormat};
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, error};

#[derive(Parser)]
#[command(name = "mplora-orchestrator")]
#[command(about = "AdapterOS promotion gate orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run promotion gates for a CPID
    Gate {
        /// CPID to check
        #[arg(long)]
        cpid: String,

        /// Continue running gates even if one fails
        #[arg(long)]
        continue_on_error: bool,

        /// Path to database
        #[arg(long, default_value = "var/aos-cp.sqlite3")]
        db_path: String,

        /// Path to telemetry bundles
        #[arg(long, default_value = "/srv/aos/bundles")]
        bundles_path: String,

        /// Path to manifests
        #[arg(long, default_value = "manifests")]
        manifests_path: String,

        /// Output report path (defaults to stdout)
        #[arg(long)]
        report: Option<PathBuf>,

        /// Report format (json or markdown)
        #[arg(long, default_value = "markdown")]
        format: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Gate {
            cpid,
            continue_on_error,
            db_path,
            bundles_path,
            manifests_path,
            report,
            format,
        } => {
            info!(cpid = %cpid, "Starting AdapterOS Promotion Gate Orchestrator");

            let config = OrchestratorConfig {
                continue_on_error,
                cpid: cpid.clone(),
                db_path,
                bundles_path,
                manifests_path,
                base_model: "models/qwen2.5-7b-mlx".to_string(),
                ephemeral_adapter_ttl_hours: 24,
            };

            let orchestrator = Orchestrator::new(config);
            let gate_report = orchestrator.run().await?;

            if gate_report.all_passed {
                info!(cpid = %cpid, gate_passed = true, "All gates passed successfully");
            } else {
                error!(cpid = %cpid, gate_passed = false, "One or more gates failed");
            }

            // Write report if requested
            if let Some(report_path) = report {
                let format = match format.as_str() {
                    "json" => ReportFormat::Json,
                    _ => ReportFormat::Markdown,
                };

                gate_report.write_to_file(&report_path, format)?;
                info!(report_path = %report_path.display(), "Report written to file");
            }

            // Exit with error code if gates failed
            if !gate_report.all_passed {
                std::process::exit(1);
            }

            Ok(())
        }
    }
}
