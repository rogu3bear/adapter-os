//! AdapterOS CLI tool (aosctl)

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

use crate::cli_telemetry;
use crate::commands;
use crate::commands::*;
use crate::commands::{baseline::BaselineCmd, golden::GoldenCmd};
use crate::logging::init_logging;
use crate::output::{OutputMode, OutputWriter};

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
pub struct Cli {
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
}

#[derive(Subcommand)]
pub enum Commands {
    // ============================================================
    // Tenant Management
    // ============================================================
    /// Initialize a new tenant
    #[command(after_help = r#"Examples:
  # Create a development tenant
  aosctl init-tenant --id tenant_dev --uid 1000 --gid 1000

  # Create a production tenant with custom IDs
  aosctl init-tenant --id tenant_prod --uid 5000 --gid 5000

  # Quick alias (hidden)
  aosctl init --id tenant_test --uid 1000 --gid 1000
"#)]
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
    /// List adapters in the registry
    #[command(
        alias = "list-adapters",
        after_help = r#"Examples:
  aosctl list-adapters
  aosctl list-adapters --tier persistent
  aosctl list-adapters --json > adapters.json
"#
    )]
    AdapterList {
        /// Filter by tier
        #[arg(short, long)]
        tier: Option<String>,
    },

    /// Register a new adapter
    #[command(after_help = r#"Examples:
  # Register a persistent adapter
  aosctl register-adapter my_adapter b3:abc123... --tier persistent --rank 16

  # Register an ephemeral adapter (default)
  aosctl register-adapter temp_fix b3:def456... --rank 8

  # High-rank adapter for complex tasks
  aosctl register-adapter specialist b3:789ghi... --tier persistent --rank 32
"#)]
    AdapterRegister {
        /// Adapter ID
        id: String,

        /// Artifact hash
        hash: String,

        /// Tier (persistent or ephemeral)
        #[arg(short, long, default_value = "ephemeral")]
        tier: String,

        /// Rank
        #[arg(short, long)]
        rank: u32,
    },

    /// Pin adapter to prevent eviction
    #[command(after_help = r#"Examples:
  # Pin adapter permanently
  aosctl pin-adapter --tenant dev --adapter specialist --reason "Production critical"

  # Pin adapter with TTL
  aosctl pin-adapter --tenant dev --adapter temp_fix --ttl-hours 24 --reason "Testing"

  # List pinned adapters
  aosctl list-pinned --tenant dev
"#)]
    AdapterPin {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Adapter ID
        #[arg(short, long)]
        adapter: String,

        /// TTL in hours (omit for permanent pin)
        #[arg(long)]
        ttl_hours: Option<u64>,

        /// Reason for pinning
        #[arg(short, long)]
        reason: String,
    },

    /// Unpin adapter to allow eviction
    #[command(after_help = r#"Examples:
  # Unpin adapter
  aosctl unpin-adapter --tenant dev --adapter temp_fix

  # Verify unpinning
  aosctl list-pinned --tenant dev
"#)]
    AdapterUnpin {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Adapter ID
        #[arg(short, long)]
        adapter: String,
    },

    /// List pinned adapters
    #[command(after_help = r#"Examples:
  # List all pinned adapters for tenant
  aosctl list-pinned --tenant dev

  # Check specific adapter status
  aosctl adapter-info specialist
"#)]
    AdapterListPinned {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,
    },

    /// Hot-swap adapters in running worker
    #[command(after_help = r#"Examples:
  # Dry-run adapter swap
  aosctl adapter-swap --tenant dev --add specialist --remove temp_fix

  # Commit adapter swap
  aosctl adapter-swap --tenant dev --add specialist --remove temp_fix --commit

  # Add multiple adapters
  aosctl adapter-swap --tenant dev --add adapter1,adapter2 --commit
"#)]
    AdapterSwap {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Adapter IDs to add (comma-separated)
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,

        /// Adapter IDs to remove (comma-separated)
        #[arg(long, value_delimiter = ',')]
        remove: Vec<String>,

        /// Timeout in milliseconds
        #[arg(long, default_value = "5000")]
        timeout: u64,

        /// Commit the swap (otherwise dry-run)
        #[arg(long)]
        commit: bool,

        /// UDS socket path
        #[arg(long, default_value = "/var/run/aos/aos.sock")]
        socket: PathBuf,
    },

    /// Show adapter information and provenance
    #[command(after_help = r#"Examples:
  # Show adapter details
  aosctl adapter-info specialist

  # Check adapter compatibility
  aosctl adapter-info temp_fix --check-compatibility

  # Show adapter provenance
  aosctl adapter-info specialist --show-provenance
"#)]
    AdapterInfo {
        /// Adapter ID
        adapter_id: String,
    },

    /// Adapters group commands
    #[command(subcommand)]
    Adapters(commands::adapters::AdaptersCmd),

    /// Adapter lifecycle management commands
    #[command(subcommand)]
    Adapter(adapter::AdapterCommand),

    // ============================================================
    // Node & Cluster Management
    // ============================================================
    /// List cluster nodes
    #[command(after_help = r#"Examples:
  # List all nodes
  aosctl node-list

  # List nodes offline (cached)
  aosctl node-list --offline

  # Check node status
  aosctl node-status node1
"#)]
    NodeList {
        /// Offline mode (use cached database state)
        #[arg(long)]
        offline: bool,
    },

    /// Verify cross-node determinism
    #[command(after_help = r#"Examples:
  # Verify all nodes
  aosctl node-verify --all

  # Verify specific nodes
  aosctl node-verify --nodes node1,node2

  # Check determinism across cluster
  aosctl node-verify --all --verbose
"#)]
    NodeVerify {
        /// Verify all nodes
        #[arg(long)]
        all: bool,

        /// Specific node IDs to verify (comma-separated)
        #[arg(long, value_delimiter = ',')]
        nodes: Option<Vec<String>>,
    },

    /// Sync adapters across nodes
    NodeSync {
        #[command(subcommand)]
        mode: NodeSyncMode,
    },

    // ============================================================
    // Registry Management
    // ============================================================
    /// Sync adapters from local directory to registry
    #[command(after_help = r#"Examples:
  # Sync adapters from directory
  aosctl sync-registry --dir ./adapters

  # Sync with custom CAS root
  aosctl sync-registry --dir ./adapters --cas-root ./var/cas

  # Sync to custom registry
  aosctl sync-registry --dir ./adapters --registry ./var/custom.db
"#)]
    RegistrySync {
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

    // ============================================================
    // Plan Management
    // ============================================================
    /// Build a plan from manifest
    #[command(after_help = r#"Examples:
  # Build plan from YAML manifest
  aosctl build-plan manifests/qwen7b.yaml --output plan/qwen7b/plan.bin

  # Build plan for production
  aosctl build-plan manifests/production.yaml --output plan/prod_v1/plan.bin
"#)]
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
    #[command(after_help = r#"Examples:
  # Import Qwen2.5-7B model
  aosctl import-model \
    --name qwen2.5-7b \
    --weights models/qwen2.5-7b-mlx/weights.safetensors \
    --config models/qwen2.5-7b-mlx/config.json \
    --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
    --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \
    --license models/qwen2.5-7b-mlx/LICENSE
"#)]
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
    /// Verify telemetry bundle chain
    #[command(after_help = r#"Examples:
  aosctl telemetry-verify --bundle-dir ./var/telemetry
  aosctl telemetry-verify --bundle-dir ./var/telemetry --json > verify.json
"#)]
    TelemetryVerify {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,
    },

    /// Validate a trace file for integrity and limits
    #[command(after_help = r#"Examples:
  # Strict validation with hash checks
  aosctl trace-validate /path/to/trace.ndjson --verify-hash

  # Tolerant validation (skip bad lines), limit events and bytes
  aosctl trace-validate /path/to/trace.ndjson.zst --tolerant --max-events 10000 --max-bytes 104857600

  # Enforce a max line length guard
  aosctl trace-validate /path/to/trace.ndjson --max-line-len 1048576
"#)]
    TraceValidate {
        /// Path to trace file (.ndjson or .ndjson.zst)
        path: PathBuf,

        /// Strict mode (default). Use --tolerant to skip invalid lines/events.
        #[arg(long, default_value_t = true, conflicts_with = "tolerant")]
        strict: bool,

        /// Tolerant mode (skip invalid lines/events and continue)
        #[arg(long, conflicts_with = "strict")]
        tolerant: bool,

        /// Verify per-event hashes
        #[arg(long, default_value_t = false)]
        verify_hash: bool,

        /// Maximum number of events to read
        #[arg(long)]
        max_events: Option<usize>,

        /// Maximum total bytes to read
        #[arg(long)]
        max_bytes: Option<u64>,

        /// Maximum line length in bytes
        #[arg(long)]
        max_line_len: Option<usize>,
    },

    /// Verify cross-host federation signatures
    #[command(after_help = r#"Examples:
  aosctl federation-verify --bundle-dir ./var/telemetry
  aosctl federation-verify --bundle-dir ./var/telemetry --database ./var/cp.db
  aosctl federation-verify --bundle-dir ./var/telemetry --json > federation.json
"#)]
    FederationVerify {
        /// Telemetry bundle directory
        #[arg(short, long)]
        bundle_dir: PathBuf,

        /// Database path
        #[arg(long, default_value = "./var/cp.db")]
        database: PathBuf,
    },

    /// Check for environment drift
    #[command(after_help = r#"Examples:
  # Check drift against baseline
  aosctl drift-check

  # Check drift and save current fingerprint
  aosctl drift-check --save-current

  # Create initial baseline
  aosctl drift-check --save-baseline

  # Use custom baseline path
  aosctl drift-check --baseline var/production_baseline.json
"#)]
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
    /// Export call graph to various formats
    #[command(after_help = r#"Examples:
  aosctl callgraph-export --codegraph-db ./var/codegraph.db --output graph.dot
  aosctl callgraph-export --codegraph-db ./var/codegraph.db --output graph.json --format json
"#)]
    CallgraphExport {
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

    /// Generate CodeGraph statistics
    #[command(after_help = r#"Examples:
  # Generate statistics
  aosctl codegraph-stats --codegraph-db ./var/codegraph.db

  # Export statistics to JSON
  aosctl codegraph-stats --codegraph-db ./var/codegraph.db --json > stats.json
"#)]
    CodegraphStats {
        /// CodeGraph database path
        #[arg(short, long)]
        codegraph_db: PathBuf,
    },

    /// Show aos-secd daemon status
    #[command(after_help = r#"Examples:
  aosctl secd-status
  aosctl secd-status --database ./var/custom.db
  aosctl secd-status --json > secd.json
"#)]
    SecdStatus {
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

    /// Show aos-secd operation audit trail
    SecdAudit {
        /// Database path
        #[arg(long, default_value = "./var/aos-cp.sqlite3")]
        database: PathBuf,

        /// Number of operations to show
        #[arg(short, long, default_value = "50")]
        limit: i64,

        /// Filter by operation type (sign, seal, unseal, get_public_key)
        #[arg(short, long)]
        operation: Option<String>,
    },

    // ============================================================
    // AOS File Operations
    // ============================================================
    /// AOS adapter file operations (create, verify, info, convert)
    #[command(subcommand)]
    Aos(commands::aos::AosCmd),

    // ============================================================
    // General Operations
    // ============================================================
    /// Import an artifact bundle
    #[command(after_help = r#"Examples:
  # Import signed bundle (default, recommended)
  aosctl import artifacts/adapters.zip

  # Import without verification (dev only, not recommended)
  aosctl import bundle.zip --no-verify
"#)]
    Import {
        /// Bundle path
        bundle: PathBuf,

        /// Skip signature verification
        #[arg(long)]
        no_verify: bool,
    },

    /// Verify a bundle
    #[command(after_help = r#"Examples:
  # Verify artifact bundle signature and hashes
  aosctl verify artifacts/adapters.zip

  # Verify telemetry bundle chain
  aosctl verify-telemetry --bundle-dir ./var/telemetry
"#)]
    Verify {
        /// Bundle path
        bundle: PathBuf,
    },

    /// Verify a packaged adapter directory
    #[command(after_help = r#"Examples:
  # Verify packaged adapter
  aosctl verify-adapter --adapters-root ./adapters --adapter-id demo_adapter

  # JSON output
  aosctl verify-adapter --adapters-root ./adapters --adapter-id demo_adapter --json

"#)]
    VerifyAdapter {
        /// Adapters root directory
        #[arg(long, default_value = "./adapters")]
        adapters_root: PathBuf,

        /// Adapter ID to verify
        #[arg(long)]
        adapter_id: String,
    },

    // ============================================================
    // Policy Management
    // ============================================================
    /// Manage policy packs
    #[command(subcommand)]
    #[command(after_help = r#"Examples:
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
"#)]
    Policy(policy::PolicyCommand),

    /// Start serving
    #[command(after_help = r#"Examples:
  # Validate setup without starting (recommended first)
  aosctl serve --tenant tenant_dev --plan cp_abc123 --dry-run

  # Start serving
  aosctl serve --tenant tenant_dev --plan cp_abc123

  # Custom socket path
  aosctl serve --tenant tenant_dev --plan cp_abc123 --socket /tmp/aos.sock
"#)]
    Serve {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Plan ID
        #[arg(short, long, alias = "plan-id")]
        plan: String,

        /// UDS socket path
        #[arg(short, long, default_value = "/var/run/aos/aos.sock")]
        socket: PathBuf,

        /// Backend selection: metal (default), mlx (C++ FFI, requires --features mlx-ffi-backend), or coreml
        #[arg(short, long, default_value = "metal")]
        backend: BackendType,

        /// Dry-run: validate preflight checks without starting server
        #[arg(long)]
        dry_run: bool,

        /// INSECURE: Skip PF egress preflight (development only)
        #[arg(long, hide = true)]
        insecure_skip_egress_check: bool,
        /// Capture telemetry events to this directory (overrides default)
        #[arg(long)]
        capture_events: Option<PathBuf>,
    },

    /// Run audit checks
    #[command(after_help = r#"Examples:
  # Audit checkpoint
  aosctl audit CP-0001

  # Audit with custom test suite
  aosctl audit CP-0001 --suite ./tests/production.yaml

  # Audit and generate report
  aosctl audit CP-0001 --json > audit.json
"#)]
    Audit {
        /// CPID to audit
        cpid: String,

        /// Test suite path
        #[arg(short, long)]
        suite: Option<PathBuf>,
    },

    /// Audit backend determinism attestation
    #[command(after_help = r#"Examples:
  # Audit Metal backend (default)
  aosctl audit-determinism

  # Audit with JSON output
  aosctl audit-determinism --format json

  # Audit MLX backend (requires --features mlx-ffi-backend)
  aosctl audit-determinism --backend mlx --model-path ./models/qwen2.5-7b-mlx
"#)]
    AuditDeterminism {
        #[command(flatten)]
        args: audit_determinism::AuditDeterminismArgs,
    },

    /// Run a local inference against the worker UDS
    #[command(after_help = r#"Examples:
  # Basic inference
  aosctl infer --prompt 'Hello world' --socket /var/run/aos/aos.sock

  # Inference using a specific adapter (preload+swap)
  aosctl infer --adapter my_adapter --prompt 'Use adapter' --socket /var/run/aos/aos.sock

  # Increase max tokens and timeout
  aosctl infer --prompt 'Test' --max-tokens 256 --timeout 60000

  # Show citations and trace for auditability
  aosctl infer --prompt 'Explain...' --show-citations --show-trace
"#)]
    Infer {
        /// Optional adapter to activate before inference
        #[arg(long)]
        adapter: Option<String>,

        /// Prompt text to infer on
        #[arg(long)]
        prompt: String,

        /// UDS socket path
        #[arg(long, default_value = "/var/run/aos/aos.sock")]
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
    #[command(after_help = r#"Examples:
  # Replay bundle
  aosctl replay ./var/bundles/bundle_001.zip

  # Replay with verbose output
  aosctl replay ./var/bundles/bundle_001.zip --verbose

  # Replay and check determinism
  aosctl replay ./var/bundles/bundle_001.zip --check-determinism
"#)]
    Replay {
        /// Bundle path
        bundle: PathBuf,

        /// Show divergence details (overridden by global --verbose)
        #[arg(short, long)]
        verbose: bool,
    },

    /// Rollback to previous checkpoint
    #[command(after_help = r#"Examples:
  # Rollback tenant to checkpoint
  aosctl rollback --tenant dev CP-0001

  # Rollback with confirmation
  aosctl rollback --tenant dev CP-0001 --confirm

  # Check rollback status
  aosctl rollback --tenant dev CP-0001 --dry-run
"#)]
    Rollback {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Target CPID
        cpid: String,
    },

    /// Baseline management (record/verify/delta with BLAKE3+Ed25519)
    #[command(subcommand)]
    #[command(after_help = r#"Examples:
  # Record a new baseline
  aosctl baseline record --run-id run-001 --commit abc123 --arch aarch64-apple-darwin \
    --suite deterministic-exec --artifacts ./artifacts --sign

  # Verify a baseline
  aosctl baseline verify --manifest baselines/run-001_aarch64-apple-darwin_deterministic-exec.toml

  # Compute delta between baselines
  aosctl baseline delta --baseline-a baselines/run-001.toml --baseline-b baselines/run-002.toml

  # List all baselines
  aosctl baseline list

  # Show baseline details
  aosctl baseline show baselines/run-001.toml
"#)]
    Baseline(BaselineCmd),

    /// Golden run archive management (audit reproducibility)
    #[command(subcommand)]
    #[command(after_help = r#"Examples:
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
"#)]
    Golden(GoldenCmd),

    /// Router weight calibration and management
    #[command(subcommand)]
    #[command(after_help = r#"Examples:
  # Calibrate router weights using a dataset
  aosctl router calibrate --dataset calibration.json --output weights.json

  # Validate weights on a dataset
  aosctl router validate --dataset test.json --weights weights.json

  # Show current router weights
  aosctl router show --weights weights.json
"#)]
    Router(router::RouterCmd),

    /// Generate HTML report from bundle
    #[command(after_help = r#"Examples:
  # Generate HTML report
  aosctl report ./var/bundles/bundle_001.zip --output report.html

  # Generate report with custom template
  aosctl report ./var/bundles/bundle_001.zip --output report.html --template custom.html

  # Generate report and open in browser
  aosctl report ./var/bundles/bundle_001.zip --output report.html --open
"#)]
    Report {
        /// Bundle path
        bundle: PathBuf,

        /// Output HTML file
        #[arg(short, long)]
        output: PathBuf,
    },

    // ============================================================
    // Access Management
    // ============================================================
    /// Create the initial control plane admin user with a generated password
    #[command(after_help = r#"Examples:
  # Create bootstrap admin user
  aosctl bootstrap-admin --email admin@example.com

  # Create with explicit display name
  aosctl bootstrap-admin --email admin@example.com --display-name "Site Administrator"
"#)]
    BootstrapAdmin {
        /// Email for the admin user
        #[arg(long)]
        email: String,

        /// Optional display name (defaults to email prefix)
        #[arg(long)]
        display_name: Option<String>,
    },

    /// Bootstrap AdapterOS installation
    #[command(after_help = r#"Examples:
  # Full installation
  aosctl bootstrap --mode full

  # Minimal installation
  aosctl bootstrap --mode minimal

  # Air-gapped installation
  aosctl bootstrap --mode full --air-gapped

  # Bootstrap with checkpoint
  aosctl bootstrap --mode full --checkpoint-file ./checkpoint.json
"#)]
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
    #[command(after_help = r#"Examples:
  # Generate bash completion
  aosctl completions bash > /usr/local/etc/bash_completion.d/aosctl

  # Generate zsh completion
  aosctl completions zsh > /usr/local/share/zsh/site-functions/_aosctl

  # Generate fish completion
  aosctl completions fish > ~/.config/fish/completions/aosctl.fish
"#)]
    Completions {
        /// Shell type
        #[arg(value_enum)]
        shell: Shell,
    },

    // ============================================================
    // Documentation & Help
    // ============================================================
    /// Run system diagnostics
    #[command(after_help = r#"Examples:
  # Full system diagnostics
  aosctl diag --full

  # System checks only
  aosctl diag --system

  # Tenant-specific checks
  aosctl diag --tenant dev

  # Create diagnostic bundle
  aosctl diag --full --bundle ./diag_bundle.zip
"#)]
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
    #[command(after_help = r#"Examples:
  # Explain specific error code
  aosctl explain E3001

  # Explain AosError variant
  aosctl explain InvalidHash

  # Get help for unknown error
  aosctl explain E9999
"#)]
    Explain {
        /// Error code (E3001) or AosError name (InvalidHash)
        code: String,
    },

    /// List all error codes
    #[command(after_help = r#"Examples:
  # List all error codes
  aosctl error-codes

  # List error codes in JSON format
  aosctl error-codes --json

  # Filter by category
  aosctl error-codes --category crypto
"#)]
    ErrorCodes {
        /// Output JSON format
        #[arg(long)]
        json: bool,
    },

    /// Interactive tutorial
    #[command(after_help = r#"Examples:
  # Run basic tutorial
  aosctl tutorial

  # Run advanced tutorial
  aosctl tutorial --advanced

  # Non-interactive mode for CI
  aosctl tutorial --ci
"#)]
    Tutorial {
        /// Run advanced tutorial
        #[arg(long)]
        advanced: bool,

        /// Non-interactive mode for CI
        #[arg(long)]
        ci: bool,
    },

    /// Display offline manual
    #[command(after_help = r#"Examples:
  # Display manpage
  aosctl manual --format man

  # Display offline markdown manual
  aosctl manual --format md

  # Search manual for specific terms
  aosctl manual --format md --search "error codes"
"#)]
    Manual {
        #[command(flatten)]
        args: commands::manual::ManualArgs,
    },

    /// Quantize Qwen FP16 weights to int4 and write manifest
    #[command(after_help = r#"Examples:
  # Quantize a directory of .safetensors into artifacts/qwen2_5_7b_int4
  aosctl quantize-qwen \
    --input ./models/qwen2.5-7b-fp16 \
    --output ./artifacts/qwen2_5_7b_int4 \
    --model-name qwen2.5-7b-instruct

  # Emit JSON manifest to stdout
  aosctl quantize-qwen --input ./fp16.safetensors --output ./artifacts --json
"#)]
    QuantizeQwen {
        /// Input path (.safetensors file or directory containing them)
        #[arg(long)]
        input: PathBuf,

        /// Output directory for .bin and manifest.json
        #[arg(long)]
        output: PathBuf,

        /// Model name for manifest metadata
        #[arg(long, default_value = "qwen2.5-7b-instruct")]
        model_name: String,

        /// Optional block size for stats (currently unused)
        #[arg(long)]
        group_size: Option<usize>,

        /// Output manifest JSON to stdout
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Train a LoRA adapter
    #[command(after_help = r#"Examples:
  # Train adapter with default settings
  aosctl train --data training_data.json --output ./trained_adapter

  # Train with custom configuration
  aosctl train --data training_data.json --output ./trained_adapter \
    --rank 8 --alpha 32.0 --learning-rate 0.001 --epochs 5

  # Train with Metal backend
  aosctl train --data training_data.json --output ./trained_adapter \
    --plan plan/qwen7b/plan.bin

  # Train with configuration file
  aosctl train --config training_config.json --data training_data.json \
    --output ./trained_adapter
"#)]
    Train {
        #[command(flatten)]
        args: train::TrainArgs,
    },

    /// Train base adapter from manifest
    #[command(after_help = r#"Examples:
  # Train base adapter with default settings
  aosctl train-base-adapter

  # Train with custom manifest and tokenizer
  aosctl train-base-adapter --manifest training/datasets/my_manifest.json \
    --tokenizer models/qwen2.5-7b-mlx/tokenizer.json

  # Train and output as .aos file
  aosctl train-base-adapter --output-format aos --adapter-id my_adapter
"#)]
    TrainBaseAdapter {
        #[command(flatten)]
        args: train_base_adapter::TrainBaseAdapterArgs,
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
    /// Initialize a code repository
    #[command(after_help = r#"Examples:
  # Initialize current directory
  aosctl code-init .

  # Initialize specific repository
  aosctl code-init /path/to/repo --tenant default
"#)]
    CodeInit {
        /// Repository path
        path: PathBuf,

        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },

    /// Update repository scan
    #[command(after_help = r#"Examples:
  # Scan repository at current commit
  aosctl code-update my-repo

  # Scan specific commit
  aosctl code-update my-repo --commit abc123
"#)]
    CodeUpdate {
        /// Repository ID
        repo_id: String,

        /// Commit SHA (defaults to HEAD)
        #[arg(long)]
        commit: Option<String>,

        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },

    /// List registered repositories
    #[command(after_help = r#"Examples:
  # List all repositories
  aosctl code-list

  # List with JSON output
  aosctl code-list --json
"#)]
    CodeList {
        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },

    /// Get repository status
    #[command(after_help = r#"Examples:
  # Get repository status
  aosctl code-status my-repo

  # Get status with JSON output
  aosctl code-status my-repo --json
"#)]
    CodeStatus {
        /// Repository ID
        repo_id: String,

        /// Tenant ID
        #[arg(long, default_value = "default")]
        tenant: String,
    },
}

#[derive(Subcommand)]
pub enum NodeSyncMode {
    /// Verify sync between two nodes
    Verify {
        /// Source node ID
        #[arg(long)]
        from: String,

        /// Target node ID
        #[arg(long)]
        to: String,
    },

    /// Push adapters to target node
    Push {
        /// Target node ID
        #[arg(long)]
        to: String,

        /// Adapter IDs to push (comma-separated)
        #[arg(long, value_delimiter = ',')]
        adapters: Vec<String>,
    },

    /// Pull adapters from source node
    Pull {
        /// Source node ID
        #[arg(long)]
        from: String,

        /// Adapter IDs to pull (comma-separated)
        #[arg(long, value_delimiter = ',')]
        adapters: Vec<String>,
    },

    /// Export adapters for air-gap transfer
    Export {
        /// Output file path
        #[arg(long)]
        file: PathBuf,
    },

    /// Import adapters from air-gap bundle
    Import {
        /// Input file path
        #[arg(long)]
        file: PathBuf,
    },
}

pub async fn run() -> Result<()> {
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

            // Display user-friendly error message
            display_user_friendly_error(&e, error_code.as_deref(), &event_id);

            Err(e)
        }
    }
}

async fn execute_command(command: &Commands, cli: &Cli, output: &OutputWriter) -> Result<()> {
    match command {
        // Tenant Management
        Commands::TenantInit { id, uid, gid } | Commands::Init { id, uid, gid } => {
            init_tenant::run(id, *uid, *gid, output).await?;
        }

        // Adapter Management
        Commands::AdapterList { tier } => {
            list_adapters::run(tier.as_deref(), output).await?;
        }
        Commands::AdapterRegister {
            id,
            hash,
            tier,
            rank,
        } => {
            register_adapter::run(id, hash, tier, *rank, output).await?;
        }
        Commands::AdapterPin {
            tenant,
            adapter,
            ttl_hours,
            reason,
        } => {
            let db = adapteros_db::Database::connect_env().await?;
            pin::pin_adapter(&db, tenant, adapter, *ttl_hours, reason, output).await?;
        }
        Commands::AdapterUnpin { tenant, adapter } => {
            let db = adapteros_db::Database::connect_env().await?;
            pin::unpin_adapter(&db, tenant, adapter, output).await?;
        }
        Commands::AdapterListPinned { tenant } => {
            let db = adapteros_db::Database::connect_env().await?;
            pin::list_pinned(&db, tenant, output).await?;
        }
        Commands::AdapterSwap {
            tenant,
            add,
            remove,
            timeout,
            commit,
            socket,
        } => {
            adapter_swap::run(tenant, add, remove, *timeout, *commit, socket).await?;
        }
        Commands::AdapterInfo { adapter_id } => {
            adapter_info::run(adapter_id).await?;
        }
        Commands::Adapter(cmd) => {
            adapter::handle_adapter_command(cmd.clone(), output).await?;
        }
        Commands::Aos(cmd) => {
            commands::aos::run(commands::aos::AosArgs { cmd: cmd.clone() }, output).await?;
        }
        Commands::Adapters(cmd) => {
            commands::adapters::run(
                commands::adapters::AdaptersArgs { cmd: cmd.clone() },
                output,
            )
            .await?;
        }

        // Node & Cluster Management
        Commands::NodeList { offline } => {
            node_list::run(*offline).await?;
        }
        Commands::NodeVerify { all, nodes } => {
            node_verify::run(*all, nodes.clone()).await?;
        }
        Commands::NodeSync { mode } => {
            use node_sync::SyncMode;
            let sync_mode = match mode {
                NodeSyncMode::Verify { from, to } => SyncMode::Verify {
                    from: from.clone(),
                    to: to.clone(),
                },
                NodeSyncMode::Push { to, adapters } => SyncMode::Push {
                    to: to.clone(),
                    adapters: adapters.clone(),
                },
                NodeSyncMode::Pull { from, adapters } => SyncMode::Pull {
                    from: from.clone(),
                    adapters: adapters.clone(),
                },
                NodeSyncMode::Export { file } => SyncMode::Export { file: file.clone() },
                NodeSyncMode::Import { file } => SyncMode::Import { file: file.clone() },
            };
            node_sync::run(sync_mode).await?;
        }

        // Registry Management
        Commands::RegistrySync {
            dir,
            cas_root,
            registry,
        } => {
            sync_registry::sync_registry(dir, cas_root, registry, output).await?;
        }

        // Plan Management
        Commands::PlanBuild {
            manifest,
            output: output_path,
            tenant_id,
        } => {
            build_plan::run(manifest, output_path, tenant_id.as_deref(), output).await?;
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
                name,
                weights,
                config,
                tokenizer,
                tokenizer_cfg,
                license,
                output,
            )
            .await?;
        }

        // Telemetry & Verification
        Commands::TelemetryVerify { bundle_dir } => {
            verify_telemetry::verify_telemetry_chain(bundle_dir, output).await?;
        }

        Commands::TraceValidate {
            path,
            strict,
            tolerant,
            verify_hash,
            max_events,
            max_bytes,
            max_line_len,
        } => {
            let effective_strict = if *tolerant { false } else { *strict };
            commands::trace_validate::run(
                path,
                effective_strict,
                *verify_hash,
                *max_events,
                *max_bytes,
                *max_line_len,
                output,
            )
            .await?;
        }

        Commands::FederationVerify {
            bundle_dir,
            database,
        } => {
            verify_federation::run(bundle_dir, database, output).await?;
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
        Commands::CallgraphExport {
            codegraph_db,
            output: output_path,
            format,
        } => {
            let format = format
                .parse::<export_callgraph::ExportFormat>()
                .map_err(|e| anyhow::anyhow!("Invalid format '{}': {}", format, e))?;
            export_callgraph::export_callgraph(&codegraph_db, &output_path, format, output).await?;
        }
        Commands::CodegraphStats { codegraph_db } => {
            codegraph_stats::run(codegraph_db.to_path_buf(), output).await?;
        }
        Commands::SecdStatus {
            pid_file,
            heartbeat_file,
            socket,
            database,
        } => {
            secd_status::run(pid_file, heartbeat_file, socket, Some(database)).await?;
        }
        Commands::SecdAudit {
            database,
            limit,
            operation,
        } => {
            secd_audit::run(database, *limit, operation.as_deref()).await?;
        }

        // General Operations
        Commands::Import { bundle, no_verify } => {
            import::run(bundle, !no_verify, output).await?;
        }
        Commands::Verify { bundle } => {
            verify::run(bundle, output).await?;
        }
        Commands::VerifyAdapter {
            adapters_root,
            adapter_id,
        } => {
            commands::verify_adapter::run(adapters_root.clone(), adapter_id.clone(), output)
                .await?;
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
            insecure_skip_egress_check,
            capture_events,
        } => {
            // Dev-only: allow bypassing PF preflight via hidden flag
            if *insecure_skip_egress_check {
                std::env::set_var("AOS_INSECURE_SKIP_EGRESS", "1");
            }
            serve::run(
                tenant,
                plan,
                socket,
                backend.clone(),
                *dry_run,
                capture_events.as_ref(),
                output,
            )
            .await?;
        }
        Commands::Audit { cpid, suite } => {
            audit::run(cpid, suite.as_deref(), output).await?;
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
            replay::run(bundle, verbose_mode, output).await?;
        }
        Commands::Rollback { tenant, cpid } => {
            rollback::run(tenant, cpid, output).await?;
        }
        Commands::Baseline(cmd) => {
            baseline::execute(cmd, output).await?;
        }
        Commands::Golden(cmd) => {
            golden::execute(cmd, output).await?;
        }
        Commands::Router(cmd) => {
            cmd.clone().run()?;
        }
        Commands::Report {
            bundle,
            output: output_path,
        } => {
            report::run(bundle, output_path, output).await?;
        }
        Commands::BootstrapAdmin {
            email,
            display_name,
        } => {
            commands::bootstrap_admin::run(email, display_name.as_deref(), output).await?;
        }
        Commands::Bootstrap {
            mode,
            air_gapped,
            json,
            checkpoint_file,
        } => {
            // Bootstrap doesn't use OutputWriter, runs standalone
            bootstrap::run(mode, *air_gapped, *json, checkpoint_file.clone()).await?;
        }

        // Utility
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            completions::generate_completions(*shell, &mut cmd)?;
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
            explain::explain(code).await?;
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

        Commands::QuantizeQwen {
            input,
            output: out_dir,
            model_name,
            group_size,
            json,
        } => {
            commands::quantize_qwen::run(input, out_dir, model_name, *group_size, *json, output)
                .await?;
        }

        Commands::Train { args } => {
            args.execute().await?;
        }

        Commands::TrainBaseAdapter { args } => {
            args.execute().await?;
        }

        // Code Intelligence Commands
        Commands::CodeInit { path, tenant } => {
            commands::code::code_init(path, tenant, output).await?;
        }
        Commands::CodeUpdate {
            repo_id,
            commit,
            tenant,
        } => {
            commands::code::code_update(repo_id, tenant, commit.as_deref(), output).await?;
        }
        Commands::CodeList { tenant } => {
            commands::code::code_list(tenant, output).await?;
        }
        Commands::CodeStatus { repo_id, tenant } => {
            commands::code::code_status(repo_id, tenant, output).await?;
        }
    }

    Ok(())
}

/// Get command name from Commands enum
fn get_command_name(command: &Commands) -> String {
    match command {
        Commands::TenantInit { .. } | Commands::Init { .. } => "init-tenant",
        Commands::AdapterList { .. } => "list-adapters",
        Commands::AdapterRegister { .. } => "register-adapter",
        Commands::AdapterPin { .. } => "pin-adapter",
        Commands::AdapterUnpin { .. } => "unpin-adapter",
        Commands::AdapterListPinned { .. } => "list-pinned",
        Commands::AdapterSwap { .. } => "adapter-swap",
        Commands::AdapterInfo { .. } => "adapter-info",
        Commands::Adapter(_) => "adapter",
        Commands::Adapters(_) => "adapters",
        Commands::NodeList { .. } => "node-list",
        Commands::NodeVerify { .. } => "node-verify",
        Commands::NodeSync { .. } => "node-sync",
        Commands::PlanBuild { .. } => "build-plan",
        Commands::ModelImport { .. } => "import-model",
        Commands::TelemetryVerify { .. } => "verify-telemetry",
        Commands::TraceValidate { .. } => "trace-validate",
        Commands::FederationVerify { .. } => "federation-verify",
        Commands::DriftCheck { .. } => "drift-check",
        Commands::CallgraphExport { .. } => "callgraph-export",
        Commands::CodegraphStats { .. } => "codegraph-stats",
        Commands::SecdStatus { .. } => "secd-status",
        Commands::SecdAudit { .. } => "secd-audit",
        Commands::Import { .. } => "import",
        Commands::Verify { .. } => "verify",
        Commands::Policy(_) => "policy",
        Commands::Serve { .. } => "serve",
        Commands::Audit { .. } => "audit",
        Commands::AuditDeterminism { .. } => "audit-determinism",
        Commands::Replay { .. } => "replay",
        Commands::Rollback { .. } => "rollback",
        Commands::Baseline(_) => "baseline",
        Commands::Golden(_) => "golden",
        Commands::Router(_) => "router",
        Commands::RegistrySync { .. } => "registry-sync",
        Commands::Report { .. } => "report",
        Commands::BootstrapAdmin { .. } => "bootstrap-admin",
        Commands::Bootstrap { .. } => "bootstrap",
        Commands::Completions { .. } => "completions",
        Commands::Diag { .. } => "diag",
        Commands::Explain { .. } => "explain",
        Commands::ErrorCodes { .. } => "error-codes",
        Commands::Tutorial { .. } => "tutorial",
        Commands::Manual { .. } => "manual",
        Commands::Train { .. } => "train",
        Commands::TrainBaseAdapter { .. } => "train-base-adapter",
        Commands::CodeInit { .. } => "code-init",
        Commands::CodeUpdate { .. } => "code-update",
        Commands::CodeList { .. } => "code-list",
        Commands::CodeStatus { .. } => "code-status",
        Commands::VerifyAdapter { .. } => "verify-adapter",
        Commands::QuantizeQwen { .. } => "quantize-qwen",
        Commands::Aos(_) => "aos",
        Commands::Infer { .. } => "infer",
    }
    .to_string()
}

/// Extract tenant ID from command if present
fn extract_tenant_from_command(command: &Commands) -> Option<String> {
    match command {
        Commands::AdapterPin { tenant, .. }
        | Commands::AdapterUnpin { tenant, .. }
        | Commands::AdapterListPinned { tenant, .. }
        | Commands::AdapterSwap { tenant, .. }
        | Commands::Serve { tenant, .. }
        | Commands::Rollback { tenant, .. } => Some(tenant.clone()),
        Commands::Diag { tenant, .. } => tenant.clone(),
        _ => None,
    }
}

/// Display user-friendly error message with CLI-specific formatting
fn display_user_friendly_error(error: &anyhow::Error, error_code: Option<&str>, event_id: &str) {
    use crate::error_codes::find_by_code;

    // First, try to get user-friendly message from error code registry
    if let Some(code) = error_code {
        if let Some(error_info) = find_by_code(code) {
            error!(error_info = %error_info, "CLI error");
            return;
        }
    }

    // Fallback: Try to extract user-friendly message from AosError if present
    let error_msg = format!("{}", error);

    // Try to map common error patterns to user-friendly messages
    let user_friendly_msg = if error_msg.contains("Connection refused") {
        "Database connection failed. Please check if the database is running and accessible."
            .to_string()
    } else if error_msg.contains("Permission denied") {
        "Permission denied. Please check file permissions and user access rights.".to_string()
    } else if error_msg.contains("No such file") {
        "File not found. Please verify the file path and ensure the file exists.".to_string()
    } else if error_msg.contains("timeout") {
        "Operation timed out. This may be due to system load or network issues.".to_string()
    } else if let Some(code) = error_code {
        format!(
            "An error occurred ({}). See: aosctl explain {} (event: {})",
            code, code, event_id
        )
    } else {
        format!("An unexpected error occurred. Event ID: {}", event_id)
    };

    error!(user_msg = %user_friendly_msg, "CLI user error");

    // Show the original error in verbose mode or for debugging
    if std::env::var("AOS_DEBUG").is_ok() || std::env::var("RUST_BACKTRACE").is_ok() {
        error!(technical_details = %error_msg, "CLI technical details");
    }
}

// Logging initialization moved to logging module
