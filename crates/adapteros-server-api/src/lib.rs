pub mod auth;
pub mod cab_workflow;
pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod model_runtime;
pub mod operation_tracker;
pub mod rate_limit;
pub mod retry;
pub mod routes;
pub mod signing;
pub mod state;
pub mod types;
pub mod uds_client;
pub mod validation;

// Selective imports from adapteros_api_types to avoid conflicts with local types
pub use adapteros_api_types::{
    // Metrics types
    AdapterHealthResponse,
    AdapterMetricsResponse,
    // Domain adapter types
    CreateDomainAdapterRequest,
    DomainAdapterExecutionResponse,
    DomainAdapterManifestResponse,
    DomainAdapterResponse,
    LoadDomainAdapterRequest,
    QualityMetricsResponse,
    // Training types (only those not defined locally)
    StartTrainingRequest,
    TestDomainAdapterRequest,
    TestDomainAdapterResponse,
};

// Direct imports (not re-exported to avoid conflicts)
pub use state::{AppState, CryptoState};
pub use types::*;
pub use uds_client::{UdsClient, UdsClientError};
// pub use services::auth::{require_role, require_any_role};
// pub use services::error_handling::{db_error_to_response, validation_error};
