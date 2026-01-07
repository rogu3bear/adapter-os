//! Boot infrastructure for AdapterOS.
//!
//! This crate provides reusable boot lifecycle management without depending on
//! Axum or other HTTP framework types. It can be used by any binary that needs
//! structured boot sequences.
//!
//! ## Key Components
//!
//! - [`phase`]: Boot phase definitions and transition rules
//! - [`lifecycle_builder`]: Builder pattern for phase-gated boot sequences
//! - [`boot_report`]: JSON boot report generation for logging and file output
//! - [`worker_auth`]: Ed25519-based worker authentication tokens
//! - [`services`]: Service registry for boot-time service management
//! - [`error`]: Boot-specific error types
//!
//! ## Design Principles
//!
//! 1. **NO Axum dependencies**: This crate must not depend on Axum, tower, or hyper
//! 2. **Phase-gated execution**: Boot phases progress monotonically
//! 3. **Observable**: Phase timings and boot reports enable debugging
//! 4. **Secure**: Worker auth uses Ed25519 with replay defense
//!
//! ## Example
//!
//! ```rust,no_run
//! use adapteros_boot::lifecycle_builder::{LifecycleBuilder, LifecycleConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = LifecycleConfig::default();
//!     let artifacts = LifecycleBuilder::new(config)
//!         .start().await?
//!         .db_connecting().await?
//!         .migrating().await?
//!         .seeding().await?
//!         .loading_policies().await?
//!         .init_crypto().await?
//!         .starting_backend().await?
//!         .loading_base_models().await?
//!         .loading_adapters().await?
//!         .worker_discovery().await?
//!         .ready().await?
//!         .build().await?;
//!
//!     println!("Boot complete in {:?}", artifacts.boot_report.as_ref().map(|r| r.total_boot_time_ms));
//!     Ok(())
//! }
//! ```
//!
//! ## Worker Authentication
//!
//! Worker auth tokens are Ed25519-signed JWTs with short TTL (30-60s) and replay defense:
//!
//! ```rust,no_run
//! use adapteros_boot::worker_auth::{generate_worker_token, validate_worker_token};
//! use ed25519_dalek::SigningKey;
//! use lru::LruCache;
//! use std::num::NonZeroUsize;
//!
//! // Control plane generates token
//! let signing_key = SigningKey::generate(&mut rand::thread_rng());
//! let token = generate_worker_token(&signing_key, "worker-1", "req-123", 45).unwrap();
//!
//! // Worker validates token
//! let verifying_key = signing_key.verifying_key();
//! let mut jti_cache = LruCache::new(NonZeroUsize::new(1000).unwrap());
//! let claims = validate_worker_token(&token, &verifying_key, Some("worker-1"), &mut jti_cache).unwrap();
//! ```

pub mod boot_report;
pub mod error;
pub mod invariant_metrics;
pub mod jti_cache;
pub mod key_ring;
pub mod key_update;
pub mod lifecycle_builder;
pub mod phase;
pub mod runtime_dir;
pub mod services;
pub mod worker_auth;

// Re-export commonly used types at crate root
pub use boot_report::{BootReport, BootReportBuilder, BuildInfo};
pub use error::{BootError, BootResult, WorkerAuthError, WorkerAuthResult};
pub use invariant_metrics::{
    boot_invariant_metrics, record_invariant_check, record_invariant_skipped,
    record_invariant_violation, BootInvariantMetrics,
};
pub use jti_cache::{JtiCacheStore, JtiEntry, DEFAULT_JTI_CACHE_SIZE, JTI_CACHE_SIZE_ENV};
pub use key_ring::{
    extract_kid_from_token, RotationMeta, RotationReceipt, WorkerKeyRing,
    DEFAULT_ROTATION_GRACE_PERIOD_SECS,
};
pub use key_update::{
    KeyUpdateRequest, KeyUpdateResponse, KEY_UPDATE_MAX_AGE_SECS, KEY_UPDATE_PROTOCOL_VERSION,
};
pub use lifecycle_builder::{BootArtifacts, LifecycleBuilder, LifecycleConfig};
pub use phase::{BootPhase, PhaseTiming, PhaseTransitions};
pub use runtime_dir::{ensure_runtime_dir, RuntimeDir, EXIT_CONFIG_ERROR};
pub use services::ServiceRegistry;
pub use worker_auth::{
    derive_kid_from_verifying_key, generate_worker_token, load_or_generate_worker_keypair,
    load_or_generate_worker_keypair_with_options, load_worker_public_key,
    load_worker_public_key_with_retry, load_worker_verifying_key, validate_worker_token,
    KeypairOptions, WorkerTokenClaims, CLOCK_SKEW_TOLERANCE_SECS,
};
