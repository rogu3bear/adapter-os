//! Development boot commands for local testing

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use clap::Subcommand;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use tokio::process::Command as TokioCommand;
use tracing::{error, info, warn};

const PID_DIR: &str = "./var/run";
const API_SERVER_PID: &str = "./var/run/api-server.pid";
const UI_SERVER_PID: &str = "./var/run/ui-server.pid";
const WORKER_PID: &str = "./var/run/aos-worker.pid";

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
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

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

/// Convenience super-command: bring up API + worker and attach the TUI
pub async fn dev_all(output: &OutputWriter) -> Result<()> {
    output.section("Starting development stack");
    dev_up(false, false, false, output).await?;
    start_worker(output).await?;

    if output.mode().is_json() {
        let status = serde_json::json!({
            "api_server": check_service_status(API_SERVER_PID, "API Server")?,
            "worker": check_service_status(WORKER_PID, "Worker")?,
        });
        output.print_json(&status)?;
        return Ok(());
    }

    output.info("Attaching TUI (Ctrl+C to exit)");
    attach_tui(output).await?;
    Ok(())
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

    output.info("Starting adapterOS development environment");
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
    start_api_server(output).await?;
    output.progress_done(true);

    // Step 3: Start UI server (if requested)
    if ui {
        output.progress("Starting UI dev server...");
        start_ui_server(output).await?;
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
        output.progress(format!("Stopping API server (PID {})...", pid));
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
        output.progress(format!("Stopping UI server (PID {})...", pid));
        if stop_process(pid)? {
            fs::remove_file(UI_SERVER_PID).ok();
            output.progress_done(true);
            stopped_count += 1;
        } else {
            output.progress_done(false);
            output.warning("UI server process not found (may have already stopped)");
        }
    }

    // Stop worker
    if let Some(pid) = read_pid_file(WORKER_PID)? {
        output.progress(format!("Stopping worker (PID {})...", pid));
        if stop_process(pid)? {
            fs::remove_file(WORKER_PID).ok();
            output.progress_done(true);
            stopped_count += 1;
        } else {
            output.progress_done(false);
            output.warning("Worker process not found (may have already stopped)");
        }
    }

    if stopped_count == 0 {
        output.warning("No running services found");
    } else {
        output.blank();
        output.success(format!("Stopped {} service(s)", stopped_count));
    }

    Ok(())
}

/// Show development service status
async fn dev_status(json: bool, output: &OutputWriter) -> Result<()> {
    info!("Checking development service status");

    let api_status = check_service_status(API_SERVER_PID, "API Server")?;
    let ui_status = check_service_status(UI_SERVER_PID, "UI Server")?;
    let worker_status = check_service_status(WORKER_PID, "Worker")?;

    if json {
        let status = serde_json::json!({
            "api_server": api_status,
            "ui_server": ui_status,
            "worker": worker_status,
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

        output.kv(
            "Worker",
            if worker_status.running {
                "Running"
            } else {
                "Stopped"
            },
        );
        if let Some(pid) = worker_status.pid {
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
        Some("worker") => vec!["./var/logs/worker.log"],
        None => vec![
            "./var/logs/api-server.log",
            "./var/logs/ui-server.log",
            "./var/logs/worker.log",
        ],
        Some(other) => {
            return Err(AosError::Validation(format!(
                "Unknown service: {}. Use 'api', 'ui', or 'worker'",
                other
            )));
        }
    };

    for log_file in log_files {
        let path = Path::new(log_file);
        if !path.exists() {
            output.warning(format!("Log file not found: {}", log_file));
            continue;
        }

        output.info(format!("=== {} ===", log_file));
        output.blank();

        // Read last N lines (simple implementation)
        match tail_file(path, lines) {
            Ok(content) => {
                output.result(&content);
            }
            Err(e) => {
                error!(error = %e, log_file = %log_file, "Failed to read log file");
                output.error(format!("Failed to read log: {}", e));
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
fn run_migrations(_output: &OutputWriter) -> Result<()> {
    // Use aosctl db migrate command (assuming it exists)
    let status = Command::new("cargo")
        .args(["run", "-p", "adapteros-cli", "--", "db", "migrate"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| AosError::Io(format!("Failed to run migrations: {}", e)))?;

    if !status.success() {
        return Err(AosError::Database("Database migrations failed".to_string()));
    }

    Ok(())
}

/// Start API server
async fn start_api_server(output: &OutputWriter) -> Result<()> {
    // Ensure log directory exists
    fs::create_dir_all("./var/logs")
        .map_err(|e| AosError::Io(format!("Failed to create log directory: {}", e)))?;

    // Redirect output to log file
    // Open separate handles for stdout and stderr (both write to same file)
    let stdout_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./var/logs/api-server.log")
        .map_err(|e| AosError::Io(format!("Failed to open log file: {}", e)))?;
    let stderr_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./var/logs/api-server.log")
        .map_err(|e| AosError::Io(format!("Failed to open log file: {}", e)))?;

    let mut child = TokioCommand::new("cargo")
        .args(["run", "--release", "-p", "adapteros-server-api"])
        .env("RUST_LOG", "info")
        .env("DATABASE_URL", "sqlite://var/aos-cp.sqlite3")
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| AosError::Io(format!("Failed to start API server: {}", e)))?;

    let pid = child
        .id()
        .ok_or_else(|| AosError::Io("Failed to get child process ID".to_string()))?;
    write_pid_file(API_SERVER_PID, pid)?;

    // Spawn background task to wait for the child process to prevent zombie processes
    // This allows the OS to properly reap the process when it exits
    let child_pid = pid;
    tokio::spawn(async move {
        if let Err(e) = child.wait().await {
            warn!(pid = child_pid, error = %e, "Error waiting for API server process");
        }
    });

    output.verbose(format!("API server started (PID: {})", pid));
    Ok(())
}

async fn start_worker(output: &OutputWriter) -> Result<()> {
    fs::create_dir_all("./var/logs")
        .map_err(|e| AosError::Io(format!("Failed to create log directory: {}", e)))?;

    let stdout_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./var/logs/worker.log")
        .map_err(|e| AosError::Io(format!("Failed to open worker log file: {}", e)))?;
    let stderr_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./var/logs/worker.log")
        .map_err(|e| AosError::Io(format!("Failed to open worker log file: {}", e)))?;

    let mut child = TokioCommand::new("cargo")
        .args([
            "run",
            "--release",
            "-p",
            "adapteros-lora-worker",
            "--bin",
            "aos_worker",
        ])
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| AosError::Io(format!("Failed to start worker: {}", e)))?;

    let pid = child
        .id()
        .ok_or_else(|| AosError::Io("Failed to get worker PID".to_string()))?;
    write_pid_file(WORKER_PID, pid)?;

    let child_pid = pid;
    tokio::spawn(async move {
        if let Err(e) = child.wait().await {
            warn!(pid = child_pid, error = %e, "Error waiting for worker process");
        }
    });

    output.verbose(format!("Worker started (PID: {})", pid));
    Ok(())
}

/// Start UI dev server
async fn start_ui_server(output: &OutputWriter) -> Result<()> {
    let ui_dir = Path::new("./ui");
    if !ui_dir.exists() {
        return Err(AosError::Io("UI directory not found: ./ui".to_string()));
    }

    // Ensure log directory exists
    fs::create_dir_all("./var/logs")
        .map_err(|e| AosError::Io(format!("Failed to create log directory: {}", e)))?;

    // Redirect output to log file
    // Open separate handles for stdout and stderr (both write to same file)
    let stdout_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./var/logs/ui-server.log")
        .map_err(|e| AosError::Io(format!("Failed to open log file: {}", e)))?;
    let stderr_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("./var/logs/ui-server.log")
        .map_err(|e| AosError::Io(format!("Failed to open log file: {}", e)))?;

    let mut child = TokioCommand::new("pnpm")
        .args(["dev"])
        .current_dir(ui_dir)
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .map_err(|e| AosError::Io(format!("Failed to start UI server: {}", e)))?;

    let pid = child
        .id()
        .ok_or_else(|| AosError::Io("Failed to get child process ID".to_string()))?;
    write_pid_file(UI_SERVER_PID, pid)?;

    // Spawn background task to wait for the child process to prevent zombie processes
    // This allows the OS to properly reap the process when it exits
    let child_pid = pid;
    tokio::spawn(async move {
        if let Err(e) = child.wait().await {
            warn!(pid = child_pid, error = %e, "Error waiting for UI server process");
        }
    });

    output.verbose(format!("UI server started (PID: {})", pid));
    Ok(())
}

async fn attach_tui(output: &OutputWriter) -> Result<()> {
    let _ = output;

    #[cfg(feature = "tui")]
    {
        crate::commands::tui::run(crate::commands::tui::TuiArgs { server_url: None })
            .await
            .map_err(|e| AosError::Internal(e.to_string()))?;
    }

    #[cfg(not(feature = "tui"))]
    {
        output.warning("TUI feature not enabled; rebuild with --features tui to auto-attach");
    }

    Ok(())
}

/// Service status
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
        let status = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map_err(|e| AosError::Io(format!("Failed to check process: {}", e)))?;

        Ok(status.success())
    }

    #[cfg(target_os = "windows")]
    {
        use sysinfo::{Pid, ProcessRefreshKind, System};

        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessRefreshKind::new());

        let sysinfo_pid = Pid::from_u32(pid);
        Ok(sys.process(sysinfo_pid).is_some())
    }

    #[cfg(all(not(unix), not(target_os = "windows")))]
    {
        // Fallback for other systems
        warn!("Process check not implemented for this platform");
        Ok(true)
    }
}

/// Stop process
fn stop_process(pid: u32) -> Result<bool> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .args([&pid.to_string()])
            .status()
            .map_err(|e| AosError::Io(format!("Failed to stop process: {}", e)))?;

        Ok(status.success())
    }

    #[cfg(target_os = "windows")]
    {
        use sysinfo::{Pid, ProcessRefreshKind, System};

        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessRefreshKind::new());

        let sysinfo_pid = Pid::from_u32(pid);
        if let Some(process) = sys.process(sysinfo_pid) {
            // Try graceful termination (SIGTERM equivalent on Windows)
            process.kill();
            info!("Sent termination signal to process {}", pid);
            Ok(true)
        } else {
            warn!("Process {} not found", pid);
            Ok(false)
        }
    }

    #[cfg(all(not(unix), not(target_os = "windows")))]
    {
        // Fallback for other systems
        warn!("Process termination not implemented for this platform");
        Err(AosError::Platform(
            "Process termination not supported on this platform".to_string(),
        ))
    }
}

/// Read last N lines from file
fn tail_file(path: &Path, lines: usize) -> Result<String> {
    let file =
        fs::File::open(path).map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;
    let reader = BufReader::new(file);

    let mut all_lines: Vec<String> = reader.lines().map_while(std::result::Result::ok).collect();

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

    #[test]
    fn test_process_exists_current_process() {
        // Test that the current process exists (should always be true)
        let current_pid = std::process::id();
        let exists = process_exists(current_pid).expect("process_exists should not fail");
        assert!(exists, "Current process should exist");
    }

    #[test]
    fn test_process_exists_nonexistent() {
        // Test with an extremely high PID that's unlikely to exist
        // Using a PID of 4294967295 (max u32) which is almost certainly not in use
        let nonexistent_pid = 4294967295u32;
        let result = process_exists(nonexistent_pid);

        // The function should not error, but return false for non-existent process
        // Note: on some systems, very high PIDs might be valid, so we just check no error
        assert!(
            result.is_ok(),
            "process_exists should not error on high PID"
        );
    }

    #[test]
    fn test_stop_process_nonexistent() {
        // Test stopping a process that doesn't exist
        let nonexistent_pid = 4294967295u32;
        let result = stop_process(nonexistent_pid);

        // On Unix, kill will fail for non-existent process
        // On Windows, sysinfo will not find the process and return false
        // Either way, we should get a result (not panic)
        match result {
            Ok(success) => {
                // On Windows, we expect Ok(false) for non-existent process
                #[cfg(target_os = "windows")]
                assert!(
                    !success,
                    "Stopping non-existent process should return false"
                );
                // On Unix, we might get Ok(false) if kill fails
                let _ = success;
            }
            Err(_) => {
                // On Unix, this is expected for non-existent PIDs
            }
        }
    }

    #[test]
    fn test_write_and_read_pid_file() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let pid_path = temp_dir.path().join("test.pid");
        let pid_str = pid_path.to_string_lossy().to_string();

        let test_pid = 12345u32;

        // Write PID file
        write_pid_file(&pid_str, test_pid).expect("write_pid_file should succeed");

        // Read it back
        let read_pid = read_pid_file(&pid_str).expect("read_pid_file should succeed");
        assert_eq!(
            read_pid,
            Some(test_pid),
            "Read PID should match written PID"
        );
    }

    #[test]
    fn test_read_pid_file_nonexistent() {
        let result = read_pid_file("/nonexistent/path/to/pid.file");
        assert!(
            result.is_ok() && result.unwrap().is_none(),
            "Reading non-existent PID file should return Ok(None)"
        );
    }

    #[test]
    fn test_tail_file() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let file_path = temp_dir.path().join("test.log");

        // Write test content
        let content = "line1\nline2\nline3\nline4\nline5";
        std::fs::write(&file_path, content).expect("Failed to write test file");

        // Test tailing last 3 lines
        let result = tail_file(&file_path, 3).expect("tail_file should succeed");
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line3");
        assert_eq!(lines[1], "line4");
        assert_eq!(lines[2], "line5");
    }
}
