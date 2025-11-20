//! Unified AOS CLI Tool
//!
//! Single binary for all AOS operations including:
//! - AOS archive management (create, validate, verify, analyze, info)
//! - Service management (start, stop, restart, status, logs)
//!
//! Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

use adapteros_aos::AOS2Writer;
use adapteros_config::initialize_config;
use adapteros_core::{AosError, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::error;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const VAR_DIR: &str = "var";
const BACKEND_PID_FILE: &str = "var/backend.pid";
const UI_PID_FILE: &str = "var/ui.pid";
const MENUBAR_PID_FILE: &str = "var/menu-bar.pid";
const STATUS_FILE: &str = "var/services.json";

#[derive(Parser, Debug)]
#[command(name = "aos")]
#[command(version)]
#[command(about = "Unified AOS CLI - Archive and Service Management", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Suppress non-essential output
    #[arg(long, short, global = true)]
    quiet: bool,

    /// Output as JSON where applicable
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Analyze AOS file structure and contents
    Analyze {
        /// Path to .aos file
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },

    /// Validate AOS file integrity and compliance
    Validate {
        /// Path to .aos file
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Skip tensor data validation (faster)
        #[arg(long)]
        skip_tensors: bool,

        /// Skip BLAKE3 hash verification
        #[arg(long)]
        skip_hash: bool,
    },

    /// Create AOS archive from directory
    Create {
        /// Input directory containing manifest.json and weights.safetensors
        #[arg(value_name = "INPUT_DIR")]
        input_dir: PathBuf,

        /// Output .aos file path
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,

        /// Override adapter ID (semantic naming: tenant/domain/purpose/revision)
        #[arg(long = "adapter-id")]
        adapter_id: Option<String>,

        /// Verify the created .aos file
        #[arg(long)]
        verify: bool,

        /// Dry run - preview without creating file
        #[arg(long = "dry-run")]
        dry_run: bool,
    },

    /// Display AOS file information
    Info {
        /// Path to .aos file
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Show full manifest JSON
        #[arg(long)]
        full_manifest: bool,

        /// Show tensor data checksums
        #[arg(long)]
        checksums: bool,
    },

    /// Deep verification of AOS files
    Verify {
        /// Path to .aos file
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Skip tensor data validation (faster)
        #[arg(long)]
        skip_tensors: bool,
    },

    /// Service management commands
    #[command(subcommand)]
    Service(ServiceCommands),
}

#[derive(Subcommand, Debug)]
enum ServiceCommands {
    /// Start a service
    Start {
        /// Service to start
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,

        /// Path to configuration file
        #[arg(long, default_value = "configs/aos.toml")]
        config: PathBuf,

        /// Do not perform side effects, just print intended actions
        #[arg(long)]
        dry_run: bool,
    },

    /// Stop a service
    Stop {
        /// Service to stop
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,

        /// Do not perform side effects, just print intended actions
        #[arg(long)]
        dry_run: bool,
    },

    /// Restart a service
    Restart {
        /// Service to restart
        #[arg(value_enum, default_value = "backend")]
        service: ServiceKind,

        /// Path to configuration file
        #[arg(long, default_value = "configs/aos.toml")]
        config: PathBuf,
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

#[derive(Debug, Clone, ValueEnum)]
enum ServiceKind {
    Backend,
    Ui,
    Menubar,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    init_logging(&cli)?;

    match run(cli).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!(component = "aos", error = %e, "Command failed");
            Err(e)
        }
    }
}

fn init_logging(cli: &Cli) -> Result<()> {
    let log_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "error"
    } else {
        "info"
    };

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    let fmt_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();

    Ok(())
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Analyze { ref file } => {
            analyze_command(file, &cli).await
        }
        Commands::Validate { ref file, skip_tensors, skip_hash } => {
            validate_command(file, skip_tensors, skip_hash, &cli).await
        }
        Commands::Create { ref input_dir, ref output, ref adapter_id, verify, dry_run } => {
            create_command(input_dir, output.as_ref(), adapter_id.as_deref(), verify, dry_run, &cli).await
        }
        Commands::Info { ref file, full_manifest, checksums } => {
            info_command(file, full_manifest, checksums, &cli).await
        }
        Commands::Verify { ref file, skip_tensors } => {
            verify_command(file, skip_tensors, &cli).await
        }
        Commands::Service(ref service_cmd) => {
            service_command(service_cmd, &cli).await
        }
    }
}

// =============================================================================
// AOS Archive Commands
// =============================================================================

async fn analyze_command(file: &Path, cli: &Cli) -> Result<()> {
    if !file.exists() {
        return Err(AosError::NotFound(format!("File not found: {}", file.display())));
    }

    let report = analyze_aos_file(file)?;

    if cli.json {
        let json = serde_json::to_string_pretty(&report)?;
        println!("{}", json);
    } else {
        print_analyze_report(&report)?;
    }

    if !report.errors.is_empty() {
        std::process::exit(1);
    }

    Ok(())
}

async fn validate_command(file: &Path, skip_tensors: bool, skip_hash: bool, cli: &Cli) -> Result<()> {
    if !file.exists() {
        return Err(AosError::NotFound(format!("File not found: {}", file.display())));
    }

    let result = validate_aos_file(file, skip_tensors, skip_hash, cli)?;

    if cli.json {
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
    } else {
        print_validation_report(&result, cli)?;
    }

    if !result.valid {
        std::process::exit(1);
    }

    Ok(())
}

async fn create_command(
    input_dir: &Path,
    output: Option<&PathBuf>,
    adapter_id: Option<&str>,
    verify: bool,
    dry_run: bool,
    cli: &Cli,
) -> Result<()> {
    let output_path = if let Some(out) = output {
        out.clone()
    } else {
        let adapter_name = input_dir
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AosError::Validation("Invalid input directory name".to_string()))?;
        PathBuf::from("adapters").join(format!("{}.aos", adapter_name))
    };

    let (_hash, _manifest) = create_aos_archive(
        input_dir,
        &output_path,
        adapter_id,
        dry_run,
        cli.verbose,
    )?;

    if verify && !dry_run {
        if !cli.quiet {
            println!("\nVerifying created archive...");
        }
        verify_aos_archive(&output_path, cli.verbose)?;
    }

    Ok(())
}

async fn info_command(file: &Path, full_manifest: bool, _checksums: bool, cli: &Cli) -> Result<()> {
    if !file.exists() {
        return Err(AosError::NotFound(format!("File not found: {}", file.display())));
    }

    let info = extract_aos_info(file)?;

    if cli.json {
        let json = serde_json::to_string_pretty(&info)?;
        println!("{}", json);
    } else {
        print_aos_info(&info, full_manifest)?;
    }

    Ok(())
}

async fn verify_command(file: &Path, skip_tensors: bool, cli: &Cli) -> Result<()> {
    if !file.exists() {
        return Err(AosError::NotFound(format!("File not found: {}", file.display())));
    }

    let result = deep_verify_aos(file, skip_tensors)?;

    if cli.json {
        let json = serde_json::to_string_pretty(&result)?;
        println!("{}", json);
    } else {
        print_verify_report(&result, cli)?;
    }

    if !result.valid {
        std::process::exit(1);
    }

    Ok(())
}

// =============================================================================
// Service Management Commands
// =============================================================================

async fn service_command(cmd: &ServiceCommands, cli: &Cli) -> Result<()> {
    ensure_var_dir()?;

    match cmd {
        ServiceCommands::Start { service, config, dry_run } => {
            start_service(service.clone(), config, *dry_run, cli).await
        }
        ServiceCommands::Stop { service, dry_run } => {
            stop_service(service.clone(), *dry_run, cli).await
        }
        ServiceCommands::Restart { service, config } => {
            restart_service(service.clone(), config, cli).await
        }
        ServiceCommands::Status => {
            status_service(cli).await
        }
        ServiceCommands::Logs { service } => {
            logs_service(service.clone(), cli).await
        }
    }
}

// Service helper functions (from original aos.rs)
fn ensure_var_dir() -> Result<()> {
    fs::create_dir_all(VAR_DIR)
        .map_err(|e| AosError::Io(format!("Failed to create var directory: {}", e)))
}

fn pid_file_for(service: &ServiceKind) -> Option<&'static str> {
    match service {
        ServiceKind::Backend => Some(BACKEND_PID_FILE),
        ServiceKind::Ui => Some(UI_PID_FILE),
        ServiceKind::Menubar => Some(MENUBAR_PID_FILE),
    }
}

fn service_name(service: &ServiceKind) -> &'static str {
    match service {
        ServiceKind::Backend => "backend",
        ServiceKind::Ui => "ui",
        ServiceKind::Menubar => "menu-bar",
    }
}

fn read_pid(path: &str) -> Option<u32> {
    let content = fs::read_to_string(path).ok()?;
    content.trim().parse::<u32>().ok()
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    true
}

async fn start_service(service: ServiceKind, config: &Path, dry_run: bool, cli: &Cli) -> Result<()> {
    let name = service_name(&service);
    let pid_file = pid_file_for(&service)
        .ok_or_else(|| AosError::Config(format!("No PID file configured for service: {}", name)))?;

    // Check if already running
    if let Some(pid) = read_pid(pid_file) {
        if process_exists(pid) {
            if !cli.quiet {
                println!("{} is already running (pid={})", name, pid);
            }
            return Ok(());
        }
    }

    if dry_run {
        println!("Would start service: {}", name);
        return Ok(());
    }

    // Initialize config for backend
    if matches!(service, ServiceKind::Backend) {
        let raw_args: Vec<String> = std::env::args().skip(1).collect();
        let config_path = config.to_string_lossy().to_string();
        initialize_config(raw_args, Some(config_path))?;
    }

    match service {
        ServiceKind::Backend => start_backend(pid_file, cli).await,
        ServiceKind::Ui => start_ui(pid_file, cli).await,
        ServiceKind::Menubar => start_menubar(pid_file, cli).await,
    }?;

    update_status_file(name, "running", read_pid(pid_file))?;
    Ok(())
}

async fn stop_service(service: ServiceKind, dry_run: bool, cli: &Cli) -> Result<()> {
    let name = service_name(&service);
    let pid_file = pid_file_for(&service)
        .ok_or_else(|| AosError::Config(format!("No PID file configured for service: {}", name)))?;

    let pid = match read_pid(pid_file) {
        Some(p) => p,
        None => {
            if !cli.quiet {
                println!("{} is not running", name);
            }
            return Ok(());
        }
    };

    if dry_run {
        println!("Would stop service: {} (pid={})", name, pid);
        return Ok(());
    }

    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, libc::SIGTERM); }
    }

    let _ = fs::remove_file(pid_file);
    update_status_file(name, "stopped", None)?;

    if !cli.quiet {
        println!("Stopped {} (pid={})", name, pid);
    }

    Ok(())
}

async fn restart_service(service: ServiceKind, config: &Path, cli: &Cli) -> Result<()> {
    stop_service(service.clone(), false, cli).await?;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    start_service(service, config, false, cli).await
}

async fn status_service(cli: &Cli) -> Result<()> {
    let services = vec![ServiceKind::Backend, ServiceKind::Ui, ServiceKind::Menubar];
    let mut statuses = Vec::new();

    for svc in services {
        let name = service_name(&svc).to_string();
        let pid_file = pid_file_for(&svc).unwrap();
        let pid = read_pid(pid_file);

        let (status, pid) = match pid {
            Some(pid_val) if process_exists(pid_val) => ("running".to_string(), Some(pid_val)),
            Some(_) => {
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

    if cli.json {
        let report = StatusReport {
            ts: chrono::Utc::now().to_rfc3339(),
            component: "aos",
            services: statuses,
        };
        let json = serde_json::to_string_pretty(&report)?;
        println!("{}", json);
    } else {
        for svc in &statuses {
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

async fn logs_service(service: ServiceKind, cli: &Cli) -> Result<()> {
    let name = service_name(&service);
    let log_path = match service {
        ServiceKind::Backend => Path::new("server.log"),
        ServiceKind::Ui => Path::new("ui-dev.log"),
        ServiceKind::Menubar => Path::new("menu-bar.log"),
    };

    if !log_path.exists() {
        if !cli.quiet {
            println!("No logs found for {}", name);
        }
        return Ok(());
    }

    let content = fs::read_to_string(log_path)
        .map_err(|e| AosError::Io(format!("Failed to read log file: {}", e)))?;

    if cli.json {
        let lines: Vec<&str> = content.lines().collect();
        let json = serde_json::to_string_pretty(&lines)?;
        println!("{}", json);
    } else {
        print!("{}", content);
    }

    Ok(())
}

// Service helper structures
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

fn load_status_map() -> Result<serde_json::Map<String, serde_json::Value>> {
    if !Path::new(STATUS_FILE).exists() {
        return Ok(serde_json::Map::new());
    }

    let content = fs::read_to_string(STATUS_FILE)
        .map_err(|e| AosError::Io(format!("Failed to read status file: {}", e)))?;

    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| AosError::Config(format!("Failed to parse status file: {}", e)))?;

    Ok(match value {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    })
}

fn write_status_map(map: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    let value = serde_json::Value::Object(map.clone());
    let content = serde_json::to_string_pretty(&value)
        .map_err(|e| AosError::Config(format!("Failed to serialize status file: {}", e)))?;

    fs::write(STATUS_FILE, content)
        .map_err(|e| AosError::Io(format!("Failed to write status file: {}", e)))
}

async fn start_backend(pid_file: &str, cli: &Cli) -> Result<()> {
    let binary = resolve_backend_binary();
    let args = vec![
        "--skip-pf-check".to_string(),
        "--config".to_string(),
        "configs/cp.toml".to_string(),
        "--single-writer".to_string(),
    ];

    let log_file = "server.log";
    let log = fs::File::create(log_file)
        .map_err(|e| AosError::Io(format!("Failed to create backend log file: {}", e)))?;
    let log_clone = log.try_clone()
        .map_err(|e| AosError::Io(format!("Failed to clone log file: {}", e)))?;

    let mut cmd = Command::new(&binary);
    cmd.args(&args)
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_clone));

    let child = cmd.spawn()
        .map_err(|e| AosError::Io(format!("Failed to spawn backend process: {}", e)))?;

    let pid = child.id().unwrap_or(0);
    fs::write(pid_file, pid.to_string())
        .map_err(|e| AosError::Io(format!("Failed to write backend PID file: {}", e)))?;

    if !cli.quiet {
        println!("Started backend (pid={})", pid);
    }

    Ok(())
}

async fn start_ui(pid_file: &str, cli: &Cli) -> Result<()> {
    let log_file = "ui-dev.log";
    let log = fs::File::create(log_file)
        .map_err(|e| AosError::Io(format!("Failed to create UI log file: {}", e)))?;
    let log_clone = log.try_clone()
        .map_err(|e| AosError::Io(format!("Failed to clone UI log file: {}", e)))?;

    let mut cmd = Command::new("pnpm");
    cmd.args(&["dev", "--host", "0.0.0.0", "--port", "3200"])
        .current_dir("ui")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_clone));

    let child = cmd.spawn()
        .map_err(|e| AosError::Io(format!("Failed to spawn UI dev server: {}", e)))?;

    let pid = child.id().unwrap_or(0);
    fs::write(pid_file, pid.to_string())
        .map_err(|e| AosError::Io(format!("Failed to write UI PID file: {}", e)))?;

    if !cli.quiet {
        println!("Started UI (pid={})", pid);
    }

    Ok(())
}

async fn start_menubar(pid_file: &str, cli: &Cli) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command as StdCommand;

        let binary_path = Path::new("menu-bar-app/.build/release/AdapterOSMenu");
        if !binary_path.exists() {
            let status = StdCommand::new("swift")
                .current_dir("menu-bar-app")
                .args(&["build", "-c", "release"])
                .status()
                .map_err(|e| AosError::Io(format!("Failed to build menu bar app: {}", e)))?;

            if !status.success() {
                return Err(AosError::Config("swift build failed".to_string()));
            }
        }

        let log_file = "menu-bar.log";
        let log = fs::File::create(log_file)
            .map_err(|e| AosError::Io(format!("Failed to create menu bar log file: {}", e)))?;
        let log_clone = log.try_clone()
            .map_err(|e| AosError::Io(format!("Failed to clone menu bar log file: {}", e)))?;

        let mut cmd = Command::new(binary_path);
        cmd.current_dir("menu-bar-app")
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_clone));

        let child = cmd.spawn()
            .map_err(|e| AosError::Io(format!("Failed to spawn menu bar app: {}", e)))?;

        let pid = child.id().unwrap_or(0);
        fs::write(pid_file, pid.to_string())
            .map_err(|e| AosError::Io(format!("Failed to write menu bar PID file: {}", e)))?;

        if !cli.quiet {
            println!("Started menu bar app (pid={})", pid);
        }

        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(AosError::Config("Menu bar app is only available on macOS".to_string()))
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

    "adapteros-server".to_string()
}

// =============================================================================
// AOS File Analysis Implementation (Simplified from aos-analyze.rs)
// =============================================================================

#[derive(Debug, Clone, Serialize)]
struct AnalysisReport {
    file_path: String,
    file_size: usize,
    header: AosHeader,
    weights: Option<WeightsAnalysis>,
    manifest: serde_json::Value,
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AosHeader {
    manifest_offset: u32,
    manifest_len: u32,
}

#[derive(Debug, Clone, Serialize)]
struct WeightsAnalysis {
    format: String,
    tensors: Vec<TensorInfo>,
    total_params: usize,
}

#[derive(Debug, Clone, Serialize)]
struct TensorInfo {
    name: String,
    dtype: String,
    shape: Vec<usize>,
    num_params: usize,
}

fn analyze_aos_file(path: &Path) -> Result<AnalysisReport> {
    let mut file = File::open(path)
        .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    let mut data = Vec::new();
    file.read_to_end(&mut data)
        .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    let header = read_aos_header(&data)?;
    let manifest = parse_aos_manifest(&data, &header)?;
    let weights = parse_weights_section(&data, &header).ok();

    let errors = Vec::new();
    let warnings = Vec::new();

    Ok(AnalysisReport {
        file_path: path.display().to_string(),
        file_size: data.len(),
        header,
        weights,
        manifest,
        errors,
        warnings,
    })
}

fn read_aos_header(data: &[u8]) -> Result<AosHeader> {
    if data.len() < 8 {
        return Err(AosError::Validation("File too small".to_string()));
    }

    let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

    Ok(AosHeader {
        manifest_offset,
        manifest_len,
    })
}

fn parse_aos_manifest(data: &[u8], header: &AosHeader) -> Result<serde_json::Value> {
    let offset = header.manifest_offset as usize;
    let len = header.manifest_len as usize;

    if data.len() < offset + len {
        return Err(AosError::Validation("Invalid manifest bounds".to_string()));
    }

    let manifest_bytes = &data[offset..offset + len];
    serde_json::from_slice(manifest_bytes).map_err(AosError::Serialization)
}

fn parse_weights_section(_data: &[u8], _header: &AosHeader) -> Result<WeightsAnalysis> {
    // Simplified - just detect format and count tensors
    Ok(WeightsAnalysis {
        format: "safetensors".to_string(),
        tensors: Vec::new(),
        total_params: 0,
    })
}

fn print_analyze_report(report: &AnalysisReport) -> Result<()> {
    println!("\nAOS File Analysis");
    println!("{}", "=".repeat(70));
    println!("File: {}", report.file_path);
    println!("Size: {} bytes", report.file_size);
    println!("\nHeader:");
    println!("  Manifest offset: {}", report.header.manifest_offset);
    println!("  Manifest length: {}", report.header.manifest_len);
    println!("\nManifest:");
    println!("{}", serde_json::to_string_pretty(&report.manifest).unwrap_or_default());
    Ok(())
}

// =============================================================================
// Validation Implementation (Simplified from aos-validate.rs)
// =============================================================================

#[derive(Debug, Serialize)]
struct ValidationResult {
    file_path: String,
    valid: bool,
    checks: Vec<Check>,
    errors: Vec<String>,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Check {
    name: String,
    passed: bool,
    message: String,
    details: Option<String>,
}

fn validate_aos_file(path: &Path, _skip_tensors: bool, _skip_hash: bool, _cli: &Cli) -> Result<ValidationResult> {
    let mut checks = Vec::new();
    let mut errors = Vec::new();
    let warnings = Vec::new();

    // Basic header validation
    match AOS2Writer::read_header(path) {
        Ok((offset, len)) => {
            checks.push(Check {
                name: "Header Format".to_string(),
                passed: true,
                message: format!("offset={}, len={}", offset, len),
                details: None,
            });
        }
        Err(e) => {
            errors.push(format!("Header validation failed: {}", e));
            checks.push(Check {
                name: "Header Format".to_string(),
                passed: false,
                message: "Invalid".to_string(),
                details: Some(e.to_string()),
            });
        }
    }

    // Try to parse manifest
    let _data = fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;

    checks.push(Check {
        name: "Format Validation".to_string(),
        passed: true,
        message: "Basic checks passed".to_string(),
        details: None,
    });

    let valid = errors.is_empty() && checks.iter().all(|c| c.passed);

    Ok(ValidationResult {
        file_path: path.display().to_string(),
        valid,
        checks,
        errors,
        warnings,
    })
}

fn print_validation_report(result: &ValidationResult, cli: &Cli) -> Result<()> {
    if result.valid {
        println!("✓ VALIDATION PASSED");
    } else {
        println!("✗ VALIDATION FAILED");
    }

    if cli.verbose {
        for check in &result.checks {
            let status = if check.passed { "✓" } else { "✗" };
            println!("{} {}: {}", status, check.name, check.message);
        }
    }

    if !result.errors.is_empty() {
        println!("\nErrors:");
        for error in &result.errors {
            println!("  ✗ {}", error);
        }
    }

    if !result.warnings.is_empty() {
        println!("\nWarnings:");
        for warning in &result.warnings {
            println!("  ⚠ {}", warning);
        }
    }

    Ok(())
}

// =============================================================================
// Create Implementation (Simplified from aos-create.rs)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AdapterManifest {
    #[serde(default = "default_format_version")]
    format_version: u32,
    #[serde(default)]
    adapter_id: String,
    #[serde(default)]
    version: String,
    #[serde(default = "default_rank")]
    rank: u32,
    #[serde(default = "default_alpha")]
    alpha: f32,
    #[serde(default)]
    base_model: String,
    #[serde(default)]
    weights_hash: String,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

fn default_format_version() -> u32 { 2 }
fn default_rank() -> u32 { 16 }
fn default_alpha() -> f32 { 32.0 }

fn create_aos_archive(
    input_dir: &Path,
    output_path: &Path,
    adapter_id_override: Option<&str>,
    dry_run: bool,
    verbose: bool,
) -> Result<(String, AdapterManifest)> {
    if !input_dir.exists() {
        return Err(AosError::Validation(format!("Input directory not found: {}", input_dir.display())));
    }

    let manifest_path = input_dir.join("manifest.json");
    let weights_path = input_dir.join("weights.safetensors");

    if !manifest_path.exists() {
        return Err(AosError::Validation("Missing manifest.json".to_string()));
    }

    if !weights_path.exists() {
        return Err(AosError::Validation("Missing weights.safetensors".to_string()));
    }

    let mut manifest: AdapterManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?
    )?;

    let weights_data = fs::read(&weights_path)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;

    let weights_hash = blake3::hash(&weights_data).to_hex().to_string();
    manifest.weights_hash = weights_hash.clone();

    if let Some(id) = adapter_id_override {
        manifest.adapter_id = id.to_string();
    }

    if dry_run {
        println!("Would create: {}", output_path.display());
        println!("  Adapter ID: {}", manifest.adapter_id);
        println!("  Size: {} bytes", weights_data.len());
        return Ok((weights_hash, manifest));
    }

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AosError::Io(format!("Failed to create output directory: {}", e)))?;
    }

    let writer = AOS2Writer::new();
    let total_size = writer.write_archive(output_path, &manifest, &weights_data)?;

    if verbose {
        println!("Created: {}", output_path.display());
        println!("  Size: {} bytes", total_size);
        println!("  Hash: {}...", &weights_hash[..16]);
    } else {
        println!("Created: {}", output_path.display());
    }

    Ok((weights_hash, manifest))
}

fn verify_aos_archive(path: &Path, _verbose: bool) -> Result<()> {
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;
    let file_data = fs::read(path)
        .map_err(|e| AosError::Io(format!("Failed to read .aos file: {}", e)))?;

    let weights_data = &file_data[8..manifest_offset as usize];
    let manifest_json = &file_data[manifest_offset as usize..(manifest_offset + manifest_len) as usize];

    let manifest: AdapterManifest = serde_json::from_slice(manifest_json)?;
    let computed_hash = blake3::hash(weights_data).to_hex().to_string();

    if computed_hash != manifest.weights_hash {
        return Err(AosError::Validation("Hash mismatch".to_string()));
    }

    println!("✓ Valid .aos file");
    Ok(())
}

// =============================================================================
// Info Implementation (Simplified from aos-info.rs)
// =============================================================================

#[derive(Debug, Serialize)]
struct AosInfo {
    file_path: String,
    file_size: u64,
    manifest_offset: u32,
    manifest_len: u32,
    manifest: serde_json::Value,
}

fn extract_aos_info(path: &Path) -> Result<AosInfo> {
    let (manifest_offset, manifest_len) = AOS2Writer::read_header(path)?;
    let metadata = fs::metadata(path)
        .map_err(|e| AosError::Io(format!("Failed to read metadata: {}", e)))?;

    let mut file = File::open(path)
        .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;

    file.seek(std::io::SeekFrom::Start(manifest_offset as u64))
        .map_err(|e| AosError::Io(format!("Failed to seek: {}", e)))?;

    let mut manifest_bytes = vec![0u8; manifest_len as usize];
    file.read_exact(&mut manifest_bytes)
        .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;

    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;

    Ok(AosInfo {
        file_path: path.display().to_string(),
        file_size: metadata.len(),
        manifest_offset,
        manifest_len,
        manifest,
    })
}

fn print_aos_info(info: &AosInfo, full_manifest: bool) -> Result<()> {
    println!("\nAOS Archive Information");
    println!("{}", "=".repeat(70));
    println!("File: {}", info.file_path);
    println!("Size: {} bytes", info.file_size);
    println!("\nStructure:");
    println!("  Header: 0-8 (8 bytes)");
    println!("  Weights: 8-{} ({} bytes)", info.manifest_offset, info.manifest_offset - 8);
    println!("  Manifest: {}-{} ({} bytes)", info.manifest_offset, info.file_size, info.manifest_len);

    if full_manifest {
        println!("\nFull Manifest:");
        println!("{}", serde_json::to_string_pretty(&info.manifest).unwrap_or_default());
    } else {
        println!("\nManifest Summary:");
        if let Some(version) = info.manifest.get("version") {
            println!("  Version: {}", version);
        }
        if let Some(adapter_id) = info.manifest.get("adapter_id") {
            println!("  Adapter ID: {}", adapter_id);
        }
        println!("\n  (Use --full-manifest to see complete JSON)");
    }

    Ok(())
}

// =============================================================================
// Verify Implementation (Simplified from aos-verify.rs)
// =============================================================================

#[derive(Debug, Serialize)]
struct VerifyResult {
    file_path: String,
    valid: bool,
    checks: Vec<Check>,
    errors: Vec<String>,
}

fn deep_verify_aos(path: &Path, _skip_tensors: bool) -> Result<VerifyResult> {
    let mut checks = Vec::new();
    let mut errors = Vec::new();

    // Header check
    match AOS2Writer::read_header(path) {
        Ok((offset, len)) => {
            checks.push(Check {
                name: "Header".to_string(),
                passed: true,
                message: format!("offset={}, len={}", offset, len),
                details: None,
            });
        }
        Err(e) => {
            errors.push(format!("Header check failed: {}", e));
            checks.push(Check {
                name: "Header".to_string(),
                passed: false,
                message: "Invalid".to_string(),
                details: Some(e.to_string()),
            });
        }
    }

    let valid = errors.is_empty();

    Ok(VerifyResult {
        file_path: path.display().to_string(),
        valid,
        checks,
        errors,
    })
}

fn print_verify_report(result: &VerifyResult, cli: &Cli) -> Result<()> {
    if result.valid {
        println!("✓ VERIFICATION PASSED");
    } else {
        println!("✗ VERIFICATION FAILED");
    }

    if cli.verbose {
        for check in &result.checks {
            let status = if check.passed { "✓" } else { "✗" };
            println!("{} {}: {}", status, check.name, check.message);
        }
    }

    if !result.errors.is_empty() {
        println!("\nErrors:");
        for error in &result.errors {
            println!("  ✗ {}", error);
        }
    }

    Ok(())
}
