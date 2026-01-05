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
use std::path::{Path, PathBuf};

mod auth_store;
mod cli;
mod cli_telemetry;
mod cmd_replay;
mod cmd_trace_export;
mod commands;
mod error_codes;
mod formatting;
mod http_client;
mod logging;
mod output;

use adapteros_lora_worker::memory::{MemoryPressureLevel, UmaPressureMonitor};
use commands::golden::GoldenCmd;
use commands::init;
use commands::*;
use logging::init_logging;
use output::{OutputMode, OutputWriter};

/// Backend type selection for inference
///
/// This is a CLI-specific wrapper that converts to `BackendPreference` from `adapteros-config`.
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum BackendType {
    /// Metal backend (macOS GPU)
    Metal,
    /// MLX backend (C++ FFI)
    #[clap(name = "mlx")]
    MLX,
    /// CoreML backend (macOS Neural Engine)
    CoreML,
}

impl From<BackendType> for adapteros_config::BackendPreference {
    fn from(bt: BackendType) -> Self {
        match bt {
            BackendType::Metal => adapteros_config::BackendPreference::Metal,
            BackendType::MLX => adapteros_config::BackendPreference::Mlx,
            BackendType::CoreML => adapteros_config::BackendPreference::CoreML,
        }
    }
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

#[derive(Subcommand, Clone, Debug)]
enum TraceCommand {
    /// Export an offline trace bundle for replay verification
    #[command(after_help = "\
Examples:
  aosctl trace export --request basic --out ./var/trace/basic
  aosctl trace export --request cross-worker --out ./var/trace/cross --fixtures ./test_data/replay_fixtures
")]
    Export {
        /// Request identifier (maps to fixture directory)
        #[arg(long)]
        request: String,
        /// Output directory for exported artifacts
        #[arg(long)]
        out: PathBuf,
        /// Optional custom fixture root (defaults to test_data/replay_fixtures)
        #[arg(long)]
        fixtures: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum Commands {
    // ============================================================
    // Authentication
    // ============================================================
    /// Login/logout for CLI-managed tokens
    #[command(subcommand)]
    Auth(commands::auth_cli::AuthCommand),

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

    /// Repository + version management commands
    #[command(subcommand)]
    Repo(commands::repo::RepoCommand),

    /// Adapter stack management commands (create, list, activate, etc.)
    #[command(subcommand, visible_alias = "stacks")]
    Stack(stack::StackCommand),

    // ============================================================
    // Interactive Chat
    // ============================================================
    /// Interactive chat with streaming inference
    #[command(subcommand)]
    Chat(chat::ChatCommand),

    // ============================================================
    // Development Commands
    // ============================================================
    /// Development environment commands (start/stop services)
    #[command(subcommand_required = false)]
    Dev {
        #[command(subcommand)]
        cmd: Option<dev::DevCommand>,
    },

    /// Scenario readiness utilities
    #[command(subcommand)]
    Scenario(commands::scenario::ScenarioSubcommand),

    /// CoreML verification and health commands
    #[command(subcommand)]
    Coreml(commands::coreml_status::CoremlCommand),

    // ============================================================
    // Model Export
    // ============================================================
    /// Export a fused CoreML package from a base model and adapter bundle
    #[cfg(feature = "coreml-export")]
    CoremlExport {
        /// Path to the base CoreML package directory or Manifest.json
        #[arg(long)]
        base_package: PathBuf,
        /// Path to the adapter .aos bundle
        #[arg(long)]
        adapter_aos: PathBuf,
        /// Output path for the fused CoreML package (directory or manifest)
        #[arg(long)]
        output_package: PathBuf,
        /// Preferred compute units: cpu_only, cpu_and_gpu, cpu_and_neural_engine, all
        #[arg(long)]
        compute_units: Option<String>,
        /// Optional logical base model ID for recording
        #[arg(long)]
        base_model_id: Option<String>,
        /// Optional logical adapter ID for recording
        #[arg(long)]
        adapter_id: Option<String>,
    },
    /// Trigger CoreML export for a completed training job
    #[cfg(feature = "coreml-export")]
    CoremlExportJob {
        /// Training job ID to export
        job_id: String,
        /// Control plane base URL (default http://localhost:8080)
        #[arg(long, default_value = "http://localhost:8080")]
        base_url: String,
    },
    /// Show CoreML export status for a training job
    #[cfg(feature = "coreml-export")]
    CoremlExportStatus {
        /// Training job ID to inspect
        job_id: String,
        /// Control plane base URL (default http://localhost:8080)
        #[arg(long, default_value = "http://localhost:8080")]
        base_url: String,
    },

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

    /// Run system health diagnostics
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

    /// Post-reboot startup verification (requires running server)
    #[command(after_help = "\
Examples:
  # Run post-reboot startup checks
  aosctl check startup

  # Check against custom server URL
  aosctl check startup --server-url http://localhost:8080

  # Check with custom timeout
  aosctl check startup --timeout 30
")]
    Check(commands::check::CheckCommand),

    /// Pre-flight system readiness check (run before launching server)
    #[command(after_help = "\
Examples:
  # Run pre-flight checks
  aosctl preflight

  # Run checks with auto-fix
  aosctl preflight --fix

  # Check specific model path
  aosctl preflight --model-path ./models/my-model

  # Skip backend checks (faster)
  aosctl preflight --skip-backends

  # Check before launch (recommended)
  aosctl preflight && cargo run -p adapteros-server-api
")]
    Preflight(commands::preflight::PreflightCommand),

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
    #[command(subcommand)]
    Registry(registry::RegistryCommand),

    // ============================================================
    // Storage Management
    // ============================================================
    /// Storage backend management commands (mode, migrate, verify)
    #[command(subcommand)]
    Storage(storage::StorageCommand),

    // ============================================================
    // Database Management
    // ============================================================
    /// Database management commands (migrate, reset)
    #[command(subcommand)]
    Db(commands::db::DbCommand),

    // ============================================================
    // Review Management
    // ============================================================
    /// Human-in-the-loop review commands (list, submit, export)
    #[command(subcommand)]
    Review(commands::review::ReviewCommand),

    // ============================================================
    // Model Management
    // ============================================================
    /// Model management commands (seed, list)
    #[command(subcommand)]
    Models(commands::models::ModelsCommand),

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
    --weights ${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}/weights.safetensors \\
    --config ${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}/config.json \\
    --tokenizer ${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}/tokenizer.json \\
    --tokenizer-cfg ${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}/tokenizer_config.json \\
    --license ${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}/LICENSE
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
    #[command(subcommand)]
    Telemetry(telemetry::TelemetryCommand),

    /// Trace utilities (export replayable artifacts)
    #[command(subcommand)]
    Trace(TraceCommand),

    /// Federation commands (verify cross-host signatures)
    #[command(subcommand)]
    Federation(federation::FederationCommand),

    /// Run deterministic drift harness across backends
    #[command(after_help = "\
Examples:
  # Run drift harness with config file
  aosctl drift-check --config drift.json

  # Override dataset path
  aosctl drift-check --config drift.json --dataset ./tests/data.json

  # Limit to CPU and CoreML
  aosctl drift-check --config drift.json --backend cpu --backend coreml
")]
    DriftCheck {
        /// Drift harness config (JSON)
        #[arg(long)]
        config: PathBuf,

        /// Override dataset path
        #[arg(long)]
        dataset: Option<PathBuf>,

        /// Override manifest path for metadata writeback
        #[arg(long)]
        manifest: Option<PathBuf>,

        /// Backends to run (repeatable)
        #[arg(long, value_name = "cpu|coreml|mlx|metal")]
        backend: Vec<String>,

        /// Reference backend (defaults to first backend)
        #[arg(long)]
        reference_backend: Option<String>,
    },

    // ============================================================
    // CodeGraph & Call Graph
    // ============================================================
    /// CodeGraph commands (export, stats)
    #[cfg(feature = "codegraph")]
    #[command(subcommand)]
    Codegraph(codegraph::CodegraphCommand),

    // ============================================================
    // Security Daemon
    // ============================================================
    /// Security daemon commands (status, audit)
    #[cfg(feature = "secd-support")]
    #[command(subcommand)]
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

  # Verify telemetry bundle chain from a trace ID
  aosctl verify trace_12345

  # Force bundle verification
  aosctl verify ./artifacts/adapters.zip --bundle
")]
    Verify {
        /// Bundle path or trace ID
        target: String,

        /// Treat target as a trace ID and fetch the receipt proof chain
        #[arg(long, conflicts_with = "bundle")]
        trace: bool,

        /// Treat target as a bundle path even if it looks like an ID
        #[arg(long)]
        bundle: bool,

        /// Control plane base URL for trace verification
        #[arg(
            long,
            env = "AOS_SERVER_URL",
            default_value = "http://127.0.0.1:8080/api"
        )]
        base_url: String,
    },

    /// Verify an offline run receipt bundle
    #[command(
        name = "verify-receipt",
        after_help = "\
Examples:
  # Verify a run receipt bundle from a directory
  aosctl verify-receipt --bundle ./var/receipts/run-123

  # Verify a single bundle file directly
  aosctl verify-receipt --bundle ./run_receipt.json
"
    )]
    VerifyReceipt {
        /// Receipt bundle directory (or JSON file)
        #[arg(long)]
        bundle: Option<PathBuf>,

        /// Fetch receipt from API instead of local bundle
        #[arg(long, value_name = "TRACE_ID")]
        online: Option<String>,

        /// Control plane base URL
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,
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

  # Custom socket path (development)
  aosctl serve --tenant tenant_dev --plan cp_abc123 --socket ./var/run/aos.sock
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
  aosctl audit-determinism --backend mlx --model-path ${AOS_MODEL_CACHE_DIR}/${AOS_BASE_MODEL_ID}
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

    /// Replay offline artifacts and verify determinism receipts
    #[command(after_help = "\
Examples:
  # Replay exported trace (writes replay_report.json)
  aosctl replay --dir ./var/trace/basic --verify

  # Replay with custom report path
  aosctl replay --dir ./var/trace/basic --report ./var/trace/result.json
")]
    Replay {
        /// Directory containing exported trace artifacts
        #[arg(long)]
        dir: PathBuf,

        /// Optional output path for replay_report.json
        #[arg(long)]
        report: Option<PathBuf>,

        /// Exit non-zero on mismatch
        #[arg(long, default_value_t = false)]
        verify: bool,
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

    /// Run diagnostics, export bundles, and verify bundles
    #[command(subcommand)]
    Diag(diag::DiagCommand),

    /// Generate a log digest (WARN/ERROR summary)
    #[command(after_help = "\
Examples:
  aosctl log-digest
  aosctl log-digest --minutes 120
  aosctl log-digest --log-dir ./var/logs --out-dir ./var/analysis
")]
    LogDigest(commands::log_digest::LogDigestCommand),

    /// Triage log digest with rule-based remediation hints
    #[command(after_help = "\
Examples:
  aosctl log-triage
  aosctl log-triage --digest ./var/analysis/digest.json
  aosctl log-triage --rules ./configs/log_triage_rules.json
")]
    LogTriage(commands::log_triage::LogTriageCommand),

    /// Build an LLM prompt from triage output
    #[command(after_help = "\
Examples:
  aosctl log-prompt
  aosctl log-prompt --triage ./var/analysis/triage.json
  aosctl log-prompt --out-dir ./var/analysis/proposals
")]
    LogPrompt(commands::log_prompt::LogPromptCommand),

    /// Targeted diagnostics for drift, health, and storage reconciler
    #[command(after_help = "\
Examples:
  # Run drift harness for a dataset version
  aosctl health drift-run --repo-id repo1 --dataset-version-ids ds-ver-1 --backend metal --baseline-backend cpu

  # Show adapter health rollup
  aosctl health adapter --repo-id repo1

  # List storage reconciliation issues
  aosctl health storage-list-issues --limit 20
")]
    Health(commands::diag_health::HealthCommand),

    /// Run determinism check (3 fixed prompts, N runs, compare outputs)
    #[command(after_help = "\
Examples:
  # Run determinism check with default settings
  aosctl determinism

  # Run with specific stack and custom runs
  aosctl determinism --stack-id my-stack --runs 5

  # Run with custom seed
  aosctl determinism --seed abc123def456...
")]
    Determinism {
        /// Stack ID to use for testing (default: first active stack)
        #[arg(long)]
        stack_id: Option<String>,

        /// Number of runs to compare (default: 3)
        #[arg(long, default_value = "3")]
        runs: usize,

        /// Seed to use (hex string, default: derived from stack manifest)
        #[arg(long)]
        seed: Option<String>,
    },

    /// Check quarantine status and verify no quarantined adapters in active stacks
    #[command(after_help = "\
Examples:
  # Check quarantine status
  aosctl quarantine

  # Check with verbose output
  aosctl quarantine --verbose
")]
    Quarantine {
        /// Verbose output
        #[arg(long)]
        verbose: bool,
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

    /// Launch interactive TUI dashboard (requires --features tui)
    #[command(after_help = "\
Examples:
  # Launch TUI dashboard
  aosctl tui

  # Launch with custom server URL
  aosctl tui --server-url http://localhost:9000
")]
    Tui {
        /// Server URL for API connections (default: http://localhost:8080)
        #[arg(long, env = "AOS_SERVER_URL")]
        server_url: Option<String>,
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

    /// Training job commands (start/status/list) - control plane
    #[command(subcommand)]
    Train(commands::train_cli::TrainCommand),

    /// Train adapter on documentation markdown files
    #[command(after_help = "\
Examples:
  # Train on all docs/*.md files with auto-activation
  aosctl train-docs

  # Train with custom settings
  aosctl train-docs --docs-dir ./my-docs --revision v2

  # Dry run to preview what would be trained
  aosctl train-docs --dry-run
")]
    TrainDocs {
        #[command(flatten)]
        args: train_docs::TrainDocsArgs,
    },

    /// Initialize AdapterOS system (Owner Home setup)
    #[command(after_help = "\
Examples:
  # Initialize system with default settings
  aosctl init

  # Initialize with custom owner email
  aosctl init --owner-email admin@example.com

  # Initialize with custom database and URLs
  aosctl init --database-url sqlite://./custom.db \\
    --ui-url http://localhost:3000 \\
    --api-url http://localhost:9000

  # Skip interactive prompts
  aosctl init --yes

  # Skip creating config file
  aosctl init --skip-config
")]
    Init {
        #[command(flatten)]
        args: init::InitArgs,
    },

    // ============================================================
    // Code Intelligence Commands
    // ============================================================
    /// Code intelligence commands (init, update, list, status)
    #[command(subcommand)]
    Code(code::CodeCommand),

    // ============================================================
    // Deprecated Commands (hidden, for backward compatibility)
    // ============================================================
    /// List adapters (deprecated - use `adapter list`)
    #[command(name = "adapter-list", hide = true)]
    AdapterListDeprecated {
        /// Filter by tier
        #[arg(short, long)]
        tier: Option<String>,
        /// Include metadata
        #[arg(long)]
        include_meta: bool,
    },

    /// Pin adapter (deprecated - use `adapter pin`)
    #[command(name = "adapter-pin", hide = true)]
    AdapterPinDeprecated {
        /// Adapter ID
        adapter_id: String,
        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// Unpin adapter (deprecated - use `adapter unpin`)
    #[command(name = "adapter-unpin", hide = true)]
    AdapterUnpinDeprecated {
        /// Adapter ID
        adapter_id: String,
        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// List nodes (deprecated - use `node list`)
    #[command(name = "node-list", hide = true)]
    NodeListDeprecated {
        /// Offline mode
        #[arg(long)]
        offline: bool,
    },

    /// Verify nodes (deprecated - use `node verify`)
    #[command(name = "node-verify", hide = true)]
    NodeVerifyDeprecated {
        /// Verify all nodes
        #[arg(long)]
        all: bool,
        /// Specific node IDs
        #[arg(long, value_delimiter = ',')]
        nodes: Option<Vec<String>>,
    },

    /// List telemetry events (deprecated - use `telemetry list`)
    #[command(name = "telemetry-list", hide = true)]
    TelemetryListDeprecated {
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,
        /// Filter by stack ID
        #[arg(long)]
        by_stack: Option<String>,
        /// Maximum results
        #[arg(long, default_value = "50")]
        limit: u32,
    },

    /// Verify telemetry (deprecated - use `telemetry verify`)
    #[command(name = "telemetry-verify", hide = true)]
    TelemetryVerifyDeprecated {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,
    },

    /// Sync registry (deprecated - use `registry sync`)
    #[command(name = "registry-sync", hide = true)]
    RegistrySyncDeprecated {
        /// Directory containing adapters
        #[arg(short, long)]
        dir: PathBuf,
        /// CAS root directory
        #[arg(long, default_value = "./var/cas")]
        cas_root: PathBuf,
        /// Registry database path
        #[arg(long, default_value = "./var/registry.db")]
        registry: PathBuf,
    },

    /// Migrate registry (deprecated - use `registry migrate`)
    #[command(name = "registry-migrate", hide = true)]
    RegistryMigrateDeprecated {
        /// Source database
        #[arg(long, default_value = "deprecated/registry.db")]
        from_db: PathBuf,
        /// Target database
        #[arg(long, default_value = "var/registry.db")]
        to_db: PathBuf,
        /// Dry run
        #[arg(long)]
        dry_run: bool,
        /// Force migration
        #[arg(long)]
        force: bool,
    },

    /// Verify federation (deprecated - use `federation verify`)
    #[command(name = "federation-verify", hide = true)]
    FederationVerifyDeprecated {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,
        /// Database path
        #[arg(long, default_value = "./var/cp.db")]
        database: PathBuf,
    },

    /// Initialize code repository (deprecated - use `code init`)
    #[command(name = "code-init", hide = true)]
    CodeInitDeprecated {
        /// Path to the repository
        repo_path: PathBuf,
        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },

    /// Update code repository (deprecated - use `code update`)
    #[command(name = "code-update", hide = true)]
    CodeUpdateDeprecated {
        /// Repository ID
        repo_id: String,
        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
        /// Specific commit
        #[arg(long)]
        commit: Option<String>,
    },

    /// List code repositories (deprecated - use `code list`)
    #[command(name = "code-list", hide = true)]
    CodeListDeprecated {
        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },

    /// Get code repository status (deprecated - use `code status`)
    #[command(name = "code-status", hide = true)]
    CodeStatusDeprecated {
        /// Repository ID
        repo_id: String,
        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },

    /// Show secd status (deprecated - use `secd status`)
    #[cfg(feature = "secd-support")]
    #[command(name = "secd-status", hide = true)]
    SecdStatusDeprecated {
        /// PID file path
        #[arg(long, default_value = "/var/run/aos-secd.pid")]
        pid_file: PathBuf,
        /// Heartbeat file path
        #[arg(long, default_value = "/var/run/aos-secd.heartbeat")]
        heartbeat_file: PathBuf,
        /// Socket path
        #[arg(long, default_value = "/var/run/aos-secd.sock")]
        socket: PathBuf,
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,
    },

    /// Show secd audit (deprecated - use `secd audit`)
    #[cfg(feature = "secd-support")]
    #[command(name = "secd-audit", hide = true)]
    SecdAuditDeprecated {
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,
        /// Number of operations to show
        #[arg(short, long, default_value = "50")]
        limit: i64,
        /// Filter by operation type
        #[arg(short, long)]
        operation: Option<String>,
    },

    /// Show codegraph stats (deprecated - use `codegraph stats`)
    #[cfg(feature = "codegraph")]
    #[command(name = "codegraph-stats", hide = true)]
    CodegraphStatsDeprecated {
        /// CodeGraph database path
        #[arg(short, long)]
        codegraph_db: PathBuf,
    },

    /// Export call graph (deprecated - use `codegraph export`)
    #[cfg(feature = "codegraph")]
    #[command(name = "callgraph-export", hide = true)]
    CallgraphExportDeprecated {
        /// CodeGraph database path
        #[arg(short, long)]
        codegraph_db: PathBuf,
        /// Output file path
        #[arg(short, long)]
        output: PathBuf,
        /// Export format
        #[arg(short, long, default_value = "dot")]
        format: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file first (before anything else reads env vars)
    adapteros_config::load_dotenv();
    // Preload env defaults from stored CLI login (if present)
    auth_store::preload_env_from_store();

    // Initialize unified logging
    init_logging()?;

    let cli = Cli::parse();

    // Create output writer based on global flags
    let output_mode = OutputMode::from_flags(cli.json, cli.quiet);
    let output = OutputWriter::new(output_mode, cli.verbose);

    // Get command name for telemetry
    let command_name = get_command_name(&cli.command);
    let tenant_id = extract_tenant_from_command(&cli.command);
    auth_store::warn_if_tenant_mismatch(tenant_id.as_deref(), &output);

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
        // Auth management
        Commands::Auth(cmd) => {
            commands::auth_cli::handle_auth_command(cmd.clone(), &output).await?;
        }

        // System Initialization (Owner Home)
        Commands::Init { args } => {
            init::run(args.clone(), output).await?;
        }

        // Tenant Management
        Commands::TenantInit { id, uid, gid } => {
            init_tenant::run(&id, *uid, *gid, &output).await?;
        }

        // Adapter Management
        Commands::Adapter(cmd) => {
            adapter::handle_adapter_command(cmd.clone(), &output).await?;
        }

        // Repository + Version Management
        Commands::Repo(cmd) => {
            commands::repo::run_repo_command(cmd.clone(), cli.json).await?;
        }

        // Adapter Stack Management
        Commands::Stack(cmd) => {
            stack::handle_stack_command(cmd.clone(), &output).await?;
        }

        // Interactive Chat
        Commands::Chat(cmd) => {
            chat::handle_chat_command(cmd.clone(), &output).await?;
        }

        // Development Commands
        Commands::Dev { cmd } => {
            if let Some(inner) = cmd {
                dev::handle_dev_command(inner.clone(), &output).await?;
            } else {
                dev::dev_all(&output).await?;
            }
        }

        // Scenario readiness utilities
        Commands::Scenario(cmd) => {
            commands::scenario::run(cmd.clone(), &output).await?;
        }

        // CoreML verification status
        Commands::Coreml(cmd) => {
            commands::coreml_status::run(cmd.clone(), &output).await?;
        }

        // CoreML export pipeline
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExport {
            base_package,
            adapter_aos,
            output_package,
            compute_units,
            base_model_id,
            adapter_id,
        } => {
            commands::coreml_export::run(
                base_package.clone(),
                adapter_aos.clone(),
                output_package.clone(),
                compute_units.clone(),
                base_model_id.clone(),
                adapter_id.clone(),
                &output,
            )
            .await?;
        }
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExportJob { job_id, base_url } => {
            commands::coreml_export::trigger_export_for_job(job_id, base_url, &output).await?;
        }
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExportStatus { job_id, base_url } => {
            commands::coreml_export::show_export_status(job_id, base_url, &output).await?;
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

        // System Health Diagnostics
        Commands::Doctor(cmd) => {
            commands::doctor::run(cmd.clone(), &output).await?;
        }

        // Post-reboot Startup Verification
        Commands::Check(cmd) => {
            commands::check::run(cmd.clone(), &output).await?;
        }

        // Pre-flight System Readiness Check
        Commands::Preflight(cmd) => {
            commands::preflight::run(cmd.clone(), &output).await?;
        }

        // Maintenance
        Commands::Maintenance(cmd) => {
            commands::maintenance::run(cmd.clone(), &output).await?;
        }

        // Registry Management
        Commands::Registry(cmd) => {
            registry::handle_registry_command(cmd.clone(), &output).await?;
        }

        // Storage Management
        Commands::Storage(cmd) => {
            storage::handle_storage_command(cmd.clone(), &output).await?;
        }

        // Database Management
        Commands::Db(cmd) => {
            commands::db::handle_db_command(cmd.clone(), &output).await?;
        }

        // Review Management
        Commands::Review(cmd) => {
            commands::review::handle_review_command(cmd.clone(), &output).await?;
        }

        // Model Management
        Commands::Models(cmd) => {
            commands::models::handle_models_command(cmd.clone(), &output).await?;
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

        Commands::Trace(trace_cmd) => match trace_cmd {
            TraceCommand::Export {
                request,
                out,
                fixtures,
            } => {
                let expectation =
                    cmd_trace_export::run(request, out, fixtures.as_deref(), &output)?;
                if output.mode().is_json() {
                    output.print_json(&serde_json::to_value(&expectation)?)?;
                } else if output.is_verbose() {
                    output.progress(format!(
                        "Expected receipt: {}",
                        expectation.expected_receipt
                    ));
                }
            }
        },

        Commands::Federation(cmd) => {
            federation::handle_federation_command(cmd.clone(), &output).await?;
        }

        Commands::DriftCheck {
            config,
            dataset,
            manifest,
            backend,
            reference_backend,
        } => {
            std::process::exit(
                commands::drift_check::drift_check(commands::drift_check::DriftCheckArgs {
                    config: config.clone(),
                    dataset_override: dataset.clone(),
                    manifest_override: manifest.clone(),
                    backends_override: backend.clone(),
                    reference_backend: reference_backend.clone(),
                })
                .await?,
            );
        }

        // CodeGraph & Call Graph
        #[cfg(feature = "codegraph")]
        Commands::Codegraph(cmd) => {
            codegraph::handle_codegraph_command(cmd.clone(), &output).await?;
        }

        // Security Daemon
        #[cfg(feature = "secd-support")]
        Commands::Secd(cmd) => {
            secd::handle_secd_command(cmd.clone()).await?;
        }

        // General Operations
        Commands::Import { bundle, no_verify } => {
            import::run(&bundle, !no_verify, &output).await?;
        }
        Commands::Verify {
            target,
            trace,
            bundle,
            base_url,
        } => {
            let as_trace = *trace || (!bundle && !Path::new(target).exists());
            if as_trace {
                verify::verify_trace_receipt(target.clone(), base_url, &output).await?;
            } else {
                verify::run(Path::new(target), &output).await?;
            }
        }
        Commands::VerifyReceipt {
            bundle,
            online,
            server_url,
        } => {
            commands::verify_receipt::run(
                bundle.as_deref(),
                online.as_deref(),
                server_url,
                &output,
            )
            .await?;
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
        Commands::Replay {
            dir,
            report,
            verify,
        } => {
            let report_path = report.as_ref().map(|p| p.as_path());
            let replay_report = cmd_replay::run(dir, *verify, report_path, &output)?;

            if output.mode().is_json() {
                output.print_json(&serde_json::to_value(&replay_report)?)?;
            } else if output.is_verbose() {
                output.progress(format!("Replay status: {}", replay_report.status));
            }
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
        Commands::Diag(cmd) => {
            diag::handle_diag_command(cmd.clone(), &output).await?;
        }

        Commands::LogDigest(cmd) => {
            commands::log_digest::run(cmd.clone(), &output).await?;
        }

        Commands::LogTriage(cmd) => {
            commands::log_triage::run(cmd.clone(), &output).await?;
        }

        Commands::LogPrompt(cmd) => {
            commands::log_prompt::run(cmd.clone(), &output).await?;
        }

        Commands::Health(cmd) => {
            commands::diag_health::run(cmd.clone(), &output).await?;
        }

        Commands::Determinism {
            stack_id,
            runs,
            seed,
        } => {
            diag::run_determinism_check(stack_id.clone(), *runs, seed.clone(), &output).await?;
        }
        Commands::Quarantine { verbose } => {
            diag::run_quarantine_check(*verbose, &output).await?;
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

        // TUI Dashboard
        Commands::Tui { server_url } => {
            #[cfg(feature = "tui")]
            {
                commands::tui::run(commands::tui::TuiArgs {
                    server_url: server_url.clone(),
                })
                .await?;
            }
            #[cfg(not(feature = "tui"))]
            {
                let _ = server_url; // Suppress unused warning
                anyhow::bail!("TUI feature not enabled. Rebuild with: cargo build --features tui");
            }
        }

        Commands::Manual { args } => {
            commands::manual::run_manual(args.clone())?;
        }

        Commands::Train(cmd) => {
            commands::train_cli::run(cmd.clone(), &output).await?;
        }

        Commands::TrainDocs { args } => {
            args.execute().await?;
        }

        // Code Intelligence Commands
        Commands::Code(cmd) => {
            code::handle_code_command(cmd.clone(), &output).await?;
        }

        // ============================================================
        // Deprecated Commands (backward compatibility)
        // ============================================================
        Commands::AdapterListDeprecated { .. } => {
            eprintln!("Warning: 'adapter-list' is deprecated. Use 'aosctl adapter list' instead.");
            adapter::handle_adapter_command(
                adapter::AdapterCommand::List {
                    json: cli.json,
                    tenant: None,
                    pinned_only: false,
                },
                &output,
            )
            .await?;
        }

        Commands::AdapterPinDeprecated { adapter_id, tenant } => {
            eprintln!("Warning: 'adapter-pin' is deprecated. Use 'aosctl adapter pin' instead.");
            adapter::handle_adapter_command(
                adapter::AdapterCommand::Pin {
                    adapter_id: adapter_id.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::AdapterUnpinDeprecated { adapter_id, tenant } => {
            eprintln!(
                "Warning: 'adapter-unpin' is deprecated. Use 'aosctl adapter unpin' instead."
            );
            adapter::handle_adapter_command(
                adapter::AdapterCommand::Unpin {
                    adapter_id: adapter_id.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::NodeListDeprecated { offline } => {
            eprintln!("Warning: 'node-list' is deprecated. Use 'aosctl node list' instead.");
            node::handle_node_command(
                node::NodeCommand::List {
                    offline: *offline,
                    json: cli.json,
                },
                &output,
            )
            .await?;
        }

        Commands::NodeVerifyDeprecated { all, nodes } => {
            eprintln!("Warning: 'node-verify' is deprecated. Use 'aosctl node verify' instead.");
            node::handle_node_command(
                node::NodeCommand::Verify {
                    all: *all,
                    nodes: nodes.clone(),
                    json: cli.json,
                },
                &output,
            )
            .await?;
        }

        Commands::TelemetryListDeprecated {
            database,
            by_stack,
            limit,
        } => {
            eprintln!(
                "Warning: 'telemetry-list' is deprecated. Use 'aosctl telemetry list' instead."
            );
            telemetry::handle_telemetry_command(
                telemetry::TelemetryCommand::List {
                    database: database.clone(),
                    by_stack: by_stack.clone(),
                    event_type: None,
                    limit: *limit,
                },
                &output,
            )
            .await?;
        }

        Commands::TelemetryVerifyDeprecated { bundle_dir } => {
            eprintln!(
                "Warning: 'telemetry-verify' is deprecated. Use 'aosctl telemetry verify' instead."
            );
            telemetry::handle_telemetry_command(
                telemetry::TelemetryCommand::Verify {
                    bundle_dir: bundle_dir.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::RegistrySyncDeprecated {
            dir,
            cas_root,
            registry: registry_path,
        } => {
            eprintln!(
                "Warning: 'registry-sync' is deprecated. Use 'aosctl registry sync' instead."
            );
            registry::handle_registry_command(
                registry::RegistryCommand::Sync {
                    dir: dir.clone(),
                    cas_root: cas_root.clone(),
                    registry: registry_path.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::RegistryMigrateDeprecated {
            from_db,
            to_db,
            dry_run,
            force,
        } => {
            eprintln!(
                "Warning: 'registry-migrate' is deprecated. Use 'aosctl registry migrate' instead."
            );
            registry::handle_registry_command(
                registry::RegistryCommand::Migrate(registry::RegistryMigrateArgs {
                    from_db: from_db.clone(),
                    to_db: to_db.clone(),
                    dry_run: *dry_run,
                    force: *force,
                }),
                &output,
            )
            .await?;
        }

        Commands::FederationVerifyDeprecated {
            bundle_dir,
            database,
        } => {
            eprintln!("Warning: 'federation-verify' is deprecated. Use 'aosctl federation verify' instead.");
            federation::handle_federation_command(
                federation::FederationCommand::Verify {
                    bundle_dir: bundle_dir.clone(),
                    database: database.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeInitDeprecated { repo_path, tenant } => {
            eprintln!("Warning: 'code-init' is deprecated. Use 'aosctl code init' instead.");
            code::handle_code_command(
                code::CodeCommand::Init {
                    repo_path: repo_path.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeUpdateDeprecated {
            repo_id,
            tenant,
            commit,
        } => {
            eprintln!("Warning: 'code-update' is deprecated. Use 'aosctl code update' instead.");
            code::handle_code_command(
                code::CodeCommand::Update {
                    repo_id: repo_id.clone(),
                    tenant: tenant.clone(),
                    commit: commit.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeListDeprecated { tenant } => {
            eprintln!("Warning: 'code-list' is deprecated. Use 'aosctl code list' instead.");
            code::handle_code_command(
                code::CodeCommand::List {
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        Commands::CodeStatusDeprecated { repo_id, tenant } => {
            eprintln!("Warning: 'code-status' is deprecated. Use 'aosctl code status' instead.");
            code::handle_code_command(
                code::CodeCommand::Status {
                    repo_id: repo_id.clone(),
                    tenant: tenant.clone(),
                },
                &output,
            )
            .await?;
        }

        #[cfg(feature = "secd-support")]
        Commands::SecdStatusDeprecated {
            pid_file,
            heartbeat_file,
            socket,
            database,
        } => {
            eprintln!("Warning: 'secd-status' is deprecated. Use 'aosctl secd status' instead.");
            secd::handle_secd_command(secd::SecdCommand::Status {
                pid_file: pid_file.clone(),
                heartbeat_file: heartbeat_file.clone(),
                socket: socket.clone(),
                database: database.clone(),
            })
            .await?;
        }

        #[cfg(feature = "secd-support")]
        Commands::SecdAuditDeprecated {
            database,
            limit,
            operation,
        } => {
            eprintln!("Warning: 'secd-audit' is deprecated. Use 'aosctl secd audit' instead.");
            secd::handle_secd_command(secd::SecdCommand::Audit {
                database: database.clone(),
                limit: *limit,
                operation: operation.clone(),
            })
            .await?;
        }

        #[cfg(feature = "codegraph")]
        Commands::CodegraphStatsDeprecated { codegraph_db } => {
            eprintln!(
                "Warning: 'codegraph-stats' is deprecated. Use 'aosctl codegraph stats' instead."
            );
            codegraph::handle_codegraph_command(
                codegraph::CodegraphCommand::Stats {
                    codegraph_db: codegraph_db.clone(),
                },
                &output,
            )
            .await?;
        }

        #[cfg(feature = "codegraph")]
        Commands::CallgraphExportDeprecated {
            codegraph_db,
            output: output_path,
            format,
        } => {
            eprintln!(
                "Warning: 'callgraph-export' is deprecated. Use 'aosctl codegraph export' instead."
            );
            codegraph::handle_codegraph_command(
                codegraph::CodegraphCommand::Export {
                    codegraph_db: codegraph_db.clone(),
                    output: output_path.clone(),
                    format: format.clone(),
                },
                &output,
            )
            .await?;
        }
    }

    Ok(())
}

/// Get command name from Commands enum
fn get_command_name(command: &Commands) -> String {
    match command {
        Commands::Auth(_) => "auth",
        Commands::TenantInit { .. } | Commands::Init { .. } => "init-tenant",
        Commands::Adapter(_) => "adapter",
        Commands::Repo(_) => "repo",
        Commands::Stack(_) => "stack",
        Commands::Chat(_) => "chat",
        Commands::Dev { .. } => "dev",
        Commands::Scenario(_) => "scenario",
        Commands::Coreml(_) => "coreml",
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExport { .. } => "coreml-export",
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExportJob { .. } => "coreml-export-job",
        #[cfg(feature = "coreml-export")]
        Commands::CoremlExportStatus { .. } => "coreml-export-status",
        Commands::Node(_) => "node",
        Commands::Status { .. } => "status",
        Commands::Doctor { .. } => "doctor",
        Commands::Check(_) => "check",
        Commands::Maintenance { .. } => "maintenance",
        Commands::Deploy { .. } => "deploy",
        Commands::Registry(_) => "registry",
        Commands::Storage(_) => "storage",
        Commands::Db(_) => "db",
        Commands::Review(_) => "review",
        Commands::Models(_) => "models",
        Commands::PlanBuild { .. } => "build-plan",
        Commands::ModelImport { .. } => "import-model",
        Commands::Telemetry(_) => "telemetry",
        Commands::Trace(_) => "trace",
        Commands::Federation(_) => "federation",
        Commands::DriftCheck { .. } => "drift-check",
        #[cfg(feature = "codegraph")]
        Commands::Codegraph(_) => "codegraph",
        #[cfg(feature = "secd-support")]
        Commands::Secd(_) => "secd",
        Commands::Import { .. } => "import",
        Commands::Verify { .. } => "verify",
        Commands::VerifyReceipt { .. } => "verify-receipt",
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
        Commands::Diag(_) => "diag",
        Commands::LogDigest(_) => "log-digest",
        Commands::LogTriage(_) => "log-triage",
        Commands::LogPrompt(_) => "log-prompt",
        Commands::Health { .. } => "health",
        Commands::Determinism { .. } => "determinism",
        Commands::Quarantine { .. } => "quarantine",
        Commands::Explain { .. } => "explain",
        Commands::ErrorCodes { .. } => "error-codes",
        Commands::Tutorial { .. } => "tutorial",
        Commands::Manual { .. } => "manual",
        Commands::Train(_) => "train",
        Commands::TrainDocs { .. } => "train-docs",
        Commands::Code(_) => "code",
        Commands::BackendStatus(_) => "backend-status",
        Commands::Tui { .. } => "tui",
        // Deprecated commands
        Commands::AdapterListDeprecated { .. } => "adapter-list",
        Commands::AdapterPinDeprecated { .. } => "adapter-pin",
        Commands::AdapterUnpinDeprecated { .. } => "adapter-unpin",
        Commands::NodeListDeprecated { .. } => "node-list",
        Commands::NodeVerifyDeprecated { .. } => "node-verify",
        Commands::TelemetryListDeprecated { .. } => "telemetry-list",
        Commands::TelemetryVerifyDeprecated { .. } => "telemetry-verify",
        Commands::RegistrySyncDeprecated { .. } => "registry-sync",
        Commands::RegistryMigrateDeprecated { .. } => "registry-migrate",
        Commands::FederationVerifyDeprecated { .. } => "federation-verify",
        Commands::CodeInitDeprecated { .. } => "code-init",
        Commands::CodeUpdateDeprecated { .. } => "code-update",
        Commands::CodeListDeprecated { .. } => "code-list",
        Commands::CodeStatusDeprecated { .. } => "code-status",
        #[cfg(feature = "secd-support")]
        Commands::SecdStatusDeprecated { .. } => "secd-status",
        #[cfg(feature = "secd-support")]
        Commands::SecdAuditDeprecated { .. } => "secd-audit",
        #[cfg(feature = "codegraph")]
        Commands::CodegraphStatsDeprecated { .. } => "codegraph-stats",
        #[cfg(feature = "codegraph")]
        Commands::CallgraphExportDeprecated { .. } => "callgraph-export",
        Commands::Preflight(_) => "preflight",
    }
    .to_string()
}

/// Extract tenant ID from command if present
fn extract_tenant_from_command(command: &Commands) -> Option<String> {
    match command {
        Commands::Serve { tenant, .. } | Commands::Rollback { tenant, .. } => Some(tenant.clone()),
        Commands::Diag(diag::DiagCommand::Run { tenant, .. }) => tenant.clone(),
        Commands::Repo(commands::repo::RepoCommand::Repo(commands::repo::RepoOps::Create(
            args,
        ))) => Some(args.tenant.clone()),
        Commands::Repo(commands::repo::RepoCommand::Repo(commands::repo::RepoOps::List {
            tenant,
            ..
        })) => Some(tenant.clone()),
        // Tenant extraction for grouped commands is handled by their respective handlers
        _ => None,
    }
}

// Logging initialization moved to logging module
