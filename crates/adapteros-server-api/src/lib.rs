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
pub mod services {
    pub mod auth;
    pub mod replay;
    pub mod error_handling;
    pub mod db_pool;
    pub mod alert_deduplication;
    pub mod retry;
}
pub mod signing;
pub mod state;
pub mod types;
pub mod uds_client;
pub mod validation;

// Selective imports from adapteros_api_types to avoid conflicts with local types
pub use adapteros_api_types::{
    // Domain adapter types
    CreateDomainAdapterRequest, DomainAdapterResponse, DomainAdapterExecutionResponse,
    DomainAdapterManifestResponse, LoadDomainAdapterRequest, TestDomainAdapterRequest,
    TestDomainAdapterResponse,
    // Metrics types
    AdapterHealthResponse, QualityMetricsResponse, AdapterMetricsResponse,
    // Training types (only those not defined locally)
    StartTrainingRequest,
};

// Direct imports (not re-exported to avoid conflicts)
pub use state::{AppState, CryptoState};
pub use types::*;
pub use uds_client::{UdsClient, UdsClientError};
pub use services::auth::{require_role, require_any_role};
pub use services::error_handling::{db_error_to_response, validation_error};
