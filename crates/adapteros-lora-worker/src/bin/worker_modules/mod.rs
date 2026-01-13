pub mod backend;
pub mod cli;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub mod coreml;
pub mod helpers;
pub mod init;
pub mod manifest;
pub mod registration;

// Re-export public items
pub use backend::{is_mock_backend, parse_backend_choice, validate_backend_feature};
pub use cli::{
    error_to_exit_code, is_prod_runtime, Args, EXIT_CONFIG_ERROR, EXIT_FATAL_ERROR, EXIT_SUCCESS,
    EXIT_TRANSIENT_ERROR,
};
pub use helpers::{
    build_capabilities_detail, detect_capabilities, dev_no_auth_enabled, mock_capabilities_detail,
    setup_mock_base_model_cache, setup_panic_hook, shutdown_worker_telemetry, WorkerIdentity,
    WORKER_IDENTITY, WORKER_TELEMETRY,
};
pub use init::run_worker;
pub use manifest::{cache_manifest, fetch_manifest_from_cp, parse_manifest, LoadedManifest};
pub use registration::{
    notify_cp_status, register_with_cp_with_retry, RegistrationParams, RegistrationResult,
};

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub use coreml::*;
