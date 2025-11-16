//! Secure Enclave Daemon for AdapterOS
//!
//! This daemon provides a privileged interface to macOS Secure Enclave operations.
//! It runs as a separate process with restricted entitlements and exposes a minimal
//! UDS API for signing and encryption operations.
//!
//! ## Security Model
//!
//! - Runs under dedicated service account
//! - Only has keychain + enclave entitlements
//! - Workers cannot directly access enclave
//! - All keys stored in Secure Enclave
//! - No network access (UDS only)

pub mod audit;
pub mod enclave;
pub mod heartbeat;
pub mod host_identity;
pub mod key_lifecycle;
pub mod pidfile;
pub mod protocol;
pub mod server;

pub use audit::AuditLogger;
pub use enclave::EnclaveManager;
pub use heartbeat::Heartbeat;
pub use host_identity::{
    AttestationMetadata, AttestationReport, HostIdentity, HostIdentityManager,
    SecureEnclaveConnection,
};
pub use key_lifecycle::{KeyAgeWarning, KeyLifecycleManager};
pub use pidfile::{is_process_running, read_pid, remove_pid, write_pid};
pub use protocol::{Request, Response};
pub use server::serve_uds;
