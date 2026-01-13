pub mod cli;
pub mod helpers;
pub mod manifest;
pub mod backend;
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub mod coreml;
pub mod registration;
pub mod init;

// Re-export public items
pub use cli::{Args, error_to_exit_code, EXIT_SUCCESS, EXIT_CONFIG_ERROR, EXIT_TRANSIENT_ERROR, EXIT_FATAL_ERROR, is_prod_runtime};
pub use init::run_worker;
pub use helpers::{setup_panic_hook, shutdown_worker_telemetry, WORKER_IDENTITY, WORKER_TELEMETRY, WorkerIdentity, detect_capabilities, build_capabilities_detail, mock_capabilities_detail, setup_mock_base_model_cache, dev_no_auth_enabled};
pub use registration::{register_with_cp_with_retry, RegistrationParams, RegistrationResult, notify_cp_status};
pub use manifest::{parse_manifest, fetch_manifest_from_cp, cache_manifest, LoadedManifest};
pub use backend::{validate_backend_feature, parse_backend_choice, is_mock_backend};

#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub use coreml::*;
