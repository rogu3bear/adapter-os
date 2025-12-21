//! Secure Enclave Daemon Binary
//!
//! Run with:
//!   aos-secd --socket /var/run/aos-secd.sock

use adapteros_core::{derive_seed, AosError, B3Hash};
use adapteros_deterministic_exec::{init_global_executor, spawn_deterministic, ExecutorConfig};
use adapteros_secd::{
    remove_pid, serve_uds, write_pid, AuditLogger, Heartbeat, KeyLifecycleManager,
};
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;

#[derive(Parser, Debug)]
#[clap(name = "aos-secd", about = "AdapterOS Secure Enclave Daemon")]
struct Args {
    /// Unix domain socket path
    #[clap(long, default_value = "/var/run/aos-secd.sock")]
    socket: PathBuf,

    /// PID file path
    #[clap(long, default_value = "/var/run/aos-secd.pid")]
    pid_file: PathBuf,

    /// Heartbeat file path
    #[clap(long, default_value = "/var/run/aos-secd.heartbeat")]
    heartbeat_file: PathBuf,

    /// Database path for audit trail
    #[clap(long, default_value = "./var/aos-cp.sqlite3")]
    database: PathBuf,

    /// Skip database connection (for testing)
    #[clap(long)]
    no_db: bool,

    /// Key age warning threshold in days
    #[clap(long, default_value = "90")]
    key_age_threshold: i64,

    /// Manifest path for deterministic seed derivation
    #[clap(long)]
    manifest: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), AosError> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = Args::parse();

    // Initialize deterministic executor with HKDF-derived seed
    let global_seed = if let Some(manifest_path) = &args.manifest {
        // Derive seed from manifest hash using HKDF per determinism patterns
        let manifest_content = std::fs::read(manifest_path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read manifest at {}: {}",
                manifest_path.display(),
                e
            ))
        })?;
        let manifest_hash = B3Hash::hash(&manifest_content);
        derive_seed(&manifest_hash, "secd-executor")
    } else {
        // Fallback: derive from daemon identity when no manifest provided
        tracing::warn!(
            component = "secd",
            "No manifest provided, using daemon identity for seed derivation"
        );
        let identity_hash = B3Hash::hash(b"aos-secd-daemon-v1");
        derive_seed(&identity_hash, "secd-executor")
    };

    let config = ExecutorConfig {
        global_seed,
        enable_event_logging: true,
        max_ticks_per_task: 10000,
        ..Default::default()
    };
    init_global_executor(config)
        .map_err(|e| AosError::Internal(format!("Failed to initialize executor: {}", e)))?;
    tracing::info!(
        component = "secd",
        seed_prefix = %hex::encode(&global_seed[..8]),
        "Deterministic executor initialized with HKDF-derived seed"
    );

    tracing::info!(
        component = "secd",
        "AdapterOS Secure Enclave Daemon starting"
    );
    tracing::info!(
        component = "secd",
        socket_path = %args.socket.display(),
        "Socket configured"
    );
    tracing::info!(
        component = "secd",
        pid_file = %args.pid_file.display(),
        "PID file configured"
    );
    tracing::info!(
        component = "secd",
        heartbeat_file = %args.heartbeat_file.display(),
        "Heartbeat file configured"
    );

    // Write PID file
    if let Err(e) = write_pid(&args.pid_file) {
        tracing::error!(
            component = "secd",
            pid_file = %args.pid_file.display(),
            error = %e,
            "Failed to write PID file"
        );
        return Err(e.into());
    }

    // Connect to database (optional)
    let db = if args.no_db {
        tracing::warn!(component = "secd", "Database connection disabled");
        None
    } else {
        let db_path = args
            .database
            .to_str()
            .ok_or_else(|| AosError::Config("Invalid database path".to_string()))?;
        match adapteros_db::Db::connect(db_path).await {
            Ok(db) => {
                tracing::info!(
                    component = "secd",
                    db_path = %db_path,
                    "Connected to database"
                );

                // Run migrations
                if let Err(e) = db.migrate().await {
                    tracing::error!(
                        component = "secd",
                        db_path = %db_path,
                        error = %e,
                        "Database migration failed"
                    );
                    tracing::warn!(
                        component = "secd",
                        db_path = %db_path,
                        "Continuing without database audit trail"
                    );
                    None
                } else {
                    tracing::info!(
                        component = "secd",
                        db_path = %db_path,
                        "Database migrations complete"
                    );
                    Some(db)
                }
            }
            Err(e) => {
                tracing::error!(
                    component = "secd",
                    db_path = %db_path,
                    error = %e,
                    "Failed to connect to database"
                );
                tracing::warn!(
                    component = "secd",
                    db_path = %db_path,
                    "Continuing without database audit trail"
                );
                None
            }
        }
    };

    // Create audit logger
    let audit_logger = AuditLogger::new(db.clone());

    // Create and start heartbeat
    let heartbeat = Arc::new(Heartbeat::new(&args.heartbeat_file)?);
    heartbeat.update()?;
    tracing::info!(
        component = "secd",
        heartbeat_file = %args.heartbeat_file.display(),
        "Heartbeat initialized"
    );

    // Spawn heartbeat updater task
    let heartbeat_task = {
        let heartbeat = heartbeat.clone();
        spawn_deterministic("Heartbeat updater".to_string(), async move {
            heartbeat.spawn_updater(Duration::from_secs(10)).await;
        })
        .map_err(|e| AosError::Internal(format!("Failed to spawn heartbeat updater: {}", e)))?
    };

    // Create key lifecycle manager
    let key_lifecycle = Arc::new(KeyLifecycleManager::new(db.clone(), args.key_age_threshold));

    // Track default keys on startup
    key_lifecycle
        .track_key("aos_bundle_signing", "signing")
        .await;
    key_lifecycle
        .track_key("aos_lora_encryption", "encryption")
        .await;

    // Spawn key age checker task (check daily)
    let key_lifecycle_task = {
        let key_lifecycle = key_lifecycle.clone();
        spawn_deterministic("Key lifecycle manager".to_string(), async move {
            key_lifecycle
                .spawn_age_checker(Duration::from_secs(86400))
                .await;
        })
        .map_err(|e| AosError::Internal(format!("Failed to spawn key lifecycle manager: {}", e)))?
    };

    // Setup graceful shutdown
    let pid_file = args.pid_file.clone();
    let heartbeat_for_cleanup = heartbeat.clone();

    let _signal_handle = spawn_deterministic("Signal handler".to_string(), async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                tracing::info!(component = "secd", "Received shutdown signal");

                // Clean up PID file
                if let Err(e) = remove_pid(&pid_file) {
                    tracing::error!(
                        component = "secd",
                        pid_file = %pid_file.display(),
                        error = %e,
                        "Failed to remove PID file"
                    );
                }

                // Clean up heartbeat file
                if let Err(e) = heartbeat_for_cleanup.remove() {
                    tracing::error!(
                        component = "secd",
                        error = %e,
                        "Failed to remove heartbeat file"
                    );
                }

                std::process::exit(0);
            }
            Err(e) => {
                tracing::error!(
                    component = "secd",
                    error = %e,
                    "Error setting up signal handler"
                );
            }
        }
    });

    tracing::info!(component = "secd", "Enclave daemon ready");

    // Start serving (this blocks until error or shutdown)
    if let Err(e) = serve_uds(&args.socket, audit_logger).await {
        tracing::error!(
            component = "secd",
            error = %e,
            "Server error"
        );

        // Clean up
        let _ = remove_pid(&args.pid_file);
        let _ = heartbeat.remove();

        return Err(e.into());
    }

    // Wait for background tasks (shouldn't reach here normally)
    heartbeat_task.abort();
    key_lifecycle_task.abort();

    Ok(())
}
