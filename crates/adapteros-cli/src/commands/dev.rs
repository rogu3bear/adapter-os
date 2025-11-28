//! Development boot commands for local testing

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use clap::Subcommand;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tracing::{error, info, warn};

const PID_DIR: &str = "./var/run";
const API_SERVER_PID: &str = "./var/run/api-server.pid";
const UI_SERVER_PID: &str = "./var/run/ui-server.pid";

#[derive(Debug, Subcommand, Clone)]
pub enum DevCommand {
    /// Start development services
    #[command(
        after_help = "Examples:\n  aosctl dev up\n  aosctl dev up --ui\n  aosctl dev up --db-reset"
    )]
    Up {
        /// Start UI dev server
        #[arg(long)]
        ui: bool,

        /// Reset database before starting
        #[arg(long)]
        db_reset: bool,

        /// Skip database migrations
        #[arg(long)]
        skip_migrations: bool,
    },

    /// Stop development services
    #[command(after_help = "Examples:\n  aosctl dev down")]
    Down,

    /// Show development service status
    #[command(after_help = "Examples:\n  aosctl dev status\n  aosctl dev status --json")]
    Status {
        /// Output format
        #[arg(long)]
        json: bool,
    },

    /// Tail development service logs
    #[command(
        after_help = "Examples:\n  aosctl dev logs\n  aosctl dev logs --service api\n  aosctl dev logs --service ui"
    )]
    Logs {
        /// Service name (api or ui)
        #[arg(long)]
        service: Option<String>,

        /// Number of lines to show
        #[arg(long, default_value = "50")]
        lines: usize,
    },
}

/// Handle dev commands
pub async fn handle_dev_command(cmd: DevCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_dev_command_name(&cmd);

    info!(command = ?cmd, "Handling dev command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await;

    match cmd {
        DevCommand::Up {
            ui,
            db_reset,
            skip_migrations,
        } => dev_up(ui, db_reset, skip_migrations, output).await,
        DevCommand::Down => dev_down(output).await,
        DevCommand::Status { json } => dev_status(json, output).await,
        DevCommand::Logs { service, lines } => dev_logs(service, lines, output).await,
    }
}

/// Get dev command name for telemetry
fn get_dev_command_name(cmd: &DevCommand) -> String {
    match cmd {
        DevCommand::Up { .. } => "dev_up",
        DevCommand::Down => "dev_down",
        DevCommand::Status { .. } => "dev_status",
        DevCommand::Logs { .. } => "dev_logs",
    }
    .to_string()
}

/// Start development services
async fn dev_up(
    ui: bool,
    db_reset: bool,
    skip_migrations: bool,
    output: &OutputWriter,
) -> Result<()> {
    info!(
        ui = ui,
        db_reset = db_reset,
        "Starting development services"
    );

    output.info("Starting AdapterOS development environment");
    output.blank();

    // Ensure PID directory exists
    fs::create_dir_all(PID_DIR)
        .map_err(|e| AosError::Io(format!("Failed to create PID directory: {}", e)))?;

    // Step 1: Handle database
    if db_reset {
        output.progress("Resetting database...");
        reset_database(output)?;
        output.progress_done(true);
    }

    if !skip_migrations {
        output.progress("Running database migrations...");
        run_migrations(output)?;
        output.progress_done(true);
    }

    // Step 2: Start API server
    output.progress("Starting API server...");
    start_api_server(output)?;
    output.progress_done(true);

    // Step 3: Start UI server (if requested)
    if ui {
        output.progress("Starting UI dev server...");
        start_ui_server(output)?;
        output.progress_done(true);
    }

    output.blank();
    output.success("Development environment started");
    output.blank();
    output.info("Services:");
    output.kv("API Server", "http://localhost:8080");
    if ui {
        output.kv("UI Server", "http://localhost:5173");
    }
    output.blank();
    output.info("To stop services: aosctl dev down");
    output.info("To view logs: aosctl dev logs");

    Ok(())
}

/// Stop development services
async fn dev_down(output: &OutputWriter) -> Result<()> {
    info!("Stopping development services");

    output.info("Stopping development services");
    output.blank();

    let mut stopped_count = 0;

    // Stop API server
    if let Some(pid) = read_pid_file(API_SERVER_PID)? {
        output.progress(&format!("Stopping API server (PID {})...", pid));
        if stop_process(pid)? {
            fs::remove_file(API_SERVER_PID).ok();
            output.progress_done(true);
            stopped_count += 1;
        } else {
            output.progress_done(false);
            output.warning("API server process not found (may have already stopped)");
        }
    }

    // Stop UI server
    if let Some(pid) = read_pid_file(UI_SERVER_PID)? {
        output.progress(&format!("Stopping UI server (PID {})...", pid));
        if stop_process(pid)? {
            fs::remove_file(UI_SERVER_PID).ok();
            output.progress_done(true);
            stopped_count += 1;
        } else {
            output.progress_done(false);
            output.warning("UI server process not found (may have already stopped)");
        }
    }

    if stopped_count == 0 {
        output.warning("No running services found");
    } else {
        output.blank();
        output.success(&format!("Stopped {} service(s)", stopped_count));
    }

    Ok(())
}

/// Show development service status
async fn dev_status(json: bool, output: &OutputWriter) -> Result<()> {
    info!("Checking development service status");

    let api_status = check_service_status(API_SERVER_PID, "API Server")?;
    let ui_status = check_service_status(UI_SERVER_PID, "UI Server")?;

    if json {
        let status = serde_json::json!({
            "api_server": api_status,
            "ui_server": ui_status,
        });
        output.result(&serde_json::to_string_pretty(&status)?);
    } else {
        output.info("Development Service Status");
        output.blank();

        output.kv(
            "API Server",
            if api_status.running {
                "Running"
            } else {
                "Stopped"
            },
        );
        if let Some(pid) = api_status.pid {
            output.kv("  PID", &pid.to_string());
        }

        output.kv(
            "UI Server",
            if ui_status.running {
                "Running"
            } else {
                "Stopped"
            },
        );
        if let Some(pid) = ui_status.pid {
            output.kv("  PID", &pid.to_string());
        }

        output.blank();
        if api_status.running || ui_status.running {
            output.info("To stop services: aosctl dev down");
        } else {
            output.info("To start services: aosctl dev up");
        }
    }

    Ok(())
}

/// Show development service logs
async fn dev_logs(service: Option<String>, lines: usize, output: &OutputWriter) -> Result<()> {
    info!(service = ?service, lines = lines, "Showing development logs");

    let log_files = match service.as_deref() {
        Some("api") => vec!["./var/logs/api-server.log"],
        Some("ui") => vec!["./var/logs/ui-server.log"],
        None => vec!["./var/logs/api-server.log", "./var/logs/ui-server.log"],
        Some(other) => {
            return Err(AosError::Validation(format!(
                "Unknown service: {}. Use 'api' or 'ui'",
                other
            )));
        }
    };

    for log_file in log_files {
        let path = Path::new(log_file);
        if !path.exists() {
            output.warning(&format!("Log file not found: {}", log_file));
            continue;
        }

        output.info(&format!("=== {} ===", log_file));
        output.blank();

        // Read last N lines (simple implementation)
        match tail_file(path, lines) {
            Ok(content) => {
                output.result(&content);
            }
            Err(e) => {
                error!(error = %e, log_file = %log_file, "Failed to read log file");
                output.error(&format!("Failed to read log: {}", e));
            }
        }

        output.blank();
    }

    Ok(())
}

/// Reset database (dev only)
fn reset_database(output: &OutputWriter) -> Result<()> {
    let db_path = Path::new("./var/aos-cp.sqlite3");
    if db_path.exists() {
        fs::remove_file(db_path)
            .map_err(|e| AosError::Io(format!("Failed to remove database: {}", e)))?;
        output.verbose("Database removed");
    }
    Ok(())
}

/// Run database migrations
fn run_migrations(output: &OutputWriter) -> Result<()> {
    // Use aosctl db migrate command (assuming it exists)
    let status = Command::new("cargo")
        .args(&["run", "-p", "adapteros-cli", "--", "db", "migrate"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| AosError::Io(format!("Failed to run migrations: {}", e)))?;

    if !status.success() {
        return Err(AosError::Other("Database migrations failed".to_string()));
    }

    Ok(())
}

/// Start API server
fn start_api_server(output: &OutputWriter) -> Result<()> {
    let mut child = Command::new("cargo")
        .args(&["run", "--release", "-p", "adapteros-server-api"])
        .env("RUST_LOG", "info")
        .env("DATABASE_URL", "sqlite://var/aos-cp.sqlite3")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AosError::Io(format!("Failed to start API server: {}", e)))?;

    let pid = child.id();
    write_pid_file(API_SERVER_PID, pid)?;

    // Detach the process (don't wait for it)
    std::mem::forget(child);

    output.verbose(&format!("API server started (PID: {})", pid));
    Ok(())
}

/// Start UI dev server
fn start_ui_server(output: &OutputWriter) -> Result<()> {
    let ui_dir = Path::new("./ui");
    if !ui_dir.exists() {
        return Err(AosError::Io("UI directory not found: ./ui".to_string()));
    }

    let mut child = Command::new("pnpm")
        .args(&["dev"])
        .current_dir(ui_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AosError::Io(format!("Failed to start UI server: {}", e)))?;

    let pid = child.id();
    write_pid_file(UI_SERVER_PID, pid)?;

    // Detach the process
    std::mem::forget(child);

    output.verbose(&format!("UI server started (PID: {})", pid));
    Ok(())
}

/// Service status
#[derive(Debug, Clone, serde::Serialize)]
struct ServiceStatus {
    running: bool,
    pid: Option<u32>,
}

/// Check service status
fn check_service_status(pid_file: &str, _service_name: &str) -> Result<ServiceStatus> {
    if let Some(pid) = read_pid_file(pid_file)? {
        // Check if process is still running
        let running = process_exists(pid)?;
        Ok(ServiceStatus {
            running,
            pid: if running { Some(pid) } else { None },
        })
    } else {
        Ok(ServiceStatus {
            running: false,
            pid: None,
        })
    }
}

/// Read PID from file
fn read_pid_file(path: &str) -> Result<Option<u32>> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Io(format!("Failed to read PID file: {}", e)))?;
    let pid = content
        .trim()
        .parse::<u32>()
        .map_err(|e| AosError::Io(format!("Invalid PID format: {}", e)))?;

    Ok(Some(pid))
}

/// Write PID to file
fn write_pid_file(path: &str, pid: u32) -> Result<()> {
    fs::write(path, pid.to_string())
        .map_err(|e| AosError::Io(format!("Failed to write PID file: {}", e)))
}

/// Check if process exists
fn process_exists(pid: u32) -> Result<bool> {
    // Use platform-specific approach
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let status = Command::new("kill")
            .args(&["-0", &pid.to_string()])
            .status()
            .map_err(|e| AosError::Io(format!("Failed to check process: {}", e)))?;

        Ok(status.success())
    }

    #[cfg(not(unix))]
    {
        // Fallback for non-Unix systems
        warn!("Process check not implemented for this platform");
        Ok(true)
    }
}

/// Stop process
fn stop_process(pid: u32) -> Result<bool> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args(&[&pid.to_string()])
            .status()
            .map_err(|e| AosError::Io(format!("Failed to stop process: {}", e)))?;

        Ok(status.success())
    }

    #[cfg(not(unix))]
    {
        // Fallback for non-Unix systems
        warn!("Process termination not implemented for this platform");
        Err(AosError::Other(
            "Process termination not supported on this platform".to_string(),
        ))
    }
}

/// Read last N lines from file
fn tail_file(path: &Path, lines: usize) -> Result<String> {
    let file =
        fs::File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;
    let reader = BufReader::new(file);

    let mut all_lines: Vec<String> = reader.lines().filter_map(|line| line.ok()).collect();

    // Get last N lines
    let start_index = if all_lines.len() > lines {
        all_lines.len() - lines
    } else {
        0
    };

    let tail_lines = all_lines.drain(start_index..).collect::<Vec<_>>();
    Ok(tail_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_dev_command_name() {
        assert_eq!(
            get_dev_command_name(&DevCommand::Up {
                ui: false,
                db_reset: false,
                skip_migrations: false,
            }),
            "dev_up"
        );
        assert_eq!(get_dev_command_name(&DevCommand::Down), "dev_down");
        assert_eq!(
            get_dev_command_name(&DevCommand::Status { json: false }),
            "dev_status"
        );
    }

    #[test]
    fn test_service_status_serialization() {
        let status = ServiceStatus {
            running: true,
            pid: Some(1234),
        };

        let json = serde_json::to_string(&status).unwrap();
        let deserialized: ServiceStatus = serde_json::from_str(&json).unwrap();

        assert_eq!(status.running, deserialized.running);
        assert_eq!(status.pid, deserialized.pid);
    }
}
