//! Main binary for the AdapterOS Service Supervisor

use adapteros_service_supervisor::{ServiceSupervisor, SupervisorConfig, SupervisorServer};
use std::sync::Arc;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "adapteros_service_supervisor=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting AdapterOS Service Supervisor");

    // Load configuration
    let config = match SupervisorConfig::load() {
        Ok(config) => {
            info!(
                "Loaded configuration with {} services",
                config.services.len()
            );
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // For development, generate a keypair. In production, this should be loaded from secure storage
    let keypair_pem = std::env::var("SUPERVISOR_KEYPAIR_PEM").unwrap_or_else(|_| {
        warn!("No SUPERVISOR_KEYPAIR_PEM provided, generating temporary keypair for development");
        // In a real deployment, this should be loaded from a secure key store
        "".to_string()
    });

    // Create supervisor
    let supervisor = match ServiceSupervisor::new(config.clone(), &keypair_pem).await {
        Ok(supervisor) => {
            info!("Service supervisor initialized successfully");
            Arc::new(supervisor)
        }
        Err(e) => {
            error!("Failed to initialize supervisor: {}", e);
            std::process::exit(1);
        }
    };

    // Create and start server
    let server = SupervisorServer::new(supervisor, &config.server);

    // Handle graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for shutdown signal");
        info!("Shutdown signal received");
    };

    tokio::select! {
        result = server.serve() => {
            if let Err(e) = result {
                error!("Server error: {}", e);
                std::process::exit(1);
            }
        }
        _ = shutdown_signal => {
            info!("Shutting down gracefully...");
            // Supervisor will be dropped and cleaned up automatically
        }
    }

    info!("AdapterOS Service Supervisor stopped");
    Ok(())
}
