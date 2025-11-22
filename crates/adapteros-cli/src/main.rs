//! AdapterOS CLI tool (aosctl)

#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(noop_method_call)]
#![allow(clippy::unneeded_struct_pattern)]
#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_must_use)]
#![allow(private_interfaces)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::redundant_closure)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::to_string_in_format_args)]
#![allow(dead_code)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::unnecessary_map_or)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::type_complexity)]
#![allow(clippy::useless_format)]
#![allow(clippy::len_zero)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::useless_asref)]
#![allow(clippy::wildcard_in_or_patterns)]
#![allow(clippy::suspicious_doc_comments)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::single_match)]

use adapteros_config::{BackendPreference, ModelConfig};
use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

mod cli;
mod cli_telemetry;
mod commands;
mod error_codes;
mod logging;
mod output;

use adapteros_lora_worker::memory::{MemoryPressureLevel, UmaPressureMonitor};
use commands::golden::GoldenCmd;
use commands::*;
use logging::init_logging;
use output::{OutputMode, OutputWriter};

/// Backend type selection for inference
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BackendType {
    /// Metal backend (macOS GPU)
    Metal,
    /// MLX backend (Python/MLX)
    Mlx,
    /// CoreML backend (macOS Neural Engine)
    CoreML,
}

#[derive(Parser)]
#[command(name = "aosctl")]
#[command(about = "AdapterOS command-line interface", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output in JSON format
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential output
    #[arg(long, short = 'q', global = true)]
    quiet: bool,

    /// Enable verbose output
    #[arg(long, short = 'v', global = true)]
    verbose: bool,

    /// Model path (overrides AOS_MODEL_PATH env var)
    #[arg(long, global = true, env = "AOS_MODEL_PATH")]
    pub model_path: Option<String>,

    /// Model backend preference (overrides AOS_MODEL_BACKEND env var)
    /// Values: auto, coreml, metal, mlx
    #[arg(long, global = true, env = "AOS_MODEL_BACKEND", default_value = "auto")]
    pub model_backend: String,
}

impl Cli {
    /// Build a ModelConfig from CLI arguments with precedence: CLI > ENV > defaults
    pub fn get_model_config(&self) -> Result<ModelConfig> {
        // Start with environment-based config (or defaults)
        let mut config = ModelConfig::from_env().map_err(|e| anyhow::anyhow!("{}", e))?;

        // Override with CLI args if provided
        if let Some(ref path) = self.model_path {
            config.path = PathBuf::from(path);
        }

        // Parse backend preference from CLI
        config.backend = self
            .model_backend
            .parse::<BackendPreference>()
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        Ok(config)
    }
}

#[derive(Subcommand)]
enum Commands {
    // ============================================================
    // Tenant Management
    // ============================================================
    /// Initialize a new tenant
    #[command(after_help = "\
Examples:
  # Create a development tenant
  aosctl init-tenant --id tenant_dev --uid 1000 --gid 1000

  # Create a production tenant with custom IDs
  aosctl init-tenant --id tenant_prod --uid 5000 --gid 5000

  # Quick alias (hidden)
  aosctl init --id tenant_test --uid 1000 --gid 1000
")]
    TenantInit {
        /// Tenant ID
        #[arg(short, long)]
        id: String,

        /// Unix UID
        #[arg(short, long)]
        uid: u32,

        /// Unix GID
        #[arg(short, long)]
        gid: u32,
    },

    // ============================================================
    // Adapter Management
    // ============================================================
    /// Adapter management commands (lifecycle, registration, pinning, etc.)
    #[command(subcommand, visible_alias = "adapters")]
    Adapter(adapter::AdapterCommand),

    // ============================================================
    // Node & Cluster Management
    // ============================================================
    /// Node management commands (list, verify, sync)
    #[command(subcommand, visible_alias = "nodes")]
    Node(node::NodeCommand),

    // ============================================================
    // System Status
    // ============================================================
    /// Show system status (adapters, cluster, tick, memory)
    Status(commands::status::StatusCommand),

    /// Run system health diagnostics (PRD-06)
    #[command(after_help = "\
Examples:
  # Run comprehensive health check
  aosctl doctor

  # Check health with custom server URL
  aosctl doctor --server-url http://localhost:8080

  # Check health with custom timeout
  aosctl doctor --timeout 30
")]
    Doctor(commands::doctor::DoctorCommand),

    // ============================================================
    // Maintenance
    // ============================================================
    /// Maintenance operations (GC, sweeps, etc.)
    Maintenance(commands::maintenance::MaintenanceCommand),

    // ============================================================
    // Deployment
    // ============================================================
    /// Deployment workflows (adapters, etc.)
    Deploy(commands::deploy::DeployCommand),

    // ============================================================
    // Registry Management
    // ============================================================
    /// Registry management commands (sync, migrate)
    #[command(subcommand, alias = "sync-registry", alias = "registry-sync", alias = "registry-migrate")]
    Registry(registry::RegistryCommand),

    // ============================================================
    // Plan Management
    // ============================================================
    /// Build a plan from manifest
    #[command(after_help = "\
Examples:
  # Build plan from YAML manifest
  aosctl build-plan manifests/qwen7b.yaml --output plan/qwen7b/plan.bin

  # Build plan for production
  aosctl build-plan manifests/production.yaml --output plan/prod_v1/plan.bin
")]
    PlanBuild {
        /// Manifest path
        manifest: PathBuf,

        /// Output path
        #[arg(short, long)]
        output: PathBuf,

        /// Tenant ID (defaults to "default")
        #[arg(long)]
        tenant_id: Option<String>,
    },

    // ============================================================
    // Model Management
    // ============================================================
    /// Import a model
    #[command(after_help = "\
Examples:
  # Import Qwen2.5-7B model
  aosctl import-model \\
    --name qwen2.5-7b \\
    --weights models/qwen2.5-7b-mlx/weights.safetensors \\
    --config models/qwen2.5-7b-mlx/config.json \\
    --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \\
    --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \\
    --license models/qwen2.5-7b-mlx/LICENSE
")]
    ModelImport {
        /// Model name
        #[arg(short, long)]
        name: String,

        /// Weights file path
        #[arg(short, long)]
        weights: PathBuf,

        /// Config file path
        #[arg(short, long)]
        config: PathBuf,

        /// Tokenizer file path
        #[arg(short, long)]
        tokenizer: PathBuf,

        /// Tokenizer config file path
        #[arg(long)]
        tokenizer_cfg: PathBuf,

        /// License file path
        #[arg(short, long)]
        license: PathBuf,
    },

    // ============================================================
    // Telemetry & Verification
    // ============================================================
    /// Telemetry commands (list, verify)
    #[command(subcommand, alias = "telemetry-list", alias = "telemetry-verify")]
    Telemetry(telemetry::TelemetryCommand),

    /// Federation commands (verify cross-host signatures)
    #[command(subcommand, alias = "federation-verify")]
    Federation(federation::FederationCommand),

    /// Check for environment drift
    #[command(after_help = "\
Examples:
  # Check drift against baseline
  aosctl drift-check

  # Check drift and save current fingerprint
  aosctl drift-check --save-current

  # Create initial baseline
  aosctl drift-check --save-baseline

  # Use custom baseline path
  aosctl drift-check --baseline var/production_baseline.json
")]
    DriftCheck {
        /// Database path
        #[arg(long)]
        db: Option<PathBuf>,

        /// Baseline fingerprint path
        #[arg(long)]
        baseline: Option<PathBuf>,

        /// Save current fingerprint
        #[arg(long)]
        save_current: bool,

        /// Save as new baseline
        #[arg(long)]
        save_baseline: bool,
    },

    // ============================================================
    // CodeGraph & Call Graph
    // ============================================================
    /// CodeGraph commands (export, stats)
    #[command(subcommand, alias = "callgraph-export", alias = "codegraph-stats")]
    Codegraph(codegraph::CodegraphCommand),

    // ============================================================
    // Security Daemon
    // ============================================================
    /// Security daemon commands (status, audit)
    #[command(subcommand, alias = "secd-status", alias = "secd-audit")]
    Secd(secd::SecdCommand),

    // ============================================================
    // General Operations
    // ============================================================
    /// Import an artifact bundle
    #[command(after_help = "\
Examples:
  # Import signed bundle (default, recommended)
  aosctl import artifacts/adapters.zip

  # Import without verification (dev only, not recommended)
  aosctl import bundle.zip --no-verify
")]
    Import {
        /// Bundle path
        bundle: PathBuf,

        /// Skip signature verification
        #[arg(long)]
        no_verify: bool,
    },

    /// Verify a bundle
    #[command(after_help = "\
Examples:
  # Verify artifact bundle signature and hashes
  aosctl verify artifacts/adapters.zip

  # Verify telemetry bundle chain
  aosctl verify-telemetry --bundle-dir ./var/telemetry
")]
    Verify {
        /// Bundle path
        bundle: PathBuf,
    },

    /// Verify adapter deliverables (A–F)
    #[command(after_help = "\
Examples:
  # Run full adapter verification
  aosctl verify-adapters

  # JSON summary for CI
  aosctl --json verify-adapters
")]
    VerifyAdapters,

    /// Verify determinism loop (dev-only; delegates to cargo xtask)
    #[command(name = "verify-determinism-loop")]
    #[command(after_help = "\
Examples:
  # Generate determinism report via xtask
  aosctl verify-determinism-loop

  # In CI, prefer this over calling `cargo xtask determinism-report` directly
")]
    VerifyDeterminismLoop,

    // ============================================================
    // Policy Management
    // ============================================================
    /// Manage policy packs
    #[command(subcommand)]
    #[command(after_help = "\
Examples:
  # List all policy packs
  aosctl policy list

  # List only implemented policies
  aosctl policy list --implemented

  # Explain a policy pack
  aosctl policy explain Egress
  aosctl policy explain 1

  # Enforce all policies (dry run)
  aosctl policy enforce --all --dry-run

  # Enforce specific policy
  aosctl policy enforce --pack Determinism
")]
    Policy(policy::PolicyCommand),

    /// Start serving
    #[command(after_help = "\
Examples:
  # Validate setup without starting (recommended first)
  aosctl serve --tenant tenant_dev --plan cp_abc123 --dry-run

  # Start serving
  aosctl serve --tenant tenant_dev --plan cp_abc123

  # Custom socket path
  aosctl serve --tenant tenant_dev --plan cp_abc123 --socket /tmp/aos.sock
")]
    Serve {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Plan ID
        #[arg(short, long)]
        plan: String,

        /// UDS socket path
        #[arg(short, long, default_value = "/var/run/aos/aos.sock")]
        socket: PathBuf,

        /// Backend selection: metal, mlx, or coreml
        #[arg(short, long, default_value = "metal")]
        backend: BackendType,

        /// Dry-run: validate preflight checks without starting server
        #[arg(long)]
        dry_run: bool,
    },

    /// Run audit checks
    #[command(after_help = "\
Examples:
  # Audit checkpoint
  aosctl audit CP-0001

  # Audit with custom test suite
  aosctl audit CP-0001 --suite ./tests/production.yaml

  # Audit and generate report
  aosctl audit CP-0001 --json > audit.json
")]
    Audit {
        /// CPID to audit
        cpid: String,

        /// Test suite path
        #[arg(short, long)]
        suite: Option<PathBuf>,
    },

    /// Audit backend determinism attestation
    #[command(after_help = "\
Examples:
  # Audit Metal backend (default)
  aosctl audit-determinism

  # Audit with JSON output
  aosctl audit-determinism --format json

  # Audit MLX backend (requires --features multi-backend)
  aosctl audit-determinism --backend mlx --model-path ./models/qwen2.5-7b-mlx
")]
    AuditDeterminism {
        #[command(flatten)]
        args: audit_determinism::AuditDeterminismArgs,
    },

    /// Run a local inference against the worker UDS
    #[command(after_help = r#"
Examples:
  # Basic inference
  aosctl infer --prompt 'Hello world' --socket /var/run/adapteros.sock

  # Inference using a specific adapter (preload+swap)
  aosctl infer --adapter my_adapter --prompt 'Use adapter' --socket /var/run/adapteros.sock

  # Increase max tokens and timeout
  aosctl infer --prompt 'Test' --max-tokens 256 --timeout 60000
"#)]
    Infer {
        /// Optional adapter to activate before inference
        #[arg(long)]
        adapter: Option<String>,

        /// Prompt text to infer on
        #[arg(long)]
        prompt: String,

        /// UDS socket path
        #[arg(long, default_value = "/var/run/adapteros.sock")]
        socket: PathBuf,

        /// Max tokens to generate
        #[arg(long)]
        max_tokens: Option<usize>,

        /// Require evidence (RAG/open-book) if enabled in worker
        #[arg(long, default_value_t = false)]
        require_evidence: bool,

        /// Timeout in milliseconds
        #[arg(long, default_value_t = 30000)]
        timeout: u64,

        /// Show citations (trace.evidence) in output
        #[arg(long, default_value_t = false)]
        show_citations: bool,

        /// Show full trace (router summary, token counts)
        #[arg(long, default_value_t = false)]
        show_trace: bool,
    },

    /// Replay a bundle
    #[command(after_help = "\
Examples:
  # Replay bundle
  aosctl replay ./var/bundles/bundle_001.zip

  # Replay with verbose output
  aosctl replay ./var/bundles/bundle_001.zip --verbose

  # Replay and check determinism
  aosctl replay ./var/bundles/bundle_001.zip --check-determinism
")]
    Replay {
        /// Bundle path
        bundle: PathBuf,

        /// Show divergence details (overridden by global --verbose)
        #[arg(short, long)]
        verbose: bool,
    },

    /// Rollback to previous checkpoint
    #[command(after_help = "\
Examples:
  # Rollback tenant to checkpoint
  aosctl rollback --tenant dev CP-0001

  # Rollback with confirmation
  aosctl rollback --tenant dev CP-0001 --confirm

  # Check rollback status
  aosctl rollback --tenant dev CP-0001 --dry-run
")]
    Rollback {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Target CPID
        cpid: String,
    },

    /// Golden run archive management (audit reproducibility)
    #[command(subcommand)]
    #[command(after_help = "\
Examples:
  # Initialize golden_runs directory
  aosctl golden init

  # Create golden run from bundle
  aosctl golden create --bundle var/bundles/baseline.ndjson --name baseline-001 --sign

  # List golden runs
  aosctl golden list

  # Verify against golden run
  aosctl golden verify --golden baseline-001 --bundle var/bundles/new_run.ndjson

  # Verify with strict (bitwise) checking
  aosctl golden verify --golden baseline-001 --bundle var/bundles/new_run.ndjson --strictness bitwise

  # Show golden run details
  aosctl golden show baseline-001
")]
    Golden(GoldenCmd),

    /// Router weight calibration and management
    #[command(subcommand)]
    #[command(after_help = "\
Examples:
  # Calibrate router weights using a dataset
  aosctl router calibrate --dataset calibration.json --output weights.json

  # Validate weights on a dataset
  aosctl router validate --dataset test.json --weights weights.json

  # Show current router weights
  aosctl router show --weights weights.json
")]
    Router(router::RouterCmd),

    /// Generate HTML report from bundle
    #[command(after_help = "\
Examples:
  # Generate HTML report
  aosctl report ./var/bundles/bundle_001.zip --output report.html

  # Generate report with custom template
  aosctl report ./var/bundles/bundle_001.zip --output report.html --template custom.html

  # Generate report and open in browser
  aosctl report ./var/bundles/bundle_001.zip --output report.html --open
")]
    Report {
        /// Bundle path
        bundle: PathBuf,

        /// Output HTML file
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Bootstrap AdapterOS installation
    #[command(after_help = "\
Examples:
  # Full installation
  aosctl bootstrap --mode full

  # Minimal installation
  aosctl bootstrap --mode minimal

  # Air-gapped installation
  aosctl bootstrap --mode full --air-gapped

  # Bootstrap with checkpoint
  aosctl bootstrap --mode full --checkpoint-file ./checkpoint.json
")]
    Bootstrap {
        /// Installation mode (full or minimal)
        #[arg(short, long, default_value = "full")]
        mode: String,

        /// Enable air-gapped mode (skip network operations)
        #[arg(long)]
        air_gapped: bool,

        /// Output JSON progress updates
        #[arg(long)]
        json: bool,

        /// Checkpoint file path
        #[arg(long)]
        checkpoint_file: Option<PathBuf>,
    },

    // ============================================================
    // Utility
    // ============================================================
    /// Generate shell completion script
    #[command(after_help = "\
Examples:
  # Generate bash completion
  aosctl completions bash > /usr/local/etc/bash_completion.d/aosctl

  # Generate zsh completion
  aosctl completions zsh > /usr/local/share/zsh/site-functions/_aosctl

  # Generate fish completion
  aosctl completions fish > ~/.config/fish/completions/aosctl.fish
")]
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Manage configuration settings
    #[command(subcommand)]
    Config(config::ConfigArgs),

    // ============================================================
    // Documentation & Help
    // ============================================================
    /// Run system diagnostics
    #[command(after_help = "\
Examples:
  # Full system diagnostics
  aosctl diag --full

  # System checks only
  aosctl diag --system

  # Tenant-specific checks
  aosctl diag --tenant dev

  # Create diagnostic bundle
  aosctl diag --full --bundle ./diag_bundle.zip
")]
    /// Show backend status and capabilities
    #[command(after_help = "\
Examples:
  # Show backend summary
  aosctl backend-status

  # Show detailed backend information
  aosctl backend-status --detailed

  # Output in JSON format
  aosctl backend-status --json
")]
    BackendStatus(commands::backend_status::BackendStatusArgs),

    /// Run diagnostics and health checks
    #[command(after_help = "\
Examples:
  # Full system diagnostics
  aosctl diag

  # System checks only
  aosctl diag --system

  # Tenant-specific checks
  aosctl diag --tenant dev

  # Create diagnostic bundle
  aosctl diag --full --bundle ./diag_bundle.zip
")]
    Diag {
        /// Diagnostic profile: system, tenant, or full
        #[arg(long, default_value = "full")]
        profile: Option<String>,

        /// Tenant ID for tenant-specific checks
        #[arg(long)]
        tenant: Option<String>,

        /// Output JSON format
        #[arg(long)]
        json: bool,

        /// Create diagnostic bundle
        #[arg(long)]
        bundle: Option<PathBuf>,

        /// System checks only
        #[arg(long, conflicts_with_all = ["tenant_only", "profile"])]
        system: bool,

        /// Tenant checks only
        #[arg(long, conflicts_with_all = ["system", "profile"])]
        tenant_only: bool,

        /// Full diagnostics (default)
        #[arg(long, conflicts_with_all = ["system", "tenant_only", "profile"])]
        full: bool,
    },

    /// Explain an error code or AosError variant
    #[command(after_help = "\
Examples:
  # Explain specific error code
  aosctl explain E3001

  # Explain AosError variant
  aosctl explain InvalidHash

  # Get help for unknown error
  aosctl explain E9999
")]
    Explain {
        /// Error code (E3001) or AosError name (InvalidHash)
        code: String,
    },

    /// List all error codes
    #[command(after_help = "\
Examples:
  # List all error codes
  aosctl error-codes

  # List error codes in JSON format
  aosctl error-codes --json

  # Filter by category
  aosctl error-codes --category crypto
")]
    ErrorCodes {
        /// Output JSON format
        #[arg(long)]
        json: bool,
    },

    /// Interactive tutorial
    #[command(after_help = "\
Examples:
  # Run basic tutorial
  aosctl tutorial

  # Run advanced tutorial
  aosctl tutorial --advanced

  # Non-interactive mode for CI
  aosctl tutorial --ci
")]
    Tutorial {
        /// Run advanced tutorial
        #[arg(long)]
        advanced: bool,

        /// Non-interactive mode for CI
        #[arg(long)]
        ci: bool,
    },

    /// Display offline manual
    #[command(after_help = "\
Examples:
  # Display manpage
  aosctl manual --format man

  # Display offline markdown manual
  aosctl manual --format md

  # Search manual for specific terms
  aosctl manual --format md --search \"error codes\"
")]
    Manual {
        #[command(flatten)]
        args: commands::manual::ManualArgs,
    },

    /// Train a LoRA adapter
    #[command(after_help = "\
Examples:
  # Train adapter with default settings
  aosctl train --data training_data.json --output ./trained_adapter

  # Train with custom configuration
  aosctl train --data training_data.json --output ./trained_adapter \\
    --rank 8 --alpha 32.0 --learning-rate 0.001 --epochs 5

  # Train with Metal backend
  aosctl train --data training_data.json --output ./trained_adapter \\
    --plan plan/qwen7b/plan.bin

  # Train with configuration file
  aosctl train --config training_config.json --data training_data.json \\
    --output ./trained_adapter
")]
    Train {
        #[command(flatten)]
        args: train::TrainArgs,
    },

    /// Alias for tenant-init (for convenience)
    #[command(hide = true)]
    Init {
        /// Tenant ID
        #[arg(short, long)]
        id: String,

        /// Unix UID
        #[arg(short, long)]
        uid: u32,

        /// Unix GID
        #[arg(short, long)]
        gid: u32,
    },

    // ============================================================
    // Code Intelligence Commands
    // ============================================================
    /// Code intelligence commands (init, update, list, status)
    #[command(subcommand, alias = "code-init", alias = "code-update", alias = "code-list", alias = "code-status")]
    Code(code::CodeCommand),
}


#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file first (before anything else reads env vars)
    adapteros_config::load_dotenv();

    // Initialize unified logging
    init_logging()?;

    let cli = Cli::parse();

    // Create output writer based on global flags
    let output_mode = OutputMode::from_flags(cli.json, cli.quiet);
    let output = OutputWriter::new(output_mode, cli.verbose);

    // Get command name for telemetry
    let command_name = get_command_name(&cli.command);
    let tenant_id = extract_tenant_from_command(&cli.command);

    // Execute command and handle errors with telemetry
    let result = execute_command(&cli.command, &cli, &output).await;

    match result {
        Ok(_) => {
            // Emit success telemetry
            let _ =
                cli_telemetry::emit_cli_command(&command_name, tenant_id.as_deref(), true).await;
            Ok(())
        }
        Err(e) => {
            // Extract error code and emit telemetry
            let error_code = cli_telemetry::extract_error_code(&e);
            let error_msg = format!("{}", e);

            let event_id = cli_telemetry::emit_cli_error(
                error_code.as_deref(),
                &command_name,
                tenant_id.as_deref(),
                &error_msg,
            )
            .await
            .unwrap_or_else(|_| "-".to_string());

            // If error code exists, suggest using explain with event ID
            if let Some(code) = error_code {
                eprintln!(
                    "\n✗ {} – see: aosctl explain {} (event: {})",
                    code, code, event_id
                );
            }

            Err(e)
        }
    }
}

async fn execute_command(command: &Commands, cli: &Cli, output: &OutputWriter) -> Result<()> {
    match command {
        // Tenant Management
        Commands::TenantInit { id, uid, gid } | Commands::Init { id, uid, gid } => {
            init_tenant::run(&id, *uid, *gid, &output).await?;
        }

        // Adapter Management
        Commands::Adapter(cmd) => {
            adapter::handle_adapter_command(cmd.clone(), &output).await?;
        }

        // Node & Cluster Management
        Commands::Node(cmd) => {
            node::handle_node_command(cmd.clone(), &output).await?;
        }

        // Deployment
        Commands::Deploy(cmd) => {
            commands::deploy::run(cmd.clone(), &output).await?;
        }

        // System Status
        Commands::Status(cmd) => {
            commands::status::run(cmd.clone(), &output).await?;
        }

        // System Health Diagnostics (PRD-06)
        Commands::Doctor(cmd) => {
            commands::doctor::run(cmd.clone(), &output).await?;
        }

        // Maintenance
        Commands::Maintenance(cmd) => {
            commands::maintenance::run(cmd.clone(), &output).await?;
        }

        // Registry Management
        Commands::Registry(cmd) => {
            registry::handle_registry_command(cmd.clone(), &output).await?;
        }

        // Plan Management
        Commands::PlanBuild {
            manifest,
            output: output_path,
            tenant_id,
        } => {
            build_plan::run(&manifest, &output_path, tenant_id.as_deref(), &output).await?;
        }

        // Model Management
        Commands::ModelImport {
            name,
            weights,
            config,
            tokenizer,
            tokenizer_cfg,
            license,
        } => {
            import_model::run(
                &name,
                &weights,
                &config,
                &tokenizer,
                &tokenizer_cfg,
                &license,
                &output,
            )
            .await?;
        }

        // Telemetry & Verification
        Commands::Telemetry(cmd) => {
            telemetry::handle_telemetry_command(cmd.clone(), &output).await?;
        }

        Commands::Federation(cmd) => {
            federation::handle_federation_command(cmd.clone(), &output).await?;
        }

        Commands::DriftCheck {
            db,
            baseline,
            save_current,
            save_baseline,
        } => {
            std::process::exit(
                commands::drift_check::drift_check(
                    db.clone(),
                    baseline.clone(),
                    *save_current,
                    *save_baseline,
                )
                .await?,
            );
        }

        // CodeGraph & Call Graph
        Commands::Codegraph(cmd) => {
            codegraph::handle_codegraph_command(cmd.clone(), &output).await?;
        }

        // Security Daemon
        Commands::Secd(cmd) => {
            secd::handle_secd_command(cmd.clone()).await?;
        }

        // General Operations
        Commands::Import { bundle, no_verify } => {
            import::run(&bundle, !no_verify, &output).await?;
        }
        Commands::Verify { bundle } => {
            verify::run(&bundle, &output).await?;
        }
        Commands::VerifyDeterminismLoop => {
            let exit_code = verify_determinism_loop::run(&output).await?;
            std::process::exit(exit_code);
        }
        Commands::VerifyAdapters => {
            let exit_code = commands::verify_adapters::run(&output).await?;
            std::process::exit(exit_code);
        }

        // Policy Management
        Commands::Policy(cmd) => {
            cmd.clone().run()?;
        }

        Commands::Serve {
            tenant,
            plan,
            socket,
            backend,
            dry_run,
        } => {
            // Build model config from CLI flags (precedence: CLI > ENV > defaults)
            let model_config = cli.get_model_config().ok();
            serve::run(
                tenant,
                plan,
                socket,
                backend.clone(),
                *dry_run,
                None, // capture_events (not supported in legacy main.rs)
                model_config.as_ref(),
                &output,
            )
            .await?;
        }
        Commands::Audit { cpid, suite } => {
            audit::run(&cpid, suite.as_deref(), &output).await?;
        }
        Commands::AuditDeterminism { args } => {
            let audit_output = audit_determinism::Output;
            let exit_code = audit_determinism::run(args, &audit_output)?;
            std::process::exit(exit_code);
        }
        Commands::Infer {
            adapter,
            prompt,
            socket,
            max_tokens,
            require_evidence,
            timeout,
            show_citations,
            show_trace,
        } => {
            // Check UMA pressure before inference
            let monitor = UmaPressureMonitor::new(15, None);
            let pressure = monitor.get_current_pressure();
            if matches!(
                pressure,
                MemoryPressureLevel::High | MemoryPressureLevel::Critical
            ) {
                eprintln!(
                    "System under pressure (level: {}), retry in 30s or reduce max_tokens",
                    pressure.to_string()
                );
                std::process::exit(1);
            }

            commands::infer::run(
                adapter.clone(),
                prompt.clone(),
                *max_tokens,
                *require_evidence,
                socket.clone(),
                *timeout,
                *show_citations,
                *show_trace,
            )
            .await?;
        }
        Commands::Replay { bundle, verbose } => {
            // Merge command-specific verbose flag with global verbose
            let verbose_mode = *verbose || cli.verbose;
            replay::run(&bundle, verbose_mode, &output).await?;
        }
        Commands::Rollback { tenant, cpid } => {
            rollback::run(&tenant, &cpid, &output).await?;
        }
        Commands::Golden(cmd) => {
            golden::execute(cmd, &output).await?;
        }
        Commands::Router(cmd) => {
            cmd.clone().run()?;
        }
        Commands::Report {
            bundle,
            output: output_path,
        } => {
            report::run(&bundle, &output_path, &output).await?;
        }
        Commands::Bootstrap {
            mode,
            air_gapped,
            json,
            checkpoint_file,
        } => {
            // Bootstrap doesn't use OutputWriter, runs standalone
            bootstrap::run(&mode, *air_gapped, *json, checkpoint_file.clone()).await?;
        }

        // Utility
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            completions::generate_completions(*shell, &mut cmd)?;
        }

        // Configuration Management
        Commands::Config(args) => {
            config::run_config_command(args.clone(), &output).await?;
        }

        // Backend Status
        Commands::BackendStatus(args) => {
            commands::backend_status::run(args.clone()).await?;
        }

        // Documentation & Help
        Commands::Diag {
            profile,
            tenant,
            json,
            bundle,
            system,
            tenant_only,
            full,
        } => {
            let diag_profile = if *system {
                diag::DiagProfile::System
            } else if *tenant_only {
                diag::DiagProfile::Tenant
            } else if *full {
                diag::DiagProfile::Full
            } else if let Some(p) = profile {
                match p.as_str() {
                    "system" => diag::DiagProfile::System,
                    "tenant" => diag::DiagProfile::Tenant,
                    "full" => diag::DiagProfile::Full,
                    _ => {
                        return Err(anyhow::anyhow!(
                            "Invalid profile: {}. Use: system, tenant, or full",
                            p
                        ))
                    }
                }
            } else {
                diag::DiagProfile::Full
            };

            diag::run(diag_profile, tenant.clone(), *json, bundle.clone()).await?;
        }

        Commands::Explain { code } => {
            explain::explain(&code).await?;
        }

        Commands::ErrorCodes { json } => {
            explain::list_error_codes(*json).await?;
        }

        Commands::Tutorial { advanced, ci } => {
            commands::tutorial::run_tutorial(
                output.clone(),
                commands::tutorial::TutorialArgs {
                    advanced: *advanced,
                    ci: *ci,
                },
            )
            .await?;
        }

        Commands::Manual { args } => {
            commands::manual::run_manual(args.clone())?;
        }

        Commands::Train { args } => {
            args.execute().await?;
        }

        // Code Intelligence Commands
        Commands::Code(cmd) => {
            code::handle_code_command(cmd.clone(), &output).await?;
        }
    }

    Ok(())
}

/// Get command name from Commands enum
fn get_command_name(command: &Commands) -> String {
    match command {
        Commands::TenantInit { .. } | Commands::Init { .. } => "init-tenant",
        Commands::Adapter(_) => "adapter",
        Commands::Node(_) => "node",
        Commands::Status { .. } => "status",
        Commands::Doctor { .. } => "doctor",
        Commands::Maintenance { .. } => "maintenance",
        Commands::Deploy { .. } => "deploy",
        Commands::Registry(_) => "registry",
        Commands::PlanBuild { .. } => "build-plan",
        Commands::ModelImport { .. } => "import-model",
        Commands::Telemetry(_) => "telemetry",
        Commands::Federation(_) => "federation",
        Commands::DriftCheck { .. } => "drift-check",
        Commands::Codegraph(_) => "codegraph",
        Commands::Secd(_) => "secd",
        Commands::Import { .. } => "import",
        Commands::Verify { .. } => "verify",
        Commands::VerifyAdapters { .. } => "verify-adapters",
        Commands::Policy(_) => "policy",
        Commands::Serve { .. } => "serve",
        Commands::Audit { .. } => "audit",
        Commands::AuditDeterminism { .. } => "audit-determinism",
        Commands::Infer { .. } => "infer",
        Commands::VerifyDeterminismLoop => "verify-determinism-loop",
        Commands::Replay { .. } => "replay",
        Commands::Rollback { .. } => "rollback",
        Commands::Golden(_) => "golden",
        Commands::Router(_) => "router",
        Commands::Report { .. } => "report",
        Commands::Bootstrap { .. } => "bootstrap",
        Commands::Completions { .. } => "completions",
        Commands::Config(_) => "config",
        Commands::Diag { .. } => "diag",
        Commands::Explain { .. } => "explain",
        Commands::ErrorCodes { .. } => "error-codes",
        Commands::Tutorial { .. } => "tutorial",
        Commands::Manual { .. } => "manual",
        Commands::Train { .. } => "train",
        Commands::Code(_) => "code",
        Commands::BackendStatus(_) => "backend-status",
    }
    .to_string()
}

/// Extract tenant ID from command if present
fn extract_tenant_from_command(command: &Commands) -> Option<String> {
    match command {
        Commands::Serve { tenant, .. } | Commands::Rollback { tenant, .. } => {
            Some(tenant.clone())
        }
        Commands::Diag { tenant, .. } => tenant.clone(),
        // Tenant extraction for grouped commands is handled by their respective handlers
        _ => None,
    }
}

// Logging initialization moved to logging module
