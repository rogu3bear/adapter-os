//! adapterOS service & adapter runner CLI (`aos`).
//!
//! Target scope:
//! - Local services/adapters only (no DB writes, migrations, or cluster ops).
//! - Start/stop/restart/status/logs for backend, UI, and menu bar app.
//! - Deterministic config loading via `adapteros-config`.
//! - Structured logging via `tracing` with JSON output.
//!
//! # Citations
//!
//! - Previous launch UX and health checks: [source: aos-launch L1-L220]
//! - Legacy service manager PID/JSON tracking: [source: aos L1-L260]

use adapteros_config::initialize_config;
use adapteros_core::{AosError, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const VAR_DIR: &str = "var";
const BACKEND_PID_FILE: &str = "var/backend.pid";
const UI_PID_FILE: &str = "var/ui.pid";
const MENUBAR_PID_FILE: &str = "var/menu-bar.pid";
const STATUS_FILE: &str = "var/services.json";

#[derive(Debug, Clone, ValueEnum)]
enum ServiceKind {
    Backend,
    Ui,
    Menubar,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start one or more services
    Start {
        /// Service to start (default: backend)
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,
    },
    /// Stop one or more services
    Stop {
        /// Service to stop (default: backend)
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,
    },
    /// Restart a service
    Restart {
        /// Service to restart (default: backend)
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,
    },
    /// Show service status
    Status,
    /// Show recent logs for a service
    Logs {
        /// Service to show logs for
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,
    },
}

#[derive(Parser, Debug)]
#[command(name = "aos")]
#[command(about = "adapterOS local service & adapter runner", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to configuration file
    #[arg(long, global = true, default_value = "configs/aos.toml")]
    config: PathBuf,

    /// Output machine-readable JSON (for status and dry-run)
    #[arg(long, global = true)]
    json: bool,

    /// Do not perform side effects, just print intended actions
    #[arg(long, global = true)]
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct ServiceStatus {
    service: String,
    status: String,
    pid: Option<u32>,
}

#[derive(Debug, Serialize)]
struct StatusReport {
    ts: String,
    component: &'static str,
    services: Vec<ServiceStatus>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatusFileEntry {
    status: String,
    pid: Option<u32>,
    timestamp: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging()?;

    // Initialize deterministic config (env + manifest + CLI)
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let cli = Cli::parse();

    // Clone config path to avoid borrow checker issues when moving cli
    let config_path = cli.config.to_string_lossy().to_string();
    initialize_config(raw_args, Some(config_path))?;

    match run(cli).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!(component = "aos", error = %e, "Command failed");
            Err(e)
        }
    }
}

fn init_logging() -> Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .json()
        .with_current_span(false)
        .with_span_list(false);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    Ok(())
}

async fn run(cli: Cli) -> Result<()> {
    ensure_var_dir()?;

    match cli.command {
        Commands::Start { ref service } => start_service(service.clone(), &cli).await,
        Commands::Stop { ref service } => stop_service(service.clone(), &cli).await,
        Commands::Restart { ref service } => restart_service(service.clone(), &cli).await,
        Commands::Status => status(&cli).await,
        Commands::Logs { ref service } => logs(service.clone(), &cli).await,
    }
}

fn ensure_var_dir() -> Result<()> {
    fs::create_dir_all(VAR_DIR)
        .map_err(|e| AosError::Io(format!("Failed to create var directory {}: {}", VAR_DIR, e)))
}

fn pid_file_for(service: &ServiceKind) -> Option<&'static str> {
    match service {
        ServiceKind::Backend => Some(BACKEND_PID_FILE),
        ServiceKind::Ui => Some(UI_PID_FILE),
        ServiceKind::Menubar => Some(MENUBAR_PID_FILE),
    }
}

fn read_pid(path: &str) -> Option<u32> {
    let content = fs::read_to_string(path).ok()?;
    content.trim().parse::<u32>().ok()
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    use std::os::unix::prelude::RawFd;

    // Use kill(0) to check if the process exists
    unsafe {
        let pid_i32 = pid as RawFd;
        libc::kill(pid_i32, 0) == 0
    }
}

#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    // On non-Unix systems, conservatively assume the process may exist.
    true
}

fn service_name(service: &ServiceKind) -> &'static str {
    match service {
        ServiceKind::Backend => "backend",
        ServiceKind::Ui => "ui",
        ServiceKind::Menubar => "menu-bar",
    }
}

async fn start_service(service: ServiceKind, cli: &Cli) -> Result<()> {
    let name = service_name(&service);
    let pid_file = pid_file_for(&service)
        .ok_or_else(|| AosError::Config(format!("No PID file configured for service: {}", name)))?;

    let existing_pid = read_pid(pid_file);
    if let Some(pid) = existing_pid {
        if process_exists(pid) {
            info!(
                component = "aos",
                service = name,
                action = "start",
                result = "already-running",
                pid = pid,
                "Service already running"
            );
            if cli.json {
                print_json_status(&ServiceStatus {
                    service: name.to_string(),
                    status: "running".to_string(),
                    pid: Some(pid),
                })?;
            } else {
                println!("{} is already running (pid={})", name, pid);
            }
            return Ok(());
        }
    }

    if cli.dry_run {
        info!(
            component = "aos",
            service = name,
            action = "start",
            result = "dry-run",
            "Dry-run: would start service"
        );

        let status = ServiceStatus {
            service: name.to_string(),
            status: "would-start".to_string(),
            pid: None,
        };
        if cli.json {
            print_json_status(&status)?;
        } else {
            println!("Dry-run: would start {}", name);
        }
        return Ok(());
    }

    let result = match service {
        ServiceKind::Backend => start_backend(pid_file, cli).await,
        ServiceKind::Ui => start_ui(pid_file, cli).await,
        ServiceKind::Menubar => start_menubar(pid_file, cli).await,
    };

    if result.is_ok() {
        update_status_file(name, "running", read_pid(pid_file))?;
    }

    result
}

async fn stop_service(service: ServiceKind, cli: &Cli) -> Result<()> {
    let name = service_name(&service);
    let pid_file = pid_file_for(&service)
        .ok_or_else(|| AosError::Config(format!("No PID file configured for service: {}", name)))?;

    let existing_pid = read_pid(pid_file);
    if existing_pid.is_none() {
        if cli.json {
            print_json_status(&ServiceStatus {
                service: name.to_string(),
                status: "stopped".to_string(),
                pid: None,
            })?;
        } else {
            println!("{} is not running", name);
        }
        return Ok(());
    }
    let pid = existing_pid.unwrap();

    if cli.dry_run {
        info!(
            component = "aos",
            service = name,
            action = "stop",
            result = "dry-run",
            pid = pid,
            "Dry-run: would stop service"
        );
        if cli.json {
            print_json_status(&ServiceStatus {
                service: name.to_string(),
                status: "would-stop".to_string(),
                pid: Some(pid),
            })?;
        } else {
            println!("Dry-run: would stop {} (pid={})", name, pid);
        }
        return Ok(());
    }

    // Try graceful termination
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }

    // Best-effort: remove PID file regardless of outcome
    let _ = fs::remove_file(pid_file);

    // Update status file to reflect stopped state
    update_status_file(name, "stopped", None)?;

    info!(
        component = "aos",
        service = name,
        action = "stop",
        result = "requested",
        pid = pid,
        "Stop signal sent"
    );

    if cli.json {
        print_json_status(&ServiceStatus {
            service: name.to_string(),
            status: "stopping".to_string(),
            pid: Some(pid),
        })?;
    } else {
        println!("Stopping {} (pid={})", name, pid);
    }

    Ok(())
}

async fn restart_service(service: ServiceKind, cli: &Cli) -> Result<()> {
    stop_service(service.clone(), cli).await?;
    start_service(service, cli).await
}

async fn status(cli: &Cli) -> Result<()> {
    let services = vec![ServiceKind::Backend, ServiceKind::Ui, ServiceKind::Menubar];

    let mut statuses = Vec::new();

    for svc in services {
        let name = service_name(&svc).to_string();
        let pid_file = pid_file_for(&svc).unwrap();
        let pid = read_pid(pid_file);

        let (status, pid) = match pid {
            Some(pid_val) if process_exists(pid_val) => ("running".to_string(), Some(pid_val)),
            Some(_) => {
                // Stale PID file – remove it
                let _ = fs::remove_file(pid_file);
                ("stopped".to_string(), None)
            }
            None => ("stopped".to_string(), None),
        };

        statuses.push(ServiceStatus {
            service: name,
            status,
            pid,
        });
    }

    let ts = chrono::Utc::now().to_rfc3339();

    // Persist status snapshot to STATUS_FILE for compatibility with previous tooling.
    {
        let mut map = load_status_map()?;
        for svc in &statuses {
            let entry = StatusFileEntry {
                status: svc.status.clone(),
                pid: svc.pid,
                timestamp: ts.clone(),
            };
            map.insert(
                svc.service.clone(),
                serde_json::to_value(entry).map_err(|e| {
                    AosError::Config(format!("Failed to encode status entry: {}", e))
                })?,
            );
        }
        write_status_map(&map)?;
    }

    let report = StatusReport {
        ts,
        component: "aos",
        services: statuses,
    };

    if cli.json {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| AosError::Config(format!("Failed to serialize status as JSON: {}", e)))?;
        println!("{json}");
    } else {
        for svc in &report.services {
            match svc.pid {
                Some(pid) if svc.status == "running" => {
                    println!("{}: {} (pid={})", svc.service, svc.status, pid);
                }
                _ => {
                    println!("{}: {}", svc.service, svc.status);
                }
            }
        }
    }

    Ok(())
}

async fn logs(service: ServiceKind, cli: &Cli) -> Result<()> {
    let name = service_name(&service);
    let log_path = match service {
        ServiceKind::Backend => Path::new("server.log"),
        ServiceKind::Ui => Path::new("ui-dev.log"),
        ServiceKind::Menubar => Path::new("menu-bar.log"),
    };

    if !log_path.exists() {
        warn!(
            component = "aos",
            service = name,
            action = "logs",
            result = "missing",
            path = %log_path.display(),
            "Log file does not exist"
        );
        if !cli.json {
            println!("No logs found for {} ({})", name, log_path.display());
        }
        return Ok(());
    }

    let content = fs::read_to_string(log_path).map_err(|e| {
        AosError::Io(format!(
            "Failed to read log file {}: {}",
            log_path.display(),
            e
        ))
    })?;

    if cli.json {
        // Return lines as JSON array for machine consumption
        let lines: Vec<&str> = content.lines().collect();
        let json = serde_json::to_string_pretty(&lines)
            .map_err(|e| AosError::Config(format!("Failed to serialize logs as JSON: {}", e)))?;
        println!("{json}");
    } else {
        print!("{content}");
        io::stdout().flush().ok();
    }

    Ok(())
}

fn print_json_status(status: &ServiceStatus) -> Result<()> {
    let json = serde_json::to_string(status).map_err(|e| AosError::Config(format!("{}", e)))?;
    println!("{json}");
    Ok(())
}

fn load_status_map() -> Result<serde_json::Map<String, serde_json::Value>> {
    if !Path::new(STATUS_FILE).exists() {
        return Ok(serde_json::Map::new());
    }

    let content = fs::read_to_string(STATUS_FILE)
        .map_err(|e| AosError::Io(format!("Failed to read status file {}: {}", STATUS_FILE, e)))?;

    let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
        AosError::Config(format!(
            "Failed to parse status file {}: {}",
            STATUS_FILE, e
        ))
    })?;

    Ok(match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    })
}

fn write_status_map(map: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let value = serde_json::Value::Object(map.clone());
    let content = serde_json::to_string_pretty(&value).map_err(|e| {
        AosError::Config(format!(
            "Failed to serialize status file {}: {}",
            STATUS_FILE, e
        ))
    })?;

    fs::write(STATUS_FILE, content).map_err(|e| {
        AosError::Io(format!(
            "Failed to write status file {}: {}",
            STATUS_FILE, e
        ))
    })
}

fn update_status_file(service: &str, status: &str, pid: Option<u32>) -> Result<()> {
    let mut map = load_status_map()?;
    let entry = StatusFileEntry {
        status: status.to_string(),
        pid,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    map.insert(
        service.to_string(),
        serde_json::to_value(entry)
            .map_err(|e| AosError::Config(format!("Failed to encode status entry: {}", e)))?,
    );
    write_status_map(&map)
}

async fn start_backend(pid_file: &str, cli: &Cli) -> Result<()> {
    let binary = resolve_backend_binary();

    // Mirror common dev invocation: configs/cp.toml, skip PF check, single-writer.
    let args = vec![
        "--skip-pf-check".to_string(),
        "--config".to_string(),
        "configs/cp.toml".to_string(),
        "--single-writer".to_string(),
    ];

    let log_file = "server.log";
    let log = fs::File::create(log_file).map_err(|e| {
        AosError::Io(format!(
            "Failed to create backend log file {}: {}",
            log_file, e
        ))
    })?;
    let log_clone = log
        .try_clone()
        .map_err(|e| AosError::Io(format!("Failed to clone log file: {}", e)))?;

    let mut cmd = Command::new(&binary);
    cmd.args(&args)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_clone));

    let child = cmd.spawn().map_err(|e| {
        AosError::Io(format!(
            "Failed to spawn backend process ({}): {}",
            binary, e
        ))
    })?;

    let pid = child.id().unwrap_or(0);

    fs::write(pid_file, pid.to_string())
        .map_err(|e| AosError::Io(format!("Failed to write backend PID file: {}", e)))?;

    info!(
        component = "aos",
        service = "backend",
        action = "start",
        result = "started",
        pid = pid,
        binary = %binary,
        "Backend started"
    );

    if cli.json {
        print_json_status(&ServiceStatus {
            service: "backend".to_string(),
            status: "starting".to_string(),
            pid: Some(pid),
        })?;
    } else {
        println!("Starting backend (pid={})", pid);
    }

    // Wait for backend to be reachable on HTTP, mirroring the previous
    // launch script behavior. We log warnings rather than failing hard
    // if the process is still running but not yet responsive.
    if let Err(e) = wait_for_backend_ready(pid).await {
        warn!(
            component = "aos",
            service = "backend",
            action = "health-check",
            result = "timeout",
            error = %e,
            "Backend did not become ready within expected time window"
        );
    }

    Ok(())
}

async fn start_ui(pid_file: &str, cli: &Cli) -> Result<()> {
    // We assume pnpm is available, matching the previous shell implementation.
    let log_file = "ui-dev.log";
    let log = fs::File::create(log_file)
        .map_err(|e| AosError::Io(format!("Failed to create UI log file {}: {}", log_file, e)))?;
    let log_clone = log
        .try_clone()
        .map_err(|e| AosError::Io(format!("Failed to clone UI log file: {}", e)))?;

    // Respect AOS_UI_PORT for port offset strategy
    let ui_port = std::env::var("AOS_UI_PORT").unwrap_or_else(|_| "3200".to_string());
    let mut cmd = Command::new("pnpm");
    cmd.args(["dev", "--host", "0.0.0.0", "--port", &ui_port])
        .current_dir("ui")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_clone));

    let child = cmd
        .spawn()
        .map_err(|e| AosError::Io(format!("Failed to spawn UI dev server (pnpm dev): {}", e)))?;

    let pid = child.id().unwrap_or(0);

    fs::write(pid_file, pid.to_string())
        .map_err(|e| AosError::Io(format!("Failed to write UI PID file: {}", e)))?;

    info!(
        component = "aos",
        service = "ui",
        action = "start",
        result = "started",
        pid = pid,
        "UI dev server started"
    );

    if cli.json {
        print_json_status(&ServiceStatus {
            service: "ui".to_string(),
            status: "running".to_string(),
            pid: Some(pid),
        })?;
    } else {
        println!("Starting UI (pid={})", pid);
    }

    Ok(())
}

async fn start_menubar(pid_file: &str, cli: &Cli) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command as StdCommand;

        // Build menu bar app if needed (mirrors previous bash behavior)
        let binary_path = Path::new("menu-bar-app/.build/release/adapterOSMenu");
        if !binary_path.exists() {
            info!(
                component = "aos",
                service = "menu-bar",
                action = "build",
                result = "missing-binary",
                "Building menu bar app with swift"
            );

            let status = StdCommand::new("swift")
                .current_dir("menu-bar-app")
                .arg("build")
                .arg("-c")
                .arg("release")
                .status()
                .map_err(|e| {
                    AosError::Io(format!("Failed to run swift build for menu bar app: {}", e))
                })?;

            if !status.success() {
                return Err(AosError::Config(
                    "swift build -c release for menu bar app failed".to_string(),
                ));
            }
        }

        let log_file = "menu-bar.log";
        let log = fs::File::create(log_file).map_err(|e| {
            AosError::Io(format!(
                "Failed to create menu bar log file {}: {}",
                log_file, e
            ))
        })?;
        let log_clone = log
            .try_clone()
            .map_err(|e| AosError::Io(format!("Failed to clone menu bar log file: {}", e)))?;

        let mut cmd = Command::new(binary_path);
        cmd.current_dir("menu-bar-app")
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_clone));

        let child = cmd.spawn().map_err(|e| {
            AosError::Io(format!(
                "Failed to spawn menu bar app ({}): {}",
                binary_path.display(),
                e
            ))
        })?;

        let pid = child.id().unwrap_or(0);

        fs::write(pid_file, pid.to_string())
            .map_err(|e| AosError::Io(format!("Failed to write menu bar PID file: {}", e)))?;

        info!(
            component = "aos",
            service = "menu-bar",
            action = "start",
            result = "started",
            pid = pid,
            "Menu bar app started"
        );

        if cli.json {
            print_json_status(&ServiceStatus {
                service: "menu-bar".to_string(),
                status: "running".to_string(),
                pid: Some(pid),
            })?;
        } else {
            println!("Starting menu bar app (pid={})", pid);
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(AosError::Config(
            "Menu bar app is only available on macOS".to_string(),
        ))
    }
}

fn resolve_backend_binary() -> String {
    let candidates = [
        "target/debug/adapteros-server",
        "target/release/adapteros-server",
        "adapteros-server",
    ];

    for candidate in candidates {
        if Path::new(candidate).exists() {
            return candidate.to_string();
        }
    }

    // Fall back to system PATH; if that fails at spawn time, the error
    // will include this binary name for troubleshooting.
    "adapteros-server".to_string()
}

async fn wait_for_backend_ready(pid: u32) -> Result<()> {
    let client = reqwest::Client::new();
    let urls = [
        "http://127.0.0.1:3300/v1/meta",
        "http://127.0.0.1:3300/healthz",
    ];

    let max_attempts = 30;
    for attempt in 1..=max_attempts {
        // If process has exited, stop waiting and return an error.
        if !process_exists(pid) {
            return Err(AosError::Config(format!(
                "Backend process (pid={}) exited during startup",
                pid
            )));
        }

        for url in &urls {
            match client.get(*url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!(
                        component = "aos",
                        service = "backend",
                        action = "health-check",
                        result = "ready",
                        url = *url,
                        attempt = attempt,
                        "Backend is responding on HTTP"
                    );
                    return Ok(());
                }
                _ => continue,
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    Err(AosError::Config(
        "Backend failed to become ready within timeout".to_string(),
    ))
}
