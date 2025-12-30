//! Shared Preflight Validation Module
//!
//! This module provides unified preflight validation logic that is shared
//! between CLI and Server API. It ensures consistent enforcement of
//! adapter readiness checks before activation or swap operations.
//!
//! # Overview
//!
//! Preflight checks validate that an adapter is ready for production use:
//!
//! 1. **Maintenance Mode**: System must not be in maintenance mode
//! 2. **AOS File Path**: Adapter must have a configured .aos file path
//! 3. **AOS File Hash**: File hash must be set for integrity verification
//! 4. **Content Hash**: `content_hash_b3` must be present (required for determinism)
//! 5. **Manifest Hash**: `manifest_hash` must be present (required for routing)
//! 6. **Lifecycle State**: Must be in a state that allows activation (ready/active)
//! 7. **File Exists**: The .aos file must exist on disk
//! 8. **Training Evidence**: Training snapshot evidence must exist
//! 9. **Uniqueness**: No conflicting active adapters for same repo/branch
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_core::preflight::{run_preflight, PreflightConfig};
//!
//! // Run preflight with default (strict) configuration
//! let config = PreflightConfig::for_tenant("my-tenant");
//! let result = run_preflight(&adapter, &db, &config).await;
//!
//! if !result.passed {
//!     for failure in &result.failures {
//!         eprintln!("[{}] {}", failure.code.as_str(), failure.message);
//!     }
//! }
//! ```
//!
//! # Error Codes
//!
//! All preflight failures include structured error codes for programmatic handling:
//!
//! - `PREFLIGHT_MISSING_CONTENT_HASH`: content_hash_b3 is NULL or empty
//! - `PREFLIGHT_MISSING_MANIFEST_HASH`: manifest_hash is NULL or empty
//! - `PREFLIGHT_MAINTENANCE_MODE`: System is in maintenance mode
//! - `PREFLIGHT_CONFLICTING_ADAPTERS`: Another adapter active for same repo/branch
//!
//! See [`PreflightErrorCode`] for the complete list.
//!
//! # Bypass Configuration
//!
//! For emergency scenarios, preflight checks can be bypassed using [`PreflightConfig`]:
//!
//! ```ignore
//! let config = PreflightConfig::new()
//!     .skip_maintenance("Emergency deployment during maintenance window");
//!
//! // Bypass is recorded for audit
//! let result = run_preflight(&adapter, &db, &config).await;
//! assert!(!result.bypasses_used.is_empty());
//! ```
//!
//! Bypass usage is tracked in the result for audit logging.
//!
//! # Integration
//!
//! This module is designed to be integrated into both CLI and API paths:
//!
//! - **CLI**: Implement `PreflightDbOps` for your database type
//! - **API**: Implement `PreflightDbOps` for your app state
//!
//! See [`traits`] module for the required trait implementations.

pub mod checks;
pub mod config;
pub mod error;
pub mod result;
pub mod traits;

// Re-export main types
pub use checks::{is_maintenance_mode, run_preflight};
pub use config::PreflightConfig;
pub use error::{PreflightCheckFailure, PreflightErrorCode};
pub use result::{BypassEvent, CheckStatus, PreflightAuditEvent, PreflightCheck, PreflightResult};
pub use traits::{ActiveUniquenessResult, PreflightAdapterData, PreflightDbOps, SimpleAdapterData};

#[cfg(test)]
pub use traits::mock;
