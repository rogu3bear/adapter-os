use clap::Parser;
use std::path::PathBuf;

pub fn normalize_jwt_mode(value: &str) -> String {
    match value.to_lowercase().as_str() {
        "hmac" | "hs256" => "hmac".to_string(),
        "eddsa" | "ed25519" => "eddsa".to_string(),
        other => other.to_string(),
    }
}

#[derive(Parser)]
#[command(name = "aos-cp")]
#[command(about = "adapterOS Control Plane", long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "configs/cp.toml")]
    pub config: String,

    /// Run migrations only and exit
    #[arg(long)]
    pub migrate_only: bool,

    /// Generate OpenAPI spec and exit
    #[arg(long)]
    pub generate_openapi: bool,

    /// Enable single-writer mode (prevents concurrent control plane instances)
    #[arg(long, default_value_t = true)]
    pub single_writer: bool,

    /// Path to PID file for single-writer lock
    #[arg(long)]
    pub pid_file: Option<PathBuf>,

    /// Skip PF/firewall egress checks (DEBUG BUILDS ONLY)
    /// This flag is not available in release builds for security
    #[cfg_attr(debug_assertions, arg(long))]
    #[cfg_attr(not(debug_assertions), arg(skip))]
    pub skip_pf_check: bool,

    /// Skip environment drift detection (DEBUG BUILDS ONLY)
    /// This flag is not available in release builds for security
    #[cfg_attr(debug_assertions, arg(long))]
    #[cfg_attr(not(debug_assertions), arg(skip))]
    pub skip_drift_check: bool,

    /// Path to base model manifest for executor seeding
    /// Can also be set via AOS_MANIFEST_PATH environment variable
    #[arg(long, env = "AOS_MANIFEST_PATH")]
    pub manifest_path: Option<PathBuf>,

    /// Enable strict mode (fail-closed boot)
    /// When enabled:
    /// - Worker keypair must exist (var/keys/worker_signing.key)
    /// - Boot report emission is required
    /// - Legacy auth paths are disabled
    #[arg(long, env = "AOS_STRICT")]
    pub strict: bool,
}
