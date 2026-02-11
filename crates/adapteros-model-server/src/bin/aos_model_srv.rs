//! Model Server Binary
//!
//! Dedicated server process for shared model loading.
//!
//! ## Usage
//!
//! ```bash
//! aos-model-srv --model-path /var/models/Llama-3.2-3B-Instruct-4bit \
//!               --socket-path var/run/aos-model-srv.sock
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use adapteros_model_server::{ModelServer, ModelServerConfig};

/// Model Server for shared model loading
#[derive(Parser, Debug)]
#[command(name = "aos-model-srv")]
#[command(about = "Dedicated model server for shared model loading across workers")]
#[command(version)]
struct Args {
    /// Path to the model directory
    #[arg(long, env = "AOS_MODEL_PATH")]
    model_path: PathBuf,

    /// Path to the Unix domain socket
    #[arg(long, default_value = "var/run/aos-model-srv.sock")]
    socket_path: PathBuf,

    /// Model ID for identification
    #[arg(long)]
    model_id: Option<String>,

    /// Maximum KV cache size in MB (default: 4096)
    #[arg(long, default_value = "4096")]
    kv_cache_mb: u64,

    /// Maximum number of concurrent sessions
    #[arg(long, default_value = "32")]
    max_sessions: usize,

    /// Hot adapter activation threshold (0.0-1.0)
    #[arg(long, default_value = "0.10")]
    hot_threshold: f64,

    /// Maximum number of hot adapters to cache
    #[arg(long, default_value = "8")]
    max_hot_adapters: usize,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .with(EnvFilter::from_default_env().add_directive(log_level.into()))
        .init();

    info!(
        build_id = adapteros_core::version::BUILD_ID,
        git_commit = adapteros_core::version::GIT_COMMIT_HASH,
        version = adapteros_core::version::VERSION,
        profile = adapteros_core::version::BUILD_PROFILE,
        model_path = %args.model_path.display(),
        socket_path = %args.socket_path.display(),
        kv_cache_mb = args.kv_cache_mb,
        max_sessions = args.max_sessions,
        hot_threshold = args.hot_threshold,
        max_hot_adapters = args.max_hot_adapters,
        "aos-model-srv starting"
    );

    // Validate model path
    if !args.model_path.exists() {
        error!(
            model_path = %args.model_path.display(),
            "Model path does not exist"
        );
        return Err(format!("Model path does not exist: {}", args.model_path.display()).into());
    }

    // Create configuration
    let config = ModelServerConfig {
        enabled: true,
        socket_path: args.socket_path,
        model_path: Some(args.model_path.clone()),
        model_id: args.model_id,
        kv_cache_max_bytes: args.kv_cache_mb * 1024 * 1024,
        max_sessions: args.max_sessions,
        hot_adapter_threshold: args.hot_threshold,
        max_hot_adapters: args.max_hot_adapters,
        warmup_adapters: Vec::new(),
        drain_grace_secs: 30,
        health_check_interval_secs: 10,
    };

    // Validate configuration
    config.validate().map_err(|e| {
        error!(error = %e, "Invalid configuration");
        e
    })?;

    // Create server
    let server = Arc::new(ModelServer::new(config));

    // Load model
    info!(
        model_path = %args.model_path.display(),
        "Loading model..."
    );

    if let Err(e) = server.load_model(&args.model_path).await {
        error!(
            error = %e,
            model_path = %args.model_path.display(),
            "Failed to load model"
        );
        return Err(format!("Failed to load model: {}", e).into());
    }

    info!("Model loaded successfully");

    // Set up signal handling for graceful shutdown
    let server_clone = server.clone();
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            error!(error = %e, "Failed to listen for ctrl-c");
            return;
        }

        info!("Received shutdown signal, starting drain...");
        server_clone.start_drain();

        // Give in-flight requests time to complete
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        info!("Shutdown complete");
        std::process::exit(0);
    });

    // Start serving
    server.serve().await?;

    Ok(())
}
