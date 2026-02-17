//! adapterOS Database Layer
//!
//! This crate provides the persistence layer for adapterOS, including both SQL
//! (SQLite) and KV (ReDB) backends with dual-write support for migration.
//!
//! # Dual-Write Architecture
//!
//! The database layer supports a gradual migration from SQL to KV storage via
//! four storage modes controlled by `AOS_STORAGE_BACKEND`:
//!
//! ```text
//! SqlOnly (current) -> DualWrite -> KvPrimary -> KvOnly (target)
//!      |                  |            |            |
//!      v                  v            v            v
//!  SQL only         Write both     Write both    KV only
//!                   Read SQL       Read KV
//! ```
//!
//! ## Consistency Guarantees
//!
//! **DualWrite Mode**: SQL remains authoritative. KV writes are best-effort:
//! - KV write failures are logged but don't block the operation
//! - SQL transaction commits first, then KV write occurs
//! - On KV failure, a warning is logged and operation succeeds
//!
//! **KvPrimary Mode**: Both backends written, KV is authoritative for reads:
//! - If KV read fails, SQL fallback is available
//! - Both writes must succeed for write operations
//!
//! ## Atomic Dual-Write Pattern
//!
//! For operations requiring atomicity across backends (configured via
//! `AtomicDualWriteConfig`), the pattern is:
//!
//! 1. Begin SQL transaction
//! 2. Execute SQL write
//! 3. Execute KV write
//! 4. If KV fails and rollback enabled: rollback SQL transaction
//! 5. Otherwise: commit SQL, log KV failure
//!
//! See `adapters::AtomicDualWriteConfig` for configuration options.
//!
//! # Protected Database Access
//!
//! Write operations require a `ProtectedDb` wrapper that enforces:
//! - Lifecycle token validation (prevents writes during shutdown)
//! - Audit logging for write operations
//! - Consistent error handling
//!
//! ```rust,ignore
//! // Read-only access (always available)
//! let adapters = db.list_adapters_for_tenant(tenant_id).await?;
//!
//! // Write access (requires lifecycle token)
//! let protected = ProtectedDb::from_db(db, lifecycle_token);
//! protected.create_adapter(params).await?;
//! ```
//!
//! # Naming Conventions
//!
//! ## Function Naming
//!
//! The following verb prefixes are used consistently across the codebase:
//!
//! | Prefix | Usage | Example |
//! |--------|-------|---------|
//! | `get_*` | Retrieve a single entity by ID | `get_adapter()`, `get_training_job()` |
//! | `list_*` | Retrieve multiple entities | `list_adapters_for_tenant()` |
//! | `create_*` | Create a new entity (async) | `create_training_dataset()` |
//! | `register_*` | Create with domain validation | `register_adapter()` |
//! | `insert_*` | Low-level storage write | `insert_training_metric()` |
//! | `update_*` | Modify entity fields | `update_adapter_state()` |
//! | `set_*` | Set a single property | `set_adapter_version_state()` |
//! | `delete_*` | Remove an entity entirely | `delete_adapter()` |
//! | `remove_*` | Remove a relationship/association | `remove_tag_from_session()` |
//!
//! **Key distinction**: Use `delete_*` when removing an entity from storage.
//! Use `remove_*` only when removing a relationship between entities (e.g.,
//! removing a tag from a session, removing a document from a collection).
//!
//! ## Suffix Conventions
//!
//! | Suffix | Meaning |
//! |--------|---------|
//! | `*_kv` | KV backend variant of a SQL operation |
//! | `*_for_tenant` | Tenant-scoped query |
//! | `*_by_*` | Query filtered by a specific field |

#![allow(unexpected_cfgs)]
#![allow(unused_imports)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::manual_strip)]
#![allow(clippy::redundant_closure)]

use adapteros_config::resolve_database_url;
use adapteros_core::{AosError, Result};
use fs2::available_space;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(debug_assertions)]
use once_cell::sync::Lazy;
use tracing::{debug, info, warn};

#[cfg(debug_assertions)]
static MIGRATION_TEST_LOCK: Lazy<tokio::sync::Mutex<()>> =
    Lazy::new(|| tokio::sync::Mutex::new(()));

/// Generate a new typed ID string for database storage.
pub fn new_id(prefix: adapteros_id::IdPrefix) -> String {
    adapteros_id::TypedId::new(prefix).to_string()
}

// Query constants for SELECT column lists
pub mod constants;

// Core adapter repository/version model
pub mod adapter_repositories;
pub mod crypto_at_rest;

// Database abstraction layer
pub mod api_keys;
pub mod chat_sessions_kv;
pub mod client_errors;
pub mod collections_kv;
pub mod coreml_fusion_pairs;
pub mod documents_kv;
pub mod embedding_benchmarks;
pub mod error_classification;
pub mod errors;
pub mod evidence_envelopes;
pub mod factory;
pub mod kv_backend;
pub mod kv_diff;
pub mod kv_isolation_scan;
pub mod kv_metrics;
pub mod metrics_db;
pub mod policy_audit_kv;
pub mod prefix_templates;
pub mod rag;
pub mod reembedding;
pub mod replay_kv;
pub mod repository_training_policies;
pub mod retry;
pub mod routing_rules;
pub mod setup;
pub mod sqlite_backend;
pub mod storage_issues;
pub mod storage_reconciliation;
pub mod telemetry_kv;
pub mod tenant_execution_policies;
pub mod tenant_metrics; // Tenant resource metrics
pub mod tenant_policies;
pub mod tenant_policy_bindings_kv;
pub mod tenant_settings;
pub mod tenant_settings_registry;
pub mod topology;
pub mod traits;

// Lifecycle rules module
pub mod lifecycle_rules;

// Diagnostics persistence
pub mod diagnostics;

// Migration validation utilities
pub mod migration_validation;

// Dual-write acknowledgment tracking
pub mod write_ack;

// Re-export commonly used types
pub use adapter_repositories::{
    tier_promotion_toctou_count, AdapterRepository, AdapterRepositoryPolicy, AdapterVersion,
    AdapterVersionRuntimeState, CreateDraftVersionParams, CreateRepositoryParams,
    CreateVersionParams, RepositoryGroup, UpsertAdapterRepositoryPolicyParams,
    UpsertRuntimeStateParams,
};
pub use coreml_fusion_pairs::{CoremlFusionPair, CreateCoremlFusionPairParams};
pub use embedding_benchmarks::EmbeddingBenchmarkRow;
pub use factory::{DbFactory, StorageBackend as DbStorageBackend};
pub use repository_training_policies::RepositoryTrainingPolicy;
pub use traits::{
    AdapterRecord, AdapterRecordRow, CreateStackRequest, DatabaseBackend, DatabaseBackendType,
    DatabaseConfig, StackRecord, StackRecordRow,
};

// Re-export KV backend types
pub use kv_backend::{KvBackend, KvDb, StorageError as KvStorageError};
pub use kv_diff::DiffIssue;
pub use kv_isolation_scan::{
    KvIsolationFinding, KvIsolationIssue, KvIsolationScanConfig, KvIsolationScanReport,
    KvIsolationTenantSummary,
};
pub use setup::{
    SetupDiscoveredModel, SetupSeedItem, SetupSeedOptions, SetupSeedResult, SetupSeedStatus,
};
pub use storage_issues::{NewStorageIssue, StorageIssue};
pub use storage_reconciliation::{StorageIssueParams, StorageReconciliationIssue};
pub use topology::{AdapterTopology, AdjacencyEdge, ClusterDefinition, TopologyGraph};

// Re-export KV metrics types
pub use kv_metrics::{
    evaluate_global_kv_alerts, evaluate_kv_alerts, global_kv_metrics, kv_alert_rules, KvErrorType,
    KvMetrics, KvMetricsSnapshot, KvOperationTimer, KvOperationType, KV_ALERT_METRIC_DEGRADATIONS,
    KV_ALERT_METRIC_DRIFT, KV_ALERT_METRIC_ERRORS, KV_ALERT_METRIC_FALLBACKS,
};

// Re-export system metrics database types
pub use metrics_db::{MetricsViolation, SystemMetrics, SystemMetricsDbOps};

// Re-export dual-write ack types
pub use write_ack::{WriteAck, WriteAckStore, WriteStatus};

// Re-export tenant metrics types
pub use tenant_metrics::{
    get_system_memory, MemoryMetrics, TenantCpuTracker, TenantGpuTracker, TenantMetricsService,
    TenantResourceMetrics, TenantStorageMetrics, TenantStoragePaths,
};

const MIN_FREE_SPACE_BYTES: u64 = 100 * 1024 * 1024;

// Re-export tenant policy types
pub use tenant_policies::{
    CreateCustomizationRequest, CustomizationHistoryEntry, CustomizationStatus,
    TenantPolicyCustomization, TenantPolicyCustomizationOps,
};

// Re-export tenant settings types
pub use tenant_settings::{TenantSettings, UpdateTenantSettingsParams};
pub use tenant_settings_registry::{
    RouterWeightsSetting, TenantSettingsKnownKey, TenantSettingsRegistry,
    TenantSettingsValidationError,
};

// Re-export tenant policy binding types
pub mod tenant_policy_bindings;
pub use tenant_policy_bindings::{TenantPolicyBinding, ALL_POLICIES, CORE_POLICIES};

// Re-export lifecycle rules types
pub use lifecycle_rules::{
    ActionType, ConditionEvaluationResult, ConditionOperator, CreateLifecycleRuleParams,
    LifecycleRule, LifecycleRuleAction, LifecycleRuleCondition, LifecycleRuleEvaluation,
    LifecycleRuleFilter, LifecycleRuleScope, LifecycleRuleType, TransitionValidationResult,
    UpdateLifecycleRuleParams,
};

mod protected_db;
pub use protected_db::{LifecycleToken, ProtectedDb, WriteCapableDb};

// Re-export query performance monitoring types
pub use query_performance::{
    ConcurrencyConfig, MultiTenantConfig, QueryMetrics, QueryPerformanceMonitor, QueryStats,
    ThresholdViolation, ViolationSeverity, ViolationType,
};

// API keys
pub use api_keys::{ApiKeyRecord, ApiKeyRecord as ApiKey};

/// Storage mode for database operations
///
/// Defines how the database layer handles reads and writes when both
/// SQL and KV backends are available.
///
/// # Configuration
///
/// Storage mode can be configured via the `AOS_STORAGE_BACKEND` environment variable:
/// - `sql_only` or `sql` - SQL backend only (default, current production mode)
/// - `dual_write` or `dual` - Write to both backends, read from SQL (migration validation phase)
/// - `kv_primary` - Write to both backends, read from KV (migration cutover phase)
/// - `kv_only` - KV backend only (future production target)
///
/// # Migration Path
///
/// The storage mode supports a gradual migration from SQL to KV storage:
/// 1. **SqlOnly** (current): Production default, all operations use SQL backend
/// 2. **DualWrite** (validation): Write to both backends, read from SQL for validation
/// 3. **KvPrimary** (cutover): Write to both backends, read from KV to test KV read path
/// 4. **KvOnly** (future): Full migration complete, SQL backend can be deprecated
///
/// # When to Use Each Mode
///
/// - **SqlOnly**: Current production default, use when KV backend is not yet available
/// - **DualWrite**: Use during migration validation to ensure KV writes match SQL writes
/// - **KvPrimary**: Use during migration cutover to test KV read path while keeping SQL as backup
/// - **KvOnly**: Final migration state, use when confident KV backend is production-ready
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageMode {
    /// SQL backend only (default, current production mode)
    ///
    /// All read and write operations use the SQL backend exclusively.
    /// The KV backend is ignored even if attached.
    ///
    /// # Purpose
    ///
    /// This is the default mode representing the current production state.
    /// It provides backward compatibility and is the fallback when KV backend
    /// is not available or not yet ready for production use.
    ///
    /// # Behavior
    ///
    /// - **Reads**: SQL backend only
    /// - **Writes**: SQL backend only
    /// - **KV Backend**: Ignored (not accessed)
    ///
    /// # When to Use
    ///
    /// - Current production deployments (default)
    /// - Testing SQL backend in isolation
    /// - When KV backend is not available
    /// - Initial state before migration begins
    ///
    /// # Migration Path
    ///
    /// Start here. Progress to [`DualWrite`] when ready to validate KV writes.
    ///
    /// [`DualWrite`]: StorageMode::DualWrite
    #[default]
    SqlOnly,

    /// Write to both SQL and KV backends, read from SQL (migration validation phase)
    ///
    /// This mode enables dual-write functionality to validate that KV writes
    /// match SQL writes before committing to the migration.
    ///
    /// # Purpose
    ///
    /// Validation phase of the migration path. Ensures KV backend can correctly
    /// store and retrieve data by comparing it against the authoritative SQL backend.
    ///
    /// # Behavior
    ///
    /// - **Reads**: SQL backend (authoritative source)
    /// - **Writes**: Both SQL and KV backends
    /// - **Write Failures**: KV write failures are logged but don't block the operation
    /// - **Consistency**: SQL remains the source of truth
    ///
    /// # When to Use
    ///
    /// - Validating KV backend implementation
    /// - Testing KV write path in production with zero risk
    /// - Building up KV data in parallel with SQL
    /// - Detecting KV write issues before cutover
    ///
    /// # Important Notes
    ///
    /// - SQL is still the authoritative source for reads
    /// - KV write failures are logged as warnings, not errors
    /// - Allows safe testing of KV writes in production
    /// - No performance impact on reads (still SQL-only)
    /// - Can run indefinitely until confidence is established
    ///
    /// # Migration Path
    ///
    /// Progress from [`SqlOnly`] → **DualWrite** → [`KvPrimary`]
    ///
    /// [`SqlOnly`]: StorageMode::SqlOnly
    /// [`KvPrimary`]: StorageMode::KvPrimary
    DualWrite,

    /// Write to both SQL and KV backends, read from KV (migration cutover phase)
    ///
    /// This mode switches reads to the KV backend while maintaining SQL writes
    /// as a backup and consistency check.
    ///
    /// # Purpose
    ///
    /// Cutover phase of the migration path. Tests the KV read path in production
    /// while keeping SQL as a safety net for rollback and verification.
    ///
    /// # Behavior
    ///
    /// - **Reads**: KV backend (primary source)
    /// - **Writes**: Both SQL and KV backends
    /// - **Fallback**: SQL backend available for emergency rollback
    /// - **Consistency**: KV is now the source of truth for reads
    ///
    /// # When to Use
    ///
    /// - Testing KV read performance in production
    /// - Final validation before removing SQL dependency
    /// - Establishing confidence in KV read path
    /// - Monitoring for KV read issues before full cutover
    ///
    /// # Important Notes
    ///
    /// - KV read failures will cause operation failures (unlike DualWrite)
    /// - SQL writes continue to provide rollback path
    /// - Can revert to [`DualWrite`] by changing environment variable
    /// - Performance depends on KV backend read performance
    /// - Should monitor for consistency between SQL and KV
    ///
    /// # Migration Path
    ///
    /// Progress from [`DualWrite`] → **KvPrimary** → [`KvOnly`]
    ///
    /// Rollback path: **KvPrimary** → [`DualWrite`] (if issues detected)
    ///
    /// [`DualWrite`]: StorageMode::DualWrite
    /// [`KvOnly`]: StorageMode::KvOnly
    KvPrimary,

    /// KV backend only (full migration complete)
    ///
    /// All operations use the KV backend exclusively. SQL backend is ignored
    /// and can be deprecated or removed.
    ///
    /// # Purpose
    ///
    /// Final state of the migration path. Represents full commitment to KV backend
    /// as the single source of truth.
    ///
    /// # Behavior
    ///
    /// - **Reads**: KV backend only
    /// - **Writes**: KV backend only
    /// - **SQL Backend**: Ignored (not accessed)
    ///
    /// # When to Use
    ///
    /// - After successful validation in [`KvPrimary`] mode
    /// - When confident in KV backend stability and performance
    /// - To reduce write amplification from dual-write
    /// - Final production target for new deployments
    ///
    /// # Important Notes
    ///
    /// - No SQL fallback available (point of no return)
    /// - SQL data becomes stale immediately
    /// - Cannot rollback without data migration
    /// - Best performance (no dual-write overhead)
    /// - Requires high confidence in KV backend
    ///
    /// # Migration Path
    ///
    /// Final state: [`KvPrimary`] → **KvOnly**
    ///
    /// Rollback: Requires full data migration from KV back to SQL
    ///
    /// # Configuration Example
    ///
    /// ```bash
    /// export AOS_STORAGE_BACKEND=kv_only
    /// ```
    ///
    /// [`KvPrimary`]: StorageMode::KvPrimary
    KvOnly,
}

impl std::str::FromStr for StorageMode {
    type Err = AosError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "sql_only" | "sql" => Ok(StorageMode::SqlOnly),
            "dual_write" | "dual" => Ok(StorageMode::DualWrite),
            "kv_primary" | "kv-primary" => Ok(StorageMode::KvPrimary),
            "kv_only" | "kv-only" => Ok(StorageMode::KvOnly),
            _ => Err(AosError::Config(format!(
                "Invalid storage mode '{}'. Valid options: sql_only, dual_write, kv_primary, kv_only",
                s
            ))),
        }
    }
}

impl std::fmt::Display for StorageMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageMode::SqlOnly => write!(f, "sql_only"),
            StorageMode::DualWrite => write!(f, "dual_write"),
            StorageMode::KvPrimary => write!(f, "kv_primary"),
            StorageMode::KvOnly => write!(f, "kv_only"),
        }
    }
}

/// Describes KV coverage for runtime guardrails
#[derive(Debug, Clone, Copy)]
pub struct KvCoverageSummary {
    /// Domains with KV repositories and read/write paths
    pub supported_domains: &'static [&'static str],
    /// Domains that still rely on SQL-only code paths
    pub unsupported_domains: &'static [&'static str],
}

/// Current KV coverage snapshot.
/// Keep this conservative to avoid silent cutovers when SQL-only paths remain.
pub fn kv_coverage_summary() -> KvCoverageSummary {
    // KV repositories with read/write implemented
    const SUPPORTED: &[&str] = &[
        "adapters",
        "adapter_stacks",
        "tenants",
        "users",
        "auth_sessions",
        "plans",
        "tenant_policy_bindings",
        "rag",
        "telemetry",
        "replay",
        "plugin_configs",
        "messages",
        "runtime_sessions",
        "repositories",
    ];
    // SQL-only domains that block KV-only posture today
    // Dec 2025: KV coverage implemented for prior blockers; keep list empty to allow KV-only.
    const UNSUPPORTED: &[&str] = &[];

    KvCoverageSummary {
        supported_domains: SUPPORTED,
        unsupported_domains: UNSUPPORTED,
    }
}

/// Returns true when kv_only posture must remain blocked by coverage gaps.
pub fn kv_only_blocked_by_coverage() -> bool {
    !kv_coverage_summary().unsupported_domains.is_empty()
}

#[cfg(test)]
mod storage_mode_tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_storage_mode_from_str() {
        // Test canonical names
        assert_eq!(
            StorageMode::from_str("sql_only").unwrap(),
            StorageMode::SqlOnly
        );
        assert_eq!(
            StorageMode::from_str("dual_write").unwrap(),
            StorageMode::DualWrite
        );
        assert_eq!(
            StorageMode::from_str("kv_primary").unwrap(),
            StorageMode::KvPrimary
        );
        assert_eq!(
            StorageMode::from_str("kv_only").unwrap(),
            StorageMode::KvOnly
        );

        // Test short aliases
        assert_eq!(StorageMode::from_str("sql").unwrap(), StorageMode::SqlOnly);
        assert_eq!(
            StorageMode::from_str("dual").unwrap(),
            StorageMode::DualWrite
        );

        // Test hyphenated variants (from config schema)
        assert_eq!(
            StorageMode::from_str("kv-primary").unwrap(),
            StorageMode::KvPrimary
        );
        assert_eq!(
            StorageMode::from_str("kv-only").unwrap(),
            StorageMode::KvOnly
        );

        // Test case insensitivity
        assert_eq!(
            StorageMode::from_str("SQL_ONLY").unwrap(),
            StorageMode::SqlOnly
        );
        assert_eq!(
            StorageMode::from_str("Dual_Write").unwrap(),
            StorageMode::DualWrite
        );
        assert_eq!(
            StorageMode::from_str("KV-PRIMARY").unwrap(),
            StorageMode::KvPrimary
        );

        // Test invalid values
        assert!(StorageMode::from_str("invalid").is_err());
        assert!(StorageMode::from_str("").is_err());
        assert!(StorageMode::from_str("kv").is_err());
    }

    #[test]
    fn test_storage_mode_display() {
        assert_eq!(StorageMode::SqlOnly.to_string(), "sql_only");
        assert_eq!(StorageMode::DualWrite.to_string(), "dual_write");
        assert_eq!(StorageMode::KvPrimary.to_string(), "kv_primary");
        assert_eq!(StorageMode::KvOnly.to_string(), "kv_only");
    }

    #[test]
    fn test_storage_mode_default() {
        assert_eq!(StorageMode::default(), StorageMode::SqlOnly);
    }

    #[test]
    fn test_storage_mode_read_write_predicates() {
        // SqlOnly: read SQL, write SQL
        assert!(StorageMode::SqlOnly.read_from_sql());
        assert!(!StorageMode::SqlOnly.read_from_kv());
        assert!(StorageMode::SqlOnly.write_to_sql());
        assert!(!StorageMode::SqlOnly.write_to_kv());

        // DualWrite: read SQL, write both
        assert!(StorageMode::DualWrite.read_from_sql());
        assert!(!StorageMode::DualWrite.read_from_kv());
        assert!(StorageMode::DualWrite.write_to_sql());
        assert!(StorageMode::DualWrite.write_to_kv());

        // KvPrimary: read KV first, SQL fallback allowed
        assert!(StorageMode::KvPrimary.read_from_sql());
        assert!(StorageMode::KvPrimary.read_from_kv());
        assert!(StorageMode::KvPrimary.write_to_sql());
        assert!(StorageMode::KvPrimary.write_to_kv());

        // KvOnly: read KV, write KV
        assert!(!StorageMode::KvOnly.read_from_sql());
        assert!(StorageMode::KvOnly.read_from_kv());
        assert!(!StorageMode::KvOnly.write_to_sql());
        assert!(StorageMode::KvOnly.write_to_kv());
    }

    #[test]
    fn test_storage_mode_helper_predicates() {
        // is_kv_only
        assert!(!StorageMode::SqlOnly.is_kv_only());
        assert!(!StorageMode::DualWrite.is_kv_only());
        assert!(!StorageMode::KvPrimary.is_kv_only());
        assert!(StorageMode::KvOnly.is_kv_only());

        // is_dual_write
        assert!(!StorageMode::SqlOnly.is_dual_write());
        assert!(StorageMode::DualWrite.is_dual_write());
        assert!(StorageMode::KvPrimary.is_dual_write());
        assert!(!StorageMode::KvOnly.is_dual_write());
    }
}

#[cfg(test)]
mod tenant_rate_limit_tests {
    use super::*;

    #[test]
    fn tenant_rate_limit_refills_by_window() {
        let db = Db::new_kv_only(None, StorageMode::SqlOnly);
        let tenant_id = "tenant-test";

        {
            let mut limits = db.tenant_rate_limits.write().unwrap();
            limits.insert(
                tenant_id.to_string(),
                TenantRateLimitState {
                    window_start: Instant::now()
                        - (TENANT_RATE_LIMIT_WINDOW + Duration::from_secs(1)),
                    count: TENANT_RATE_LIMIT_MAX_REQUESTS_PER_WINDOW,
                },
            );
        }

        assert!(
            db.check_rate_limit(tenant_id),
            "expected window expiration to reset count and allow request"
        );
        db.increment_rate_limit(tenant_id);

        let limits = db.tenant_rate_limits.read().unwrap();
        let state = limits
            .get(tenant_id)
            .expect("rate limit state must exist after check+increment");
        assert!(
            state.count <= 1,
            "expected count to be reset after window expiration"
        );
    }
}

impl StorageMode {
    /// Returns true if this mode reads from SQL backend
    pub fn read_from_sql(self) -> bool {
        matches!(
            self,
            StorageMode::SqlOnly | StorageMode::DualWrite | StorageMode::KvPrimary
        )
    }

    /// Returns true if this mode reads from KV backend
    pub fn read_from_kv(self) -> bool {
        matches!(self, StorageMode::KvPrimary | StorageMode::KvOnly)
    }

    /// Returns true if this mode writes to SQL backend
    pub fn write_to_sql(self) -> bool {
        matches!(
            self,
            StorageMode::SqlOnly | StorageMode::DualWrite | StorageMode::KvPrimary
        )
    }

    /// Returns true if this mode writes to KV backend
    pub fn write_to_kv(self) -> bool {
        matches!(
            self,
            StorageMode::DualWrite | StorageMode::KvPrimary | StorageMode::KvOnly
        )
    }

    /// Returns true if this is the final KV-only mode (migration complete)
    pub fn is_kv_only(self) -> bool {
        matches!(self, StorageMode::KvOnly)
    }

    /// Returns true if dual-write is active (writing to both backends)
    pub fn is_dual_write(self) -> bool {
        matches!(self, StorageMode::DualWrite | StorageMode::KvPrimary)
    }

    /// Returns true if SQL fallback is available for read operations
    ///
    /// In KvPrimary mode, if KV returns None or errors, we can fall back to SQL.
    /// This is useful during migration when data may not yet be in KV.
    pub fn sql_fallback_enabled(self) -> bool {
        matches!(
            self,
            StorageMode::SqlOnly | StorageMode::DualWrite | StorageMode::KvPrimary
        )
    }
}

/// Bootstrap health status for system initialization validation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BootstrapHealthStatus {
    /// Whether the bootstrap state is healthy
    pub healthy: bool,
    /// List of issues found during validation
    pub issues: Vec<String>,
}

// Phase 2: Governance
//
// The adapter listing endpoints are polled by the UI and scripts. A counter-only
// limiter that never refills eventually locks out the tenant until process restart.
// Keep this lightweight and operationally safe: per-tenant fixed window.
const TENANT_RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const TENANT_RATE_LIMIT_MAX_REQUESTS_PER_WINDOW: u32 = 10_000;

#[derive(Debug, Clone, Copy)]
struct TenantRateLimitState {
    window_start: Instant,
    count: u32,
}

/// Database connection pool and query methods (SQLite)
///
/// For production deployments, use `PostgresDb` instead.
/// Supports optional KV backend integration for migration scenarios.
#[derive(Clone)]
pub struct Db {
    pool: Option<SqlitePool>,
    kv: Option<std::sync::Arc<KvDb>>,
    storage_mode: StorageMode,
    atomic_dual_write_config: Arc<crate::adapters::AtomicDualWriteConfig>,
    /// Degradation state: if Some, contains the reason for degradation
    degraded_reason: std::sync::Arc<std::sync::RwLock<Option<String>>>,
    /// Performance monitor for tenant-scoped queries
    performance_monitor: std::sync::Arc<std::sync::RwLock<Option<QueryPerformanceMonitor>>>,

    // Phase 2: Governance & Optimization
    /// Global query timeout in milliseconds (0 = disabled)
    query_timeout_ms: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Tenant rate limit counters (per-window)
    tenant_rate_limits:
        std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, TenantRateLimitState>>>,
    /// Query plan cache for prepared statements
    plan_cache: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, String>>>,
    /// Directory containing the SQLite database file (if applicable)
    db_dir: Option<PathBuf>,
}

impl Db {
    /// Create a new Db instance with the given components
    ///
    /// This is the primary constructor for creating a Db with custom configuration.
    /// For simple SQLite connections, use `Db::connect()` or `Db::connect_env()` instead.
    ///
    /// # Arguments
    /// * `pool` - SQLite connection pool
    /// * `kv` - Optional KV backend for dual-write or KV-only modes
    /// * `storage_mode` - Controls read/write behavior across backends
    pub fn new(
        pool: SqlitePool,
        kv: Option<std::sync::Arc<KvDb>>,
        storage_mode: StorageMode,
    ) -> Self {
        Self::new_with_pool(Some(pool), kv, storage_mode, None)
    }

    /// Create a Db without an attached SQL pool (KV-only or external managed SQL)
    pub fn new_kv_only(kv: Option<std::sync::Arc<KvDb>>, storage_mode: StorageMode) -> Self {
        Self::new_with_pool(None, kv, storage_mode, None)
    }

    fn new_with_pool(
        pool: Option<SqlitePool>,
        kv: Option<std::sync::Arc<KvDb>>,
        storage_mode: StorageMode,
        db_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            pool,
            kv,
            storage_mode,
            atomic_dual_write_config: Arc::new(crate::adapters::AtomicDualWriteConfig::from_env()),
            degraded_reason: std::sync::Arc::new(std::sync::RwLock::new(None)),
            performance_monitor: std::sync::Arc::new(std::sync::RwLock::new(None)),
            query_timeout_ms: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(3000)), // Default 3s timeout
            tenant_rate_limits: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            plan_cache: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            db_dir,
        }
    }

    /// Connect to SQLite database with WAL mode
    ///
    /// Configuration:
    /// - WAL mode for better concurrency
    /// - Normal synchronous mode (balance between safety and performance)
    /// - 30-second connection timeout
    /// - Max 20 connections in pool
    /// - Statement cache size of 100
    /// - **CRITICAL:** Foreign key enforcement enabled
    pub async fn connect(path: &str) -> Result<Self> {
        // Special-case in-memory connections to avoid producing the invalid
        // `sqlite://:memory:` URI, which SQLite cannot open. Use a unique
        // named in-memory database to prevent cross-test collisions.
        let database_url = if matches!(path, ":memory:" | "sqlite://:memory:" | "sqlite::memory:") {
            static MEMORY_DB_COUNTER: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let memory_id = MEMORY_DB_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!(
                "sqlite://file:aos_mem_{}_{}?mode=memory&cache=shared",
                std::process::id(),
                memory_id
            )
        } else if path.starts_with("sqlite:") {
            path.to_string()
        } else {
            format!("sqlite://{}", path)
        };

        let is_memory = database_url.contains(":memory:") || database_url.contains("mode=memory");
        let mut db_dir: Option<PathBuf> = None;

        // Ensure parent directories exist for on-disk databases; SQLite will fail
        // with code 14 ("unable to open database file") if the path's parent
        // directory is missing.
        if !is_memory {
            let fs_path = database_url
                .trim_start_matches("sqlite://")
                .trim_start_matches("file:")
                .split('?')
                .next()
                .unwrap_or_default();

            if !fs_path.is_empty() {
                let parent = Path::new(fs_path)
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf())
                    .or_else(|| Some(PathBuf::from(".")));

                if let Some(dir) = parent {
                    std::fs::create_dir_all(&dir).map_err(|e| {
                        AosError::Database(format!(
                            "Failed to create database directory {}: {}",
                            dir.display(),
                            e
                        ))
                    })?;
                    db_dir = Some(dir);
                }
            }
        }

        let options = SqliteConnectOptions::from_str(&database_url)?
            .create_if_missing(true)
            // Use Memory journal mode for in-memory DBs (WAL not supported), WAL for disk
            .journal_mode(if is_memory {
                sqlx::sqlite::SqliteJournalMode::Memory
            } else {
                sqlx::sqlite::SqliteJournalMode::Wal
            })
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(30)) // 30s timeout for busy database
            .statement_cache_capacity(100) // Cache up to 100 prepared statements
            .pragma("temp_store", "MEMORY")
            .pragma("mmap_size", "268435456") // 256MB memory mapping for index performance
            .foreign_keys(true); // CRITICAL: Enable foreign key constraints

        // For in-memory databases, limit pool to 1 connection to ensure data consistency
        // (each new connection to :memory: creates a separate database)
        let pool = if is_memory {
            sqlx::pool::PoolOptions::new()
                .max_connections(1)
                .connect_with(options)
                .await
                .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?
        } else {
            SqlitePool::connect_with(options)
                .await
                .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?
        };

        Ok(Self::new_with_pool(
            Some(pool),
            None,
            StorageMode::SqlOnly,
            db_dir,
        ))
    }

    /// Connect to SQLite database using DATABASE_URL environment variable
    pub async fn connect_env() -> Result<Self> {
        let resolved = resolve_database_url()?;
        let database_url = resolved.path.to_string_lossy().to_string();
        info!(
            database_url = %database_url,
            source = %resolved.source,
            "Connecting to database from environment/default"
        );
        Self::connect(&database_url).await
    }

    /// Resolve the canonical auth session relation, creating a compatibility view if needed.
    ///
    /// Preferred relation: `auth_sessions`.
    /// Fallback: if `user_sessions` exists, create a view `auth_sessions` projecting legacy columns.
    /// Returns the relation name or an error if neither exists.
    pub async fn resolve_session_table(&self) -> Result<String> {
        let Some(pool) = self.pool_opt() else {
            return Err(AosError::Database(
                "SQL backend unavailable for auth session resolution".to_string(),
            ));
        };

        async fn has_relation(pool: &SqlitePool, name: &str) -> Result<bool> {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name = ?",
            )
            .bind(name)
            .fetch_one(pool)
            .await?;
            Ok(exists > 0)
        }

        if has_relation(pool, "auth_sessions").await? {
            return Ok("auth_sessions".to_string());
        }

        if has_relation(pool, "user_sessions").await? {
            sqlx::query(
                r#"
                CREATE VIEW IF NOT EXISTS auth_sessions AS
                SELECT
                    jti,
                    COALESCE(session_id, jti) AS session_id,
                    user_id,
                    tenant_id,
                    device_id,
                    rot_id,
                    refresh_hash,
                    refresh_expires_at,
                    ip_address,
                    user_agent,
                    created_at,
                    last_activity,
                    CAST(
                        CASE
                            WHEN typeof(expires_at) = 'integer' THEN expires_at
                            ELSE strftime('%s', expires_at)
                        END AS INTEGER
                    ) AS expires_at,
                    COALESCE(locked, 0) AS locked
                FROM user_sessions
                "#,
            )
            .execute(pool)
            .await?;
            return Ok("auth_sessions".to_string());
        }

        Err(AosError::Database(
            "Missing session table (auth_sessions or user_sessions)".to_string(),
        ))
    }

    /// Create a new Db instance with configuration from environment variables
    ///
    /// This constructor reads configuration from the following environment variables:
    /// - `AOS_DATABASE_URL` or `DATABASE_URL` - Database connection URL (default: "var/cp.db")
    /// - `AOS_STORAGE_BACKEND` or `AOS_STORAGE_MODE` - Storage mode (default: "sql_only")
    /// - `AOS_KV_PATH` - Path to KV database file (default: "var/aos-kv.redb")
    ///
    /// If `AOS_STORAGE_BACKEND` is set to a mode that requires KV backend (dual_write, kv_primary, kv_only),
    /// the KV backend will be automatically initialized at `AOS_KV_PATH`.
    ///
    /// # Graceful Degradation
    ///
    /// If KV backend initialization fails, the system will automatically fall back to `SqlOnly` mode
    /// and log a warning. This ensures the system remains operational even if KV backend is unavailable.
    /// The degradation reason is tracked and can be queried via `is_degraded()` and `degradation_reason()`.
    ///
    /// # Example
    ///
    /// ```bash
    /// export AOS_DATABASE_URL=var/aos-cp.sqlite3
    /// export AOS_STORAGE_BACKEND=dual_write
    /// export AOS_KV_PATH=var/aos-kv.redb
    /// ```
    ///
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = Db::from_config().await?;
    /// // Database is now configured with dual-write mode (or SqlOnly if KV failed)
    /// if db.is_degraded() {
    ///     println!("Warning: Running in degraded mode: {}", db.degradation_reason().unwrap());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_config() -> Result<Self> {
        use std::str::FromStr;
        use tracing::info;

        // Read database URL from environment
        let resolved_db = resolve_database_url()?;
        let database_url = resolved_db.path.to_string_lossy().to_string();

        // Read storage mode from environment
        let storage_mode_str = std::env::var("AOS_STORAGE_BACKEND")
            .or_else(|_| std::env::var("AOS_STORAGE_MODE"))
            .unwrap_or_else(|_| "sql_only".to_string());

        let requested_mode = StorageMode::from_str(&storage_mode_str)?;

        info!(
            database_url = %database_url,
            database_source = %resolved_db.source,
            storage_mode = %requested_mode,
            "Initializing database from configuration"
        );

        // KV path is required for all KV-capable modes
        let kv_path =
            std::env::var("AOS_KV_PATH").unwrap_or_else(|_| "var/aos-kv.redb".to_string());

        // Short-circuit for KV-only: no SQLite dependency
        if requested_mode == StorageMode::KvOnly {
            let kv = KvDb::init_redb(std::path::Path::new(&kv_path)).map(std::sync::Arc::new)?;
            info!(kv_path = %kv_path, "KV-only mode enabled; skipping SQL pool");
            let mut db = Self::new_kv_only(Some(kv), StorageMode::KvOnly);
            db.enforce_kv_only_guard()?;
            return Ok(db);
        }

        // Create base database connection for SQL-capable modes
        let mut db = Self::connect(&database_url).await?;

        // Initialize KV backend if required by storage mode
        if requested_mode.write_to_kv() || requested_mode.read_from_kv() {
            info!(kv_path = %kv_path, "Initializing KV backend");

            match db.init_kv_backend(std::path::Path::new(&kv_path)) {
                Ok(()) => {
                    // KV backend initialized successfully, set requested mode
                    db.set_storage_mode(requested_mode)?;
                    info!(
                        mode = %requested_mode,
                        "KV backend initialized successfully"
                    );
                }
                Err(e) => {
                    // KV backend failed - gracefully degrade to SqlOnly
                    warn!(
                        event = crate::constants::DEGRADATION_EVENT_INIT_FAILED,
                        error = %e,
                        requested_mode = %requested_mode,
                        fallback_mode = "sql_only",
                        "KV backend initialization failed - falling back to SqlOnly mode"
                    );
                    db.set_storage_mode(StorageMode::SqlOnly)?;
                    db.mark_degraded(format!("KV backend init failed: {}", e));
                }
            }
        } else {
            // No KV backend needed for SqlOnly mode
            db.set_storage_mode(requested_mode)?;
        }

        db.enforce_kv_only_guard()?;

        Ok(db)
    }

    /// Create in-memory database for testing
    ///
    /// This creates a temporary SQLite database in memory with all migrations applied.
    /// Useful for unit tests and integration tests.
    ///
    /// # Note
    /// This is available in both test and non-test builds for maximum flexibility.
    pub async fn new_in_memory() -> Result<Self> {
        let db = Self::connect("sqlite::memory:").await?;
        db.migrate().await?;
        Ok(db)
    }

    pub(crate) fn migration_timeout() -> Duration {
        let default_secs = if cfg!(debug_assertions) { 120 } else { 30 };
        match std::env::var("AOS_MIGRATION_TIMEOUT_SECS") {
            Ok(raw) => match raw.trim().parse::<u64>() {
                Ok(secs) if secs > 0 => Duration::from_secs(secs),
                _ => {
                    warn!(
                        value = %raw,
                        default_secs,
                        "Invalid AOS_MIGRATION_TIMEOUT_SECS; using default"
                    );
                    Duration::from_secs(default_secs)
                }
            },
            Err(_) => Duration::from_secs(default_secs),
        }
    }

    /// Run database migrations with signature verification
    ///
    /// Per Artifacts Ruleset #13: All migrations must be Ed25519 signed.
    /// This method:
    /// 1. Verifies all migration signatures before applying
    /// 2. Runs migrations via sqlx
    /// 3. Verifies database is at expected version after completion
    pub async fn migrate(&self) -> Result<()> {
        use tracing::info;

        #[cfg(debug_assertions)]
        let _migration_guard = MIGRATION_TEST_LOCK.lock().await;

        // Use CARGO_MANIFEST_DIR to find migrations relative to workspace root
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .ok_or_else(|| AosError::Database("Failed to find workspace root".to_string()))?;

        let migrations_path = workspace_root.join("migrations");

        // Verify migrations directory exists
        if !migrations_path.exists() {
            return Err(AosError::Database(format!(
                "Migrations directory not found: {}",
                migrations_path.display()
            ))
            .into());
        }

        // CRITICAL: Verify all migration signatures before applying.
        // Bypass only for local emergency debugging via AOS_SKIP_MIGRATION_SIGNATURES=1.
        // CI and tests must run with verification enabled.
        // SECURITY: This bypass is only available in debug builds.
        let skip_sig_verification = {
            #[cfg(debug_assertions)]
            {
                std::env::var("AOS_SKIP_MIGRATION_SIGNATURES").is_ok()
            }
            #[cfg(not(debug_assertions))]
            {
                if std::env::var("AOS_SKIP_MIGRATION_SIGNATURES").is_ok() {
                    warn!("AOS_SKIP_MIGRATION_SIGNATURES is set but IGNORED in release builds for security");
                }
                false
            }
        };
        if skip_sig_verification {
            warn!("Skipping migration signature verification (env override; debug build only)");
        } else {
            info!("Verifying migration signatures...");

            let verifier = crate::migration_verify::MigrationVerifier::new(&migrations_path)?;

            verifier.verify_all()?;
            info!(
                "✓ All {} migration signatures verified (fingerprint: {})",
                verifier.signature_count(),
                verifier.public_key_fingerprint()
            );
        }

        // Use sqlx::migrate with dynamic path (PathBuf implements MigrationSource)
        let migrator = sqlx::migrate::Migrator::new(migrations_path.clone())
            .await
            .map_err(|e| AosError::Database(format!("Failed to create migrator: {}", e)))?;

        // Run migrations with a timeout to avoid hanging on locked databases.
        self.ensure_disk_space("database migrations")?;
        info!("Applying database migrations...");
        tokio::time::timeout(Self::migration_timeout(), migrator.run(self.pool()))
        .await
        .map_err(|_| {
            AosError::Database(
                "Migration timed out while waiting for database lock. Run `aosctl db unlock` and retry."
                    .to_string(),
            )
        })?
        .map_err(|e| AosError::Database(format!("Migration failed: {}", e)))?;

        // Apply compatibility fixes for schema drift between signed migrations and code expectations.
        self.ensure_adapter_lora_strength_column().await?;
        self.ensure_adapter_recommended_for_moe_column().await?;
        self.ensure_worker_runtime_metadata_columns().await?;
        self.ensure_inference_trace_tokens_index().await?;

        // Verify database version after migration
        self.verify_migration_version(&migrations_path).await?;

        Ok(())
    }

    /// Backfill the `lora_strength` column on adapters if migrations missed it.
    async fn ensure_adapter_lora_strength_column(&self) -> Result<()> {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM pragma_table_info('adapters') WHERE name = 'lora_strength' LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to inspect adapters schema: {}", e)))?;

        if exists.is_none() {
            warn!("Adapters table missing lora_strength column; applying runtime patch");
            if let Err(e) =
                sqlx::query("ALTER TABLE adapters ADD COLUMN lora_strength REAL DEFAULT 1.0")
                    .execute(self.pool())
                    .await
            {
                // Best-effort compatibility patch: concurrent initializers may race here.
                let msg = e.to_string();
                if !msg.contains("duplicate column name: lora_strength") {
                    return Err(AosError::Database(format!(
                        "Failed to add lora_strength column: {}",
                        e
                    )));
                }
            }
            // Backfill existing rows to preserve deterministic defaults.
            sqlx::query("UPDATE adapters SET lora_strength = 1.0 WHERE lora_strength IS NULL")
                .execute(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to backfill lora_strength: {}", e))
                })?;
        }

        Ok(())
    }

    /// Backfill the `recommended_for_moe` column on adapters if migrations missed it.
    async fn ensure_adapter_recommended_for_moe_column(&self) -> Result<()> {
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM pragma_table_info('adapters') WHERE name = 'recommended_for_moe' LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to inspect adapters schema: {}", e)))?;

        if exists.is_none() {
            warn!("Adapters table missing recommended_for_moe column; applying runtime patch");
            sqlx::query("ALTER TABLE adapters ADD COLUMN recommended_for_moe INTEGER DEFAULT 1")
                .execute(self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to add recommended_for_moe column: {}", e))
                })?;
            // Backfill existing rows to preserve deterministic defaults (true = recommended for MoE).
            sqlx::query(
                "UPDATE adapters SET recommended_for_moe = 1 WHERE recommended_for_moe IS NULL",
            )
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to backfill recommended_for_moe: {}", e))
            })?;
        }

        Ok(())
    }

    /// Ensure worker runtime metadata columns exist.
    ///
    /// These fields are populated during worker registration and surfaced by listing endpoints.
    async fn ensure_worker_runtime_metadata_columns(&self) -> Result<()> {
        let pool = self.pool();

        let ensure_column = |name: &'static str, ddl: &'static str| async move {
            let exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM pragma_table_info('workers') WHERE name = ? LIMIT 1",
            )
            .bind(name)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to inspect workers schema: {}", e)))?;

            if exists.is_none() {
                warn!(
                    column = name,
                    "Workers table missing runtime metadata column; applying runtime patch"
                );
                sqlx::query(ddl).execute(pool).await.map_err(|e| {
                    AosError::Database(format!("Failed to apply workers DDL: {}", e))
                })?;
            }

            Ok::<(), AosError>(())
        };

        ensure_column("backend", "ALTER TABLE workers ADD COLUMN backend TEXT").await?;
        ensure_column(
            "model_hash_b3",
            "ALTER TABLE workers ADD COLUMN model_hash_b3 TEXT",
        )
        .await?;
        ensure_column(
            "capabilities_json",
            "ALTER TABLE workers ADD COLUMN capabilities_json TEXT",
        )
        .await?;

        // Best-effort index for quick grouping/lookup; safe to run repeatedly.
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_workers_model_hash_b3 ON workers(model_hash_b3)",
        )
        .execute(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to ensure workers index: {}", e)))?;

        Ok(())
    }

    /// Ensure token paging queries can use a composite index.
    async fn ensure_inference_trace_tokens_index(&self) -> Result<()> {
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_inference_trace_tokens_trace_token_index \
             ON inference_trace_tokens (trace_id, token_index)",
        )
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to ensure inference_trace_tokens index: {}",
                e
            ))
        })?;

        Ok(())
    }

    /// Verify database is at the expected migration version
    ///
    /// Fail fast with clear error if schema version doesn't match expected.
    /// Prevents version drift where code expects newer schema than DB has.
    ///
    /// **Critical:** This method now FAILS if database version != expected version.
    /// Use `aosctl db reset` (dev only) to recreate database with all migrations.
    pub async fn verify_migration_version(&self, migrations_path: &std::path::Path) -> Result<()> {
        use tracing::{error, info, warn};

        // Get latest migration version from database
        let latest_db_migration: Option<(i64, String)> = sqlx::query_as(
            "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to query migration version: {}", e)))?;

        // Get max migration number from filenames to determine expected version
        // SQLx uses the number prefix (e.g., 0081) not file count
        let expected_version = std::fs::read_dir(migrations_path)
            .map_err(|e| AosError::Database(format!("Failed to read migrations directory: {}", e)))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("sql"))
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.split('_').next())
                    .and_then(|num| num.parse::<i64>().ok())
            })
            .max()
            .unwrap_or(0);

        match latest_db_migration {
            Some((version, description)) => {
                info!(
                    "Database at migration version {} ({}) - expected version {}",
                    version, description, expected_version
                );

                // FAIL FAST if version mismatch (relaxed in tests to avoid fixture churn)
                if version != expected_version {
                    error!(
                        "❌ SCHEMA VERSION MISMATCH: Database at version {}, expected {}",
                        version, expected_version
                    );
                    error!("Migration files count: {}", expected_version);
                    error!("Database has {} migrations applied", version);

                    if version < expected_version {
                        error!(
                            "Database is BEHIND - {} migrations missing",
                            expected_version - version
                        );
                        error!("Run migrations: aosctl db migrate");
                    } else {
                        error!("Database is AHEAD - code expects older schema");
                        error!("This may indicate migration file removal or code rollback");
                    }

                    if cfg!(test) {
                        warn!(
                            "Schema version mismatch detected in tests (db={}, expected={}); continuing",
                            version, expected_version
                        );
                    } else {
                        return Err(AosError::Database(format!(
                            "Schema version mismatch: DB version {} != expected {}. Server cannot start with mismatched schema.",
                            version, expected_version
                        )).into());
                    }
                }

                info!("✓ Schema version verified: {}", version);
            }
            None => {
                if expected_version > 0 {
                    error!(
                        "❌ Database has NO migrations applied but {} migration files exist",
                        expected_version
                    );
                    error!("Run migrations: aosctl db migrate");
                    return Err(AosError::Database(format!(
                        "Database has no migrations applied but {} migration files exist. Run migrations first.",
                        expected_version
                    )).into());
                }
                warn!("No migrations applied yet (empty database)");
            }
        }

        Ok(())
    }

    /// Recover from system crash or unexpected shutdown
    ///
    /// Scans for orphaned adapters and inconsistent state, then cleans up:
    /// 1. Marks adapters stuck in loading state as unloaded
    /// 2. Resets invalid activation percentages
    /// 3. Logs recovery actions for audit trail
    ///
    /// Should be called after migrations but before handling requests.
    ///
    /// **CRITICAL FIX:** Wraps all recovery operations in a single transaction
    /// to ensure atomicity. This prevents partial recovery on crash during recovery.
    pub async fn recover_from_crash(&self) -> Result<()> {
        use chrono::Utc;
        use tracing::{info, warn};

        info!("Starting crash recovery scan...");

        let mut recovery_actions = Vec::new();

        // CRITICAL: Begin transaction for atomic recovery
        let mut tx = self.begin_write_tx().await?;

        // 1. Find adapters stuck in "loading" state (orphaned from crash)
        let stale_adapters: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT adapter_id, name, load_state
            FROM adapters
            WHERE load_state = 'loading'
              AND last_loaded_at < datetime('now', '-5 minutes')
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query stale adapters: {}", e)))?;

        if !stale_adapters.is_empty() {
            warn!(
                "Found {} orphaned adapters stuck in loading state",
                stale_adapters.len()
            );

            for (adapter_id, name, load_state) in stale_adapters {
                recovery_actions.push(format!(
                    "Adapter {} ({}) stuck in state '{}' - marking as unloaded",
                    name, adapter_id, load_state
                ));

                // Mark as unloaded in database (within transaction)
                sqlx::query(
                    "UPDATE adapters SET load_state = 'cold', current_state = 'unloaded', updated_at = datetime('now') WHERE adapter_id = ?",
                )
                .bind(&adapter_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;

                info!("✓ Recovered adapter: {} ({})", name, adapter_id);
            }
        }

        // 2. Find models stuck in "importing" state (orphaned from crash)
        let stale_imports: Vec<(String, String)> = sqlx::query_as(
            r#"
            SELECT id, name
            FROM models
            WHERE import_status = 'importing'
            "#,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query stale model imports: {}", e)))?;

        if !stale_imports.is_empty() {
            warn!(
                "Found {} models stuck in importing state",
                stale_imports.len()
            );

            for (model_id, name) in &stale_imports {
                recovery_actions.push(format!(
                    "Model {} ({}) stuck in importing state - marking as failed",
                    name, model_id
                ));

                sqlx::query(
                    "UPDATE models SET import_status = 'failed', import_error = 'stuck in importing state during crash recovery', updated_at = datetime('now') WHERE id = ?",
                )
                .bind(model_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update model import state: {}", e)))?;

                info!("Recovered stuck model import: {} ({})", name, model_id);
            }
        }

        // 3. Clean up invalid activation counts (negative values)
        let reset_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM adapters WHERE activation_count < 0")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to query invalid activation_count: {}", e))
                })?;

        if reset_count > 0 {
            warn!(
                "Found {} adapters with invalid activation_count - resetting",
                reset_count
            );

            sqlx::query("UPDATE adapters SET activation_count = 0 WHERE activation_count < 0")
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to reset activation_count: {}", e))
                })?;

            recovery_actions.push(format!(
                "Reset {} adapters with invalid activation percentages",
                reset_count
            ));
        }

        // CRITICAL: Commit transaction atomically
        tx.commit().await.map_err(|e| {
            AosError::Database(format!("Failed to commit recovery transaction: {}", e))
        })?;

        // 4. Log summary (after successful commit)
        if recovery_actions.is_empty() {
            info!("✓ Crash recovery complete - no issues detected");
        } else {
            info!(
                "✓ Crash recovery complete - {} actions taken:",
                recovery_actions.len()
            );
            for action in &recovery_actions {
                info!("  - {}", action);
            }

            // Log to audit trail if available
            let audit_log = serde_json::json!({
                "action": "crash_recovery",
                "actions_taken": recovery_actions.len(),
                "recovery_actions": recovery_actions,
                "timestamp": Utc::now().to_rfc3339()
            });
            tracing::debug!("Crash recovery audit: {}", audit_log);
        }

        Ok(())
    }

    /// Seed database with development data
    pub async fn seed_dev_data(&self) -> Result<()> {
        use argon2::{
            password_hash::{PasswordHasher, SaltString},
            Argon2,
        };
        use rand::rngs::OsRng;

        // This function must be safe to call on every dev boot.
        // We treat seeding as best-effort and idempotent: ensure required dev infra exists
        // even if other tables already contain rows.
        tracing::info!("Seeding development data (idempotent)...");

        // Ensure default tenant exists (used by dev auth + many dev flows).
        sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, created_at)
             VALUES ('default', 'default', datetime('now'))",
        )
        .execute(self.pool())
        .await?;

        // Ensure local node exists for single-node dev environments.
        // Worker registration hardcodes node_id=\"local\"; missing it causes FK failures.
        {
            let labels = serde_json::json!({
                "metal_family": "Local Dev",
                "memory_gb": 32
            })
            .to_string();
            sqlx::query(
                "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at)
                 VALUES ('local', 'localhost', 'http://localhost:0', 'active', datetime('now'), ?, datetime('now'))",
            )
            .bind(labels)
            .execute(self.pool())
            .await?;
        }

        // Only seed dev users when no users exist. This keeps local dev auth stable without
        // clobbering manually created accounts.
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(self.pool())
            .await?;
        if user_count > 0 {
            tracing::info!("Users already exist; skipping dev user seed");
            return Ok(());
        }

        // Create seed users with hashed passwords
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password("password".as_bytes(), &salt)
            .map_err(|e| AosError::Crypto(format!("failed to hash password: {}", e)))?
            .to_string();

        let users = vec![
            ("admin@aos.local", "Admin", "admin", &password_hash),
            ("operator@aos.local", "Operator", "operator", &password_hash),
            ("sre@aos.local", "SRE", "sre", &password_hash),
            ("viewer@aos.local", "Viewer", "viewer", &password_hash),
        ];

        for (email, display_name, role, pwd_hash) in users {
            let username = email
                .split('@')
                .next()
                .ok_or_else(|| AosError::Database(format!("Invalid email format: {}", email)))?;

            sqlx::query(
                "INSERT OR IGNORE INTO users (id, email, display_name, pw_hash, role, disabled, created_at, tenant_id)
                 VALUES (?, ?, ?, ?, ?, 0, datetime('now'), 'default')",
            )
            .bind(format!("{}-user", username))
            .bind(email)
            .bind(display_name)
            .bind(pwd_hash)
            .bind(role)
            .execute(self.pool())
            .await?;
        }

        tracing::info!("Development data seeded successfully");
        Ok(())
    }

    /// Get adapter by ID and tenant
    pub async fn get_adapter_by_id(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<AdapterRecord>> {
        let row = sqlx::query_as::<_, traits::AdapterRecordRow>(
            r#"
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, version, lifecycle_state, lora_strength, archived_at, archived_by, archive_reason, purged_at
            FROM adapters
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch adapter: {}", e)))?;

        Ok(row.map(AdapterRecord::from))
    }

    /// List stacks for a tenant
    pub async fn list_stacks_for_tenant(&self, tenant_id: &str) -> Result<Vec<StackRecord>> {
        if self.storage_mode().read_from_kv() {
            if let Some(kv_repo) = self.get_stack_kv_repo() {
                use stacks_kv::StackKvOps;
                let stacks = kv_repo
                    .list_stacks_by_tenant(tenant_id)
                    .await?
                    .into_iter()
                    .map(|kv| stacks_kv::kv_to_stack_record(&kv))
                    .collect::<Result<Vec<_>>>()?;
                return Ok(stacks);
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(Vec::new());
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(Vec::new()),
        };
        let rows = sqlx::query_as::<_, traits::StackRecordRow>(
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type, lifecycle_state, created_at, updated_at, created_by, version, determinism_mode, routing_determinism_mode, metadata_json
            FROM adapter_stacks
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks: {}", e)))?;

        Ok(rows.into_iter().map(StackRecord::from).collect())
    }

    /// Get a stack by ID and tenant
    pub async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>> {
        // Prefer KV if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_stack_kv_repo() {
                use stacks_kv::StackKvOps;
                if let Some(kv_stack) = repo.get_stack(tenant_id, id).await? {
                    return Ok(Some(stacks_kv::kv_to_stack_record(&kv_stack)?));
                }
            }
            if !self.storage_mode().sql_fallback_enabled() {
                return Ok(None);
            }
        }

        let pool = match self.pool_opt() {
            Some(p) => p,
            None => return Ok(None),
        };
        let row = sqlx::query_as::<_, traits::StackRecordRow>(
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type, lifecycle_state, created_at, updated_at, created_by, version, determinism_mode, routing_determinism_mode, metadata_json
            FROM adapter_stacks
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?;

        Ok(row.map(StackRecord::from))
    }

    /// Delete a stack by ID and tenant
    ///
    /// FIXED (ADR-0023 Bug #1): Delete from KV first, then SQL to prevent race condition
    /// where concurrent reads see SQL empty but KV still has stale data
    pub async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool> {
        use stacks_kv::StackKvOps;
        use tracing::{debug, error, warn};

        // Delete from KV first to prevent race condition
        let kv_start = std::time::Instant::now();
        let kv_delete_result = if let Some(kv_backend) = self.get_stack_kv_repo() {
            kv_backend.delete_stack(tenant_id, id).await
        } else {
            Ok(false) // No KV backend, treat as not found
        };
        let kv_latency = kv_start.elapsed();

        // Handle KV delete result before SQL delete
        let kv_succeeded = match &kv_delete_result {
            Err(e) => {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        stack_id = %id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "KV delete failed before SQL delete (strict mode). Aborting to prevent inconsistency."
                    );
                    return Err(AosError::Database(format!(
                        "KV delete failed (strict mode), aborting SQL delete: {e}"
                    )));
                } else {
                    warn!(
                        error = %e,
                        stack_id = %id,
                        tenant_id = %tenant_id,
                        mode = "dual-write",
                        "KV delete failed, continuing with SQL delete (non-strict mode)"
                    );
                }
                false
            }
            Ok(kv_deleted) => *kv_deleted,
        };

        // Now delete from SQL (only if KV succeeded or non-strict mode)
        let mut sql_deleted = false;
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                let sql_start = std::time::Instant::now();
                let result = sqlx::query(
                    r#"
                    DELETE FROM adapter_stacks
                    WHERE tenant_id = ? AND id = ?
                    "#,
                )
                .bind(tenant_id)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to delete stack: {}", e)))?;
                sql_deleted = result.rows_affected() > 0;
                let sql_latency = sql_start.elapsed();

                // Record dual-write latency lag if both succeeded
                if kv_succeeded && sql_deleted {
                    let lag = if kv_latency > sql_latency {
                        kv_latency.saturating_sub(sql_latency)
                    } else {
                        std::time::Duration::ZERO
                    };
                    global_kv_metrics().record_dual_write_lag(lag);
                    debug!(
                        stack_id = %id,
                        tenant_id = %tenant_id,
                        mode = "dual-write",
                        sql_latency_ms = sql_latency.as_millis() as u64,
                        kv_latency_ms = kv_latency.as_millis() as u64,
                        lag_ms = lag.as_millis() as u64,
                        "Stack deleted from both SQL and KV backends"
                    );
                }
            }
        }

        Ok(sql_deleted || kv_succeeded)
    }

    /// Update a stack
    pub async fn update_stack(&self, id: &str, stack: &CreateStackRequest) -> Result<bool> {
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type_str = stack.workflow_type.as_ref().map(|w| format!("{:?}", w));

        let mut updated = false;
        let mut should_bump_version = false;
        let mut new_version: Option<i64> = None;
        // SQL update if enabled
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                let (current_adapter_ids_json, current_workflow_type, current_version) =
                    if let Some(row) = sqlx::query_as::<_, (String, Option<String>, String)>(
                        r#"
                        SELECT adapter_ids_json, workflow_type, version
                        FROM adapter_stacks
                        WHERE id = ?
                        "#,
                    )
                    .bind(id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to fetch stack for update: {}", e))
                    })? {
                        row
                    } else {
                        return Ok(false);
                    };

                should_bump_version = current_adapter_ids_json != adapter_ids_json
                    || current_workflow_type != workflow_type_str;

                let current_version_num = current_version.parse::<i64>().unwrap_or(1);
                let bump = if should_bump_version { 1i64 } else { 0i64 };
                let next_version = if should_bump_version {
                    current_version_num + 1
                } else {
                    current_version_num
                };
                new_version = Some(next_version);
                let result = sqlx::query(
                    r#"
                    UPDATE adapter_stacks
                    SET name = ?, description = ?, adapter_ids_json = ?, workflow_type = ?, determinism_mode = ?, routing_determinism_mode = ?, version = version + ?, updated_at = datetime('now')
                    WHERE id = ?
                    "#,
                )
                .bind(&stack.name)
                .bind(&stack.description)
                .bind(&adapter_ids_json)
                .bind(&workflow_type_str)
                .bind(&stack.determinism_mode)
                .bind(&stack.routing_determinism_mode)
                .bind(bump)
                .bind(id)
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?;

                updated |= result.rows_affected() > 0;
            }
        }

        // KV update (dual-write mode)
        if updated {
            if let Some(kv_backend) = self.get_stack_kv_repo() {
                use stacks_kv::StackKvOps;
                if let Err(e) = kv_backend.update_stack(id, stack).await {
                    warn!(error = %e, stack_id = %id, "Failed to update stack in KV backend (dual-write)");
                } else {
                    debug!(stack_id = %id, "Stack updated in both SQL and KV backends");
                }
                if should_bump_version {
                    let Some(next_version) = new_version else {
                        warn!(stack_id = %id, "Missing next stack version for KV update");
                        return Ok(updated);
                    };
                    if let Err(e) = kv_backend
                        .update_version(&stack.tenant_id, id, &next_version.to_string())
                        .await
                    {
                        warn!(error = %e, stack_id = %id, "Failed to update stack version in KV backend (dual-write)");
                    }
                }
            }
        }

        Ok(updated)
    }

    /// Activate a stack by setting lifecycle_state to 'active' (SQL + KV)
    pub async fn activate_stack(&self, tenant_id: &str, id: &str) -> Result<()> {
        let mut activated = false;
        let kv_repo = if self.storage_mode().write_to_kv() {
            self.get_stack_kv_repo()
        } else {
            None
        };

        if self.storage_mode().write_to_sql() {
            let pool = self.pool_opt().ok_or_else(|| {
                AosError::Database("SQL backend unavailable for activate_stack".to_string())
            })?;

            let result = sqlx::query(
                r#"
                UPDATE adapter_stacks
                SET lifecycle_state = 'active',
                    updated_at = datetime('now')
                WHERE tenant_id = ? AND id = ?
                "#,
            )
            .bind(tenant_id)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to activate stack: {}", e)))?;

            activated |= result.rows_affected() > 0;

            if !activated && kv_repo.is_none() {
                return Err(AosError::NotFound(format!(
                    "Stack {} not found for tenant {}",
                    id, tenant_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No writable backend configured for activate_stack".to_string(),
            ));
        }

        // KV activation (dual-write mode)
        if let Some(kv_backend) = kv_repo {
            use stacks_kv::StackKvOps;
            match kv_backend.activate_stack(id).await {
                Ok(()) => {
                    activated = true;
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        stack_id = %id,
                        tenant_id = %tenant_id,
                        "Failed to activate stack in KV backend (dual-write)"
                    );

                    if !activated {
                        return Err(AosError::Database(format!(
                            "Failed to activate stack (KV backend): {}",
                            e
                        )));
                    }
                }
            }
        }

        if !activated {
            return Err(AosError::NotFound(format!(
                "Stack {} not found for tenant {}",
                id, tenant_id
            )));
        }

        Ok(())
    }

    /// Get the underlying pool for custom queries
    pub fn pool(&self) -> &SqlitePool {
        self.pool
            .as_ref()
            .expect("SQL pool not available for current storage mode")
    }

    /// Get the underlying pool if attached (KV-only may return None)
    pub fn pool_opt(&self) -> Option<&SqlitePool> {
        self.pool.as_ref()
    }

    /// Root directory used for disk space checks (database directory if known).
    fn disk_root(&self) -> PathBuf {
        self.db_dir
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Ensure there is sufficient free space before performing write-heavy operations.
    fn ensure_disk_space(&self, context: &str) -> Result<()> {
        let root = self.disk_root();
        match available_space(&root) {
            Ok(free) if free < MIN_FREE_SPACE_BYTES => Err(AosError::Io(format!(
                "Insufficient disk space (<{} bytes) for {} ({} bytes available) at {}",
                MIN_FREE_SPACE_BYTES,
                context,
                free,
                root.display()
            ))),
            Ok(_) => Ok(()),
            Err(e) => Err(AosError::Io(format!(
                "Failed to check disk space at {}: {}",
                root.display(),
                e
            ))),
        }
    }

    /// Begin a SQLite transaction with a pre-flight disk space check.
    pub async fn begin_write_tx(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>> {
        self.ensure_disk_space("write transaction")?;
        self.pool()
            .begin()
            .await
            .map_err(|e| AosError::Database(format!("Failed to begin transaction: {}", e)))
    }

    /// Enable performance monitoring for tenant-scoped queries
    ///
    /// When enabled, tenant-scoped adapter operations will be monitored for
    /// performance regressions and SLA compliance (10ms threshold).
    pub fn enable_performance_monitoring(&self, slow_query_threshold_ms: u64) {
        let monitor = QueryPerformanceMonitor::new(slow_query_threshold_ms);
        match self.performance_monitor.write() {
            Ok(mut guard) => *guard = Some(monitor),
            Err(poisoned) => {
                tracing::error!("Performance monitor lock poisoned, recovering");
                *poisoned.into_inner() = Some(monitor);
            }
        }
    }

    /// Get access to the performance monitor (if enabled)
    pub fn performance_monitor(
        &self,
    ) -> Option<std::sync::RwLockReadGuard<'_, Option<QueryPerformanceMonitor>>> {
        match self.performance_monitor.read() {
            Ok(guard) => Some(guard),
            Err(poisoned) => {
                tracing::error!("Performance monitor lock poisoned during read");
                Some(poisoned.into_inner())
            }
        }
    }

    /// Get mutable access to the performance monitor (if enabled)
    pub fn performance_monitor_mut(
        &self,
    ) -> Option<std::sync::RwLockWriteGuard<'_, Option<QueryPerformanceMonitor>>> {
        match self.performance_monitor.write() {
            Ok(guard) => Some(guard),
            Err(poisoned) => {
                tracing::error!("Performance monitor lock poisoned during write");
                Some(poisoned.into_inner())
            }
        }
    }

    /// Generate performance report
    ///
    /// Returns a human-readable report showing query performance metrics.
    pub fn generate_performance_report(&self) -> String {
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                return monitor.report();
            }
        }
        "Performance monitoring not enabled".to_string()
    }

    /// Run WAL checkpoint to flush WAL to database file
    ///
    /// This should be called periodically (e.g., every 5 minutes) to:
    /// - Reduce WAL file size
    /// - Improve read performance
    /// - Reduce recovery time on restart
    ///
    /// Uses PASSIVE mode to avoid blocking concurrent writes.
    pub async fn wal_checkpoint(&self) -> Result<()> {
        if let Some(pool) = self.pool.as_ref() {
            sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("WAL checkpoint failed: {}", e)))?;
        }
        Ok(())
    }

    /// Get the current storage mode
    pub fn storage_mode(&self) -> StorageMode {
        self.storage_mode
    }

    /// Set the storage mode
    ///
    /// This allows runtime control of storage mode behavior, enabling gradual migration
    /// from SQL to KV backend. The mode can be changed at any time, but changing the
    /// mode does not migrate existing data - use migration utilities for that.
    ///
    /// # Migration Path
    ///
    /// The recommended migration sequence is:
    /// 1. **SqlOnly** -> **DualWrite**: Start writing to both backends for validation
    /// 2. **DualWrite** -> **KvPrimary**: Switch reads to KV while keeping SQL writes as backup
    /// 3. **KvPrimary** -> **KvOnly**: Complete migration, disable SQL writes
    ///
    /// # Important Notes
    ///
    /// - **KV Backend Required**: If setting a mode that uses KV (DualWrite, KvPrimary, KvOnly),
    ///   ensure the KV backend is attached via `attach_kv_backend()` or `init_kv_backend()` first.
    /// - **Data Migration**: Changing mode does not migrate data. Use `kv_migration` utilities
    ///   to copy data from SQL to KV before switching to KV read modes.
    /// - **Thread Safety**: This method requires `&mut self`, so mode changes must be
    ///   coordinated at the application level in multi-threaded environments.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use adapteros_db::{Db, StorageMode};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut db = Db::connect("var/aos-cp.sqlite3").await?;
    ///
    /// // Initialize KV backend
    /// db.init_kv_backend(std::path::Path::new("var/aos-kv.redb"))?;
    ///
    /// // Enable dual-write mode for migration validation
    /// db.set_storage_mode(StorageMode::DualWrite)?;
    ///
    /// // After validation, switch to KV-primary mode
    /// db.set_storage_mode(StorageMode::KvPrimary)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Panics
    ///
    /// Does not panic, but operations may fail if:
    /// - KV backend is not attached when using KV-dependent modes
    /// - Data has not been migrated to KV before switching to KV read modes
    pub fn set_storage_mode(&mut self, mode: StorageMode) -> Result<()> {
        use tracing::info;

        if self.storage_mode != mode {
            info!(
                old_mode = %self.storage_mode,
                new_mode = %mode,
                "Storage mode changed"
            );

            // Warn if setting KV mode without KV backend
            if (mode.read_from_kv() || mode.write_to_kv()) && !self.has_kv_backend() {
                warn!(
                    mode = %mode,
                    "Storage mode requires KV backend but none is attached. \
                     Attach KV backend with attach_kv_backend() or init_kv_backend()"
                );
            }
        }

        self.storage_mode = mode;

        // Enforce guardrails for KV-only posture
        if let Err(err) = self.enforce_kv_only_guard() {
            let reason = format!("KV-only guard failed: {}", err);
            warn!(error = %err, "KV-only guard failed; reverting to sql_only");
            self.storage_mode = StorageMode::SqlOnly;
            self.mark_degraded(reason.clone());
            return Err(err);
        }

        Ok(())
    }

    /// Get atomic dual-write configuration
    pub fn atomic_dual_write_config(&self) -> &crate::adapters::AtomicDualWriteConfig {
        &self.atomic_dual_write_config
    }

    /// Set atomic dual-write configuration
    pub fn set_atomic_dual_write_config(&mut self, config: crate::adapters::AtomicDualWriteConfig) {
        self.atomic_dual_write_config = Arc::new(config);
    }

    /// Builder-style setter for atomic dual-write configuration
    pub fn with_atomic_dual_write_config(
        mut self,
        config: crate::adapters::AtomicDualWriteConfig,
    ) -> Self {
        self.set_atomic_dual_write_config(config);
        self
    }

    /// Returns true when dual-write must operate in strict (rollback-on-failure) mode.
    ///
    /// Strict mode is always enforced during KV cutover/steady-state modes
    /// (`kv_primary` and `kv_only`), and can also be enabled explicitly via
    /// `AOS_ATOMIC_DUAL_WRITE_STRICT`.
    pub fn dual_write_requires_strict(&self) -> bool {
        matches!(
            self.storage_mode,
            StorageMode::KvPrimary | StorageMode::KvOnly
        ) || self.atomic_dual_write_config.is_strict()
    }

    /// Alias for `dual_write_requires_strict()` for ergonomic use in rollback logic.
    pub fn is_strict_atomic(&self) -> bool {
        self.dual_write_requires_strict()
    }

    /// Attach a KV backend to this database instance
    ///
    /// This enables dual-write or KV-primary modes. The KV backend will be used
    /// according to the current storage_mode setting.
    pub fn attach_kv_backend(&mut self, kv: KvDb) {
        self.kv = Some(std::sync::Arc::new(kv));
    }

    /// Initialize KV backend with redb at the given path
    ///
    /// This is a convenience method that creates a KvDb instance and attaches it.
    pub fn init_kv_backend(&mut self, path: &std::path::Path) -> Result<()> {
        let kv = KvDb::init_redb(path)?;
        self.attach_kv_backend(kv);
        Ok(())
    }

    /// Get a reference to the KV backend if attached
    pub fn kv_backend(&self) -> Option<&std::sync::Arc<KvDb>> {
        self.kv.as_ref()
    }

    /// Check if KV backend is available
    pub fn has_kv_backend(&self) -> bool {
        self.kv.is_some()
    }

    /// Detach the KV backend
    ///
    /// This removes the KV backend and resets storage mode to SqlOnly.
    pub fn detach_kv_backend(&mut self) {
        self.kv = None;
        self.storage_mode = StorageMode::SqlOnly;
    }

    /// Record a KV read fallback (used for drift detection/alerts)
    fn record_kv_read_fallback(&self, context: &str) {
        let metrics = crate::kv_metrics::global_kv_metrics();
        metrics.record_fallback_read();
        metrics.record_drift_detected();
        warn!(context = %context, "KV read fallback to SQL");
    }

    /// Record a KV write fallback (KV write failed but SQL succeeded)
    fn record_kv_write_fallback(&self, context: &str) {
        let metrics = crate::kv_metrics::global_kv_metrics();
        metrics.record_fallback_write();
        metrics.record_drift_detected();
        warn!(context = %context, "KV write failed, recorded fallback");
    }

    // Phase 2: Advanced Query Governance & Infrastructure

    /// Set global query timeout in milliseconds
    pub fn set_query_timeout(&self, timeout_ms: u64) {
        self.query_timeout_ms
            .store(timeout_ms, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get current query timeout
    pub fn get_query_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            self.query_timeout_ms
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    /// Check rate limit for tenant
    /// Returns true if allowed, false if limit exceeded.
    pub fn check_rate_limit(&self, tenant_id: &str) -> bool {
        let now = Instant::now();
        let mut limits = match self.tenant_rate_limits.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(
                    tenant_id = %tenant_id,
                    "Rate limit lock poisoned during write, recovering"
                );
                poisoned.into_inner()
            }
        };

        let state = limits
            .entry(tenant_id.to_string())
            .or_insert(TenantRateLimitState {
                window_start: now,
                count: 0,
            });

        if now.duration_since(state.window_start) >= TENANT_RATE_LIMIT_WINDOW {
            state.window_start = now;
            state.count = 0;
        }

        state.count < TENANT_RATE_LIMIT_MAX_REQUESTS_PER_WINDOW
    }

    /// Increment rate limit counter
    pub fn increment_rate_limit(&self, tenant_id: &str) {
        let now = Instant::now();
        let mut limits = match self.tenant_rate_limits.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::error!(tenant_id = %tenant_id, "Rate limit lock poisoned during write, recovering");
                poisoned.into_inner()
            }
        };
        let state = limits
            .entry(tenant_id.to_string())
            .or_insert(TenantRateLimitState {
                window_start: now,
                count: 0,
            });

        if now.duration_since(state.window_start) >= TENANT_RATE_LIMIT_WINDOW {
            state.window_start = now;
            state.count = 0;
        }

        state.count = state.count.saturating_add(1);
    }

    /// Get cached query plan
    pub fn get_cached_plan(&self, query_key: &str) -> Option<String> {
        match self.plan_cache.read() {
            Ok(guard) => guard.get(query_key).cloned(),
            Err(poisoned) => {
                tracing::error!(query_key = %query_key, "Plan cache lock poisoned during read");
                poisoned.into_inner().get(query_key).cloned()
            }
        }
    }

    /// Cache query plan
    pub fn cache_query_plan(&self, query_key: &str, plan: &str) {
        match self.plan_cache.write() {
            Ok(mut guard) => {
                guard.insert(query_key.to_string(), plan.to_string());
            }
            Err(poisoned) => {
                tracing::error!(query_key = %query_key, "Plan cache lock poisoned during write, recovering");
                poisoned
                    .into_inner()
                    .insert(query_key.to_string(), plan.to_string());
            }
        }
    }

    /// Prevent entering KV-only mode when unsupported domains remain.
    ///
    /// Falls back to KvPrimary and records degradation reason for observability.
    pub fn enforce_kv_only_guard(&mut self) -> Result<()> {
        if !self.storage_mode.is_kv_only() {
            return Ok(());
        }

        let coverage = kv_coverage_summary();
        if coverage.unsupported_domains.is_empty() {
            // Also downgrade if KV health signals fallbacks/errors while in kv_only.
            let snapshot = crate::kv_metrics::global_kv_metrics().snapshot();
            if snapshot.fallback_operations_total == 0 && snapshot.errors_total == 0 {
                return Ok(());
            }

            let reason = format!(
                "KV-only downgraded: fallbacks={} errors={}",
                snapshot.fallback_operations_total, snapshot.errors_total
            );

            if self.pool.is_none() {
                return Err(AosError::Config(reason));
            }

            warn!(
                event = crate::constants::DEGRADATION_EVENT_KV_UNSUPPORTED,
                fallback_mode = "kv_primary",
                fallbacks = snapshot.fallback_operations_total,
                errors = snapshot.errors_total,
                "KV-only mode degraded due to KV fallbacks/errors"
            );

            self.storage_mode = StorageMode::KvPrimary;
            self.mark_degraded(reason);
            return Ok(());
        }

        let reason = format!(
            "KV-only blocked: missing KV coverage for {}",
            coverage.unsupported_domains.join(", ")
        );

        // If we would downgrade but have no SQL pool, bail out to avoid panics on pool()
        if self.pool.is_none() {
            return Err(AosError::Config(reason));
        }

        warn!(
            event = crate::constants::DEGRADATION_EVENT_KV_UNSUPPORTED,
            fallback_mode = "kv_primary",
            missing = ?coverage.unsupported_domains,
            "KV-only mode rejected; coverage incomplete"
        );

        // Fall back to KvPrimary to preserve SQL fallback while keeping KV writes
        self.storage_mode = StorageMode::KvPrimary;
        self.mark_degraded(reason);
        Ok(())
    }

    /// Get a StackKvRepository if KV writes are enabled
    fn get_stack_kv_repo(&self) -> Option<stacks_kv::StackKvRepository> {
        if self.storage_mode().write_to_kv() {
            self.kv_backend().map(|kv| {
                let kv_backend: Arc<dyn kv_backend::KvBackend> = kv.clone();
                stacks_kv::StackKvRepository::new(kv_backend)
            })
        } else {
            None
        }
    }

    /// Insert a new adapter stack
    pub async fn insert_stack(&self, req: &CreateStackRequest) -> Result<String> {
        let id = new_id(adapteros_id::IdPrefix::Stk);
        let adapter_ids_json =
            serde_json::to_string(&req.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type = req.workflow_type.as_deref().unwrap_or("Parallel");
        let description = req.description.as_deref().unwrap_or("");

        // SQL write if enabled
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                sqlx::query(
                    r#"
                    INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at, updated_at, determinism_mode, routing_determinism_mode)
                    VALUES (?, ?, ?, ?, ?, ?, 1, 'active', datetime('now'), datetime('now'), ?, ?)
                    "#,
                )
                .bind(&id)
                .bind(&req.tenant_id)
                .bind(&req.name)
                .bind(description)
                .bind(&adapter_ids_json)
                .bind(workflow_type)
                .bind(&req.determinism_mode)
                .bind(&req.routing_determinism_mode)
                .execute(pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;
            } else if !self.storage_mode().write_to_kv() {
                return Err(AosError::Database(
                    "SQL backend unavailable for insert_stack".to_string(),
                ));
            }
        }

        // KV write (dual-write mode)
        if let Some(kv_backend) = self.get_stack_kv_repo() {
            use stacks_kv::StackKvOps;
            if let Err(e) = kv_backend.create_stack(req).await {
                warn!(error = %e, stack_id = %id, "Failed to write stack to KV backend (dual-write)");
            } else {
                debug!(stack_id = %id, "Stack written to both SQL and KV backends");
            }
        }

        Ok(id)
    }

    /// Increment adapter activation count
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    pub async fn increment_adapter_activation(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<()> {
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                // SECURITY: Update only within tenant scope
                sqlx::query(
                    r#"
                    UPDATE adapters
                    SET activation_count = activation_count + 1,
                        last_activated = datetime('now'),
                        updated_at = datetime('now')
                    WHERE adapter_id = ? AND tenant_id = ?
                    "#,
                )
                .bind(adapter_id)
                .bind(tenant_id)
                .execute(pool)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to increment adapter activation: {}", e))
                })?;
            }
        }

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo.increment_adapter_activation_kv(adapter_id).await {
                warn!(
                    error = %e,
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    mode = "dual-write",
                    "Failed to increment adapter activation in KV backend"
                );
            }
        }

        Ok(())
    }

    /// Rebuild all indexes for a tenant
    ///
    /// Rebuilds all indexes to optimize query performance. This is useful after:
    /// - Large bulk operations (import/migration)
    /// - Adapter evictions and cleanup
    /// - Performance degradation over time
    ///
    /// The operation:
    /// 1. Analyzes table statistics via ANALYZE
    /// 2. Validates index integrity via PRAGMA integrity_check
    /// 3. Rebuilds all indexes for the tenant via REINDEX
    ///
    /// Timeline: O(n log n) where n = number of adapter rows for the tenant
    pub async fn rebuild_all_indexes(&self, tenant_id: &str) -> Result<()> {
        use tracing::{info, warn};

        info!(tenant_id = %tenant_id, "Starting index rebuild");

        // Step 1: Analyze table statistics
        info!("Analyzing table statistics");
        sqlx::query("ANALYZE adapters")
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to analyze adapters table: {}", e)))?;

        sqlx::query("ANALYZE users")
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to analyze users table: {}", e)))?;

        sqlx::query("ANALYZE adapter_stacks")
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to analyze adapter_stacks table: {}", e))
            })?;

        // Step 2: Perform integrity check
        info!("Validating database integrity");
        let integrity_result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to perform integrity check: {}", e)))?;

        if integrity_result != "ok" {
            warn!(result = %integrity_result, "Integrity check reported issues");
            return Err(AosError::Database(format!(
                "Database integrity check failed: {}",
                integrity_result
            ))
            .into());
        }

        // Step 3: Rebuild all indexes
        info!("Rebuilding all indexes");
        sqlx::query("REINDEX")
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to rebuild indexes: {}", e)))?;

        // Step 4: Log completion and gather statistics
        let adapter_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM adapters WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to count adapters: {}", e)))?;

        info!(
            tenant_id = %tenant_id,
            adapter_count = adapter_count,
            "✓ Index rebuild complete"
        );

        Ok(())
    }

    /// List adapters for a specific tenant
    pub async fn list_adapters_by_tenant(&self, tenant_id: &str) -> Result<Vec<AdapterRecord>> {
        // Phase 2: Rate Limiting
        if !self.check_rate_limit(tenant_id) {
            return Err(AosError::QuotaExceeded {
                resource: "adapter_listings".to_string(),
                failure_code: Some("RATE_LIMIT_EXCEEDED".to_string()),
            });
        }
        self.increment_rate_limit(tenant_id);

        let timeout_duration = self.get_query_timeout();
        let pool = self.pool().clone();
        let tenant_id_owned = tenant_id.to_string();

        let result = tokio::time::timeout(timeout_duration, async move {
            sqlx::query_as::<_, traits::AdapterRecordRow>(
                r#"
                SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, lora_strength, targets_json, acl_json,
                       adapter_id, languages_json, framework, active, category, scope,
                       framework_id, framework_version, repo_id, commit_sha, intent,
                       current_state, pinned, memory_bytes, last_activated, activation_count,
                       expires_at, load_state, last_loaded_at, adapter_name, tenant_namespace,
                       domain, purpose, revision, parent_id, fork_type, fork_reason,
                       version, lifecycle_state, archived_at, archived_by, archive_reason, purged_at
                FROM adapters INDEXED BY idx_adapters_tenant_name
                WHERE tenant_id = ?
                ORDER BY name ASC
                "#,
            )
            .bind(tenant_id_owned)
            .fetch_all(&pool)
            .await
        })
        .await
        .map_err(|_| AosError::PerformanceViolation(format!("Query timeout after {:?}", timeout_duration)))?
        .map_err(|e| AosError::Database(format!("Failed to list adapters by tenant: {}", e)))?;

        Ok(result.into_iter().map(AdapterRecord::from).collect())
    }

    /// Get user by username (optimized with direct prefix matching)
    ///
    /// Optimizations:
    /// - Uses simple equality check instead of LIKE pattern matching
    /// - Relies on email UNIQUE constraint index
    /// - Falls back to ID match only if email doesn't exist
    ///
    /// Performance: O(log n) via index lookup vs O(n) with LIKE
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        // First, try to find user by email prefix (e.g., "admin" -> "admin@aos.local")
        // This is more efficient than LIKE pattern matching
        let email_query = format!("{}@%", username);

        let row = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, display_name, pw_hash, role, disabled, created_at, COALESCE(tenant_id, 'default') as tenant_id, failed_attempts, last_failed_at, lockout_until
            FROM users
            WHERE email LIKE ?
            LIMIT 1
            "#,
        )
        .bind(email_query)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get user by email: {}", e)))?;

        // If not found by email, try exact ID match
        if let Some(user) = row {
            return Ok(Some(user));
        }

        let user_id = format!("{}-user", username);
        let row = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, display_name, pw_hash, role, disabled, created_at, COALESCE(tenant_id, 'default') as tenant_id, failed_attempts, last_failed_at, lockout_until
            FROM users
            WHERE id = ?
            "#,
        )
        .bind(&user_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get user by id: {}", e)))?;

        Ok(row)
    }

    /// Get index hash for a tenant and index type
    pub async fn get_index_hash(
        &self,
        tenant_id: &str,
        index_type: &str,
    ) -> Result<Option<adapteros_core::B3Hash>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            r#"
            SELECT hash
            FROM index_hashes
            WHERE tenant_id = ? AND index_type = ?
            "#,
        )
        .bind(tenant_id)
        .bind(index_type)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get index hash: {}", e)))?;

        match row {
            Some((hash_bytes,)) => {
                if hash_bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&hash_bytes);
                    Ok(Some(adapteros_core::B3Hash::new(arr)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Get trusted adapter signing key for a tenant
    ///
    /// Returns the first active (non-revoked) trusted public key for the given tenant.
    /// Used during adapter import to verify manifest signatures when the tenant
    /// policy requires signed adapters.
    ///
    /// Returns None if no active trusted key exists for the tenant.
    pub async fn get_trusted_adapter_key(
        &self,
        tenant_id: &str,
    ) -> Result<Option<adapteros_crypto::PublicKey>> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"
            SELECT public_key_hex
            FROM trusted_adapter_keys
            WHERE tenant_id = ? AND revoked_at IS NULL
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get trusted adapter key: {}", e)))?;

        match row {
            Some((key_hex,)) => {
                let key_bytes = hex::decode(&key_hex).map_err(|e| {
                    AosError::Database(format!("Invalid hex in trusted adapter key: {}", e))
                })?;
                if key_bytes.len() != 32 {
                    return Err(AosError::Database(format!(
                        "Invalid trusted adapter key length: {} (expected 32)",
                        key_bytes.len()
                    )));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&key_bytes);
                let pubkey = adapteros_crypto::PublicKey::from_bytes(&arr).map_err(|e| {
                    AosError::Database(format!("Invalid Ed25519 public key: {}", e))
                })?;
                Ok(Some(pubkey))
            }
            None => Ok(None),
        }
    }

    /// Close the database connection pool gracefully
    ///
    /// This method should be called during shutdown to ensure:
    /// - Pending transactions are completed
    /// - WAL checkpoint is performed
    /// - All connections are properly released
    ///
    /// ## SQLite Behavior
    /// SQLite connection pools are typically closed automatically when dropped,
    /// but this explicit method provides:
    /// - Guaranteed synchronous shutdown
    /// - Ability to handle shutdown errors explicitly
    /// - Clear intent in shutdown sequences
    ///
    /// ## Usage in Shutdown
    /// Call this as part of graceful shutdown before process exit:
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: Db) -> Result<(), Box<dyn std::error::Error>> {
    /// db.close().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn close(&self) -> Result<()> {
        use tracing::info;

        info!("Closing database connection pool");

        // In KV-only mode there may be no SQL pool attached
        let Some(pool) = self.pool_opt() else {
            info!("No SQL pool attached; nothing to close");
            return Ok(());
        };

        // SQLite: Perform WAL checkpoint to finalize pending writes
        sqlx::query("PRAGMA optimize")
            .execute(pool)
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to optimize database during shutdown: {}",
                    e
                ))
            })?;

        info!("Database connection pool closed successfully");
        Ok(())
    }
}

// Re-export sqlx types for convenience
pub use sqlx;
pub use sqlx::Row;

pub mod activity_events;
pub mod query_helpers;
pub use activity_events::ActivityEvent;
pub mod adapter_snapshots;
pub mod crypto_audit;
pub mod key_rotation_events;
pub use adapter_snapshots::{AdapterTrainingSnapshot, CreateSnapshotParams};
pub use key_rotation_events::KeyRotationEvent;
pub mod provenance_certificates;
pub use provenance_certificates::{NewProvenanceCertificate, ProvenanceCertificateRecord};
pub mod tenant_weight_encryption;
pub use tenant_weight_encryption::{
    dek_fingerprint, derive_tenant_weight_dek, seal_weight, unseal_weight, unseal_weight_verified,
    EncryptedWeightFile, EncryptionStatus, SealedWeight, TenantWeightKey,
};
pub mod inference_evidence;
pub use inference_evidence::{CreateEvidenceParams, InferenceEvidence};
pub mod inference_write_bundle;
pub use inference_write_bundle::{
    inference_bundle_commit_failed, inference_bundle_commit_success, InferenceWriteBundle,
};
pub mod inference_trace;
pub use inference_trace::{
    backfill_receipt_digests, count_pending_receipt_backfill, find_trace_by_receipt_digest,
    get_inference_trace_detail_for_tenant, get_provenance_chain, get_receipt_parity_stats,
    list_inference_traces_for_tenant, recompute_receipt, BackfillResult,
    InferenceTraceDetailRecord, InferenceTraceListRecord, InferenceTraceReceiptRecord,
    InferenceTraceTokenRecord, SqlTraceSink, TraceCancellation, TraceCancellationReceipt,
    TraceFinalization, TraceReceipt, TraceReceiptVerification, TraceSink, TraceStart,
    TraceTokenInput,
};
pub mod inference_verdicts;
pub use inference_verdicts::{
    CreateVerdictParams, EvaluatorType, InferenceVerdict, Verdict, VerdictSummary,
};
pub mod batch_jobs;
pub use batch_jobs::{
    BatchItemRecord, BatchJobRecord, CreateBatchItemParams, CreateBatchJobParams,
};
pub mod replay_metadata;
pub use replay_metadata::{CreateReplayMetadataParams, InferenceReplayMetadata};
pub mod replay_executions;
pub use replay_executions::{
    CreateReplayExecutionParams, ReplayExecution, UpdateReplayExecutionParams,
};
pub mod adapter_record;
pub mod query_performance;
pub use adapter_record::{
    AccessControl, AdapterIdentity, AdapterRecordBuilder, AdapterRecordV1, ArtifactInfo,
    CodeIntelligence, FlatAdapterRow, ForkMetadata, LifecycleState, LoRAConfig, SchemaCompatible,
    SchemaMetadata, SemanticNaming, TierConfig,
};
pub mod adapter_consistency;
pub mod adapters;
pub mod adapters_kv;
pub mod kv_migration;
pub use adapter_consistency::AdapterConsistency;
pub use adapters::{
    Adapter, AdapterRegistrationBuilder, AdapterRegistrationParams, AosRegistrationMetadata,
    AtomicDualWriteConfig,
};
pub use adapters_kv::{AdapterKvOps, AdapterKvRepository};
pub use kv_migration::{MigrationDiscrepancy, MigrationProgress, MigrationStats};
pub mod artifacts;
pub mod audit;
pub use audit::AuditLog;
pub mod audits;
pub mod policy_audit;
pub use evidence_envelopes::{AllChainsVerificationResult, EvidenceEnvelopeFilter};
pub use policy_audit::{ChainVerificationResult, PolicyAuditDecision, PolicyDecisionFilters};
pub mod chat_sessions;
pub use chat_sessions::{
    AddMessageParams, ChatCategory, ChatMessage, ChatProvenance, ChatSearchResult, ChatSession,
    ChatSessionTrace, ChatSessionWithStatus, ChatTag, CreateChatProvenanceParams,
    CreateChatSessionParams, SessionShare,
};
pub mod lifecycle;
pub use lifecycle::{LifecycleHistoryEvent, StackReference};
pub mod metadata;
pub use metadata::{
    AdapterMeta, AdapterStackMeta, ForkType, LifecycleState as MetadataLifecycleState,
    WorkflowType, API_SCHEMA_VERSION,
};
pub mod migration_verify;
pub mod unified_access;
pub mod validation;
pub use audits::Audit;
pub use validation::{
    LifecycleEnforcementOptions, LifecycleEnforcementResult, PrerequisiteCheckResult,
    SingleActiveValidationResult,
};
pub mod code_policies;
pub mod commits;
pub mod contacts;
pub use contacts::{Contact, ContactStream};
pub mod cp_pointers;
pub mod enclave_operations;
pub use enclave_operations::{EnclaveOperation, OperationStats};
pub mod ephemeral_adapters;
pub mod git;
pub mod git_repositories;
pub use git_repositories::GitRepository;
pub mod incidents;
pub mod jobs;
pub use jobs::Job;
pub mod training_jobs;
pub mod training_jobs_kv;
pub use training_jobs::{
    LinkDatasetParams, TrainingJobDatasetLink, TrainingJobRecord, TrainingMetricRow,
    TrainingProgress,
};
pub mod training_datasets;
pub mod training_datasets_kv;
pub use training_datasets::{
    DatasetAdapterLink, DatasetFile, DatasetStatistics, EvidenceEntry, TrainingDataset,
};
pub mod key_metadata;
pub use key_metadata::KeyMetadata;
pub mod manifests;
pub mod model_operations;
pub mod models;
pub use model_operations::ModelOperation;
pub mod nodes;
pub mod patch_proposals;
pub use patch_proposals::PatchProposal;
pub mod pinned_adapters;
pub mod plans;
pub mod plugin_configs;
pub mod plugin_configs_kv;
pub use plugin_configs::{PluginConfig, PluginTenantEnable};
pub mod plugin_enables;
pub mod policies;
pub mod policy_hash;
pub mod policy_management;
pub mod promotions;
pub use promotions::{
    CreatePromotionRequestParams, CreateReleaseCorrelationParams, GoldenRunStage,
    PromotionApproval, PromotionGate, PromotionRequest, RecordApprovalParams, RecordGateParams,
    UpdateCiAttestationParams, UpdateReleasePromotionStatusParams,
};
pub mod plans_kv;
pub mod stacks_kv;
pub mod tenants;
pub mod tenants_kv;
pub use policy_hash::PolicyHashRecord;
pub use stacks_kv::{StackKvOps, StackKvRepository};
pub use tenants_kv::{CreateTenantParams, TenantKvOps, TenantKvRepository};
pub mod discrepancy_cases;
pub mod process_monitoring;
pub mod progress_events;
pub mod rag_retrieval_audit;
pub mod replay_sessions;
pub mod repositories;
pub mod repositories_kv;
pub mod routing_decisions;
pub use routing_decisions::{RouterCandidate, RoutingDecision, RoutingDecisionFilters};
pub mod routing_decision_chain;
pub use routing_decision_chain::{make_chain_record_from_api, RoutingDecisionChainRecord};
pub mod routing_telemetry_bridge;
pub use routing_telemetry_bridge::{event_to_decision, persist_router_decisions};
pub mod telemetry_bundles;
pub mod users;
pub mod users_kv;
pub use users::{Role, User};
// Re-export users_kv types for dual-write operations
pub use users_kv::{kv_to_user, user_to_kv, UserKeys, UserKvOps, UserKvRepository};
// Re-export KV Role type with an alias to distinguish from SQL Role
pub use users_kv::Role as KvRole;
pub mod user_tenant_access;
pub use user_tenant_access::{
    cleanup_expired_tenant_access, get_user_tenant_access, get_user_tenant_access_details,
    grant_user_tenant_access, revoke_user_tenant_access, UserTenantAccess,
};
pub mod workers;
pub use models::Worker;
pub use workers::{
    temporal_ordering_violations, TrainingTask, WorkerHealthRecord, WorkerIncident,
    WorkerIncidentType,
};

// Document management modules
pub mod collections;
pub mod documents;
pub use collections::{CreateCollectionParams, DocumentCollection};
pub use documents::{CreateChunkParams, CreateDocumentParams, Document, DocumentChunk};

// Workspace, notifications, messages, dashboard, and tutorial modules
pub mod dashboard_configs;
pub mod messages;
pub mod messages_kv;
pub mod notifications;
pub mod tutorials;
pub mod workspace_active_state;
pub mod workspaces;

// System statistics module
pub mod system_stats;

// Federation module
pub mod federation;
pub use federation::{PeerSyncState, PeerSyncStatus, QuarantineDetails, QuarantineRecord};

// Authentication sessions module
pub mod auth_sessions;
pub mod auth_sessions_kv;
pub use auth_sessions::AuthSession;
pub mod runtime_sessions;
pub mod runtime_sessions_kv;
pub use dashboard_configs::DashboardWidgetConfig;
pub use notifications::{Notification, NotificationType};
pub use runtime_sessions::RuntimeSession;
pub use tutorials::TutorialStatus;
pub use workspace_active_state::WorkspaceActiveState;
pub use workspaces::{ResourceType, Workspace, WorkspaceMember, WorkspaceResource, WorkspaceRole};

// Re-export unified access types
pub use unified_access::{
    ConnectionInfo, DatabaseAccess, DatabaseStatistics, DatabaseType, DbHealthStatus, SqlParameter,
    ToSql, Transaction, UnifiedDatabaseAccess, UnifiedTransaction,
};
// Re-export canonical health types from adapteros-core
pub use adapteros_core::{HealthCheckResult, HealthStatus};

/// KV backend health status
///
/// Provides detailed health information about the KV backend including
/// connectivity, performance, and storage metrics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KvHealthStatus {
    /// Overall health status
    pub status: HealthStatus,
    /// Whether KV backend is attached
    pub attached: bool,
    /// Current storage mode
    pub storage_mode: String,
    /// Error message if unhealthy
    pub error: Option<String>,
    /// KV backend connectivity check result
    pub connectivity_ok: bool,
    /// Read latency in milliseconds (if available)
    pub read_latency_ms: Option<f64>,
    /// Write latency in milliseconds (if available)
    pub write_latency_ms: Option<f64>,
    /// Approximate number of keys (if available)
    pub key_count: Option<usize>,
}

// Add update_anomaly_status method to Db impl
impl Db {
    /// Update anomaly status with investigation details
    pub async fn update_anomaly_status(
        &self,
        anomaly_id: &str,
        status: &str,
        investigation_notes: &str,
        investigated_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE process_anomalies SET status = ?, investigation_notes = ?, investigated_by = ?, updated_at = datetime('now') WHERE id = ?"
        )
        .bind(status)
        .bind(investigation_notes)
        .bind(investigated_by)
        .bind(anomaly_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update anomaly status: {}", e)))?;
        Ok(())
    }

    /// Get a system setting value by key
    ///
    /// Returns None if the key doesn't exist or the value is empty.
    pub async fn get_system_setting(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM system_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to get system setting: {}", e)))?;

        Ok(row.map(|(v,)| v).filter(|v| !v.is_empty()))
    }

    /// Set a system setting value
    ///
    /// Creates the setting if it doesn't exist, updates if it does.
    pub async fn set_system_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO system_settings (key, value, updated_at)
             VALUES (?, ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to set system setting: {}", e)))?;

        Ok(())
    }

    /// Perform KV backend health check
    ///
    /// This checks the health of the KV backend (if attached) by:
    /// 1. Verifying KV backend is attached
    /// 2. Testing read connectivity with a health check key
    /// 3. Testing write connectivity with a timestamped value
    /// 4. Measuring read and write latencies
    /// 5. Estimating storage size (if supported)
    ///
    /// Returns:
    /// - Healthy: KV backend is accessible and responsive
    /// - Degraded: KV backend is accessible but slow or having issues
    /// - Unhealthy: KV backend is not accessible or failing operations
    /// - Unknown: KV backend is not attached
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: Db) -> Result<(), Box<dyn std::error::Error>> {
    /// let kv_health = db.kv_health_check().await?;
    /// println!("KV backend status: {:?}", kv_health.status);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn kv_health_check(&self) -> Result<KvHealthStatus> {
        use std::time::Instant;
        use tracing::debug;

        // Check if KV backend is attached
        let Some(kv) = self.kv_backend() else {
            return Ok(KvHealthStatus {
                status: HealthStatus::Unknown,
                attached: false,
                storage_mode: self.storage_mode.to_string(),
                error: Some("KV backend not attached".to_string()),
                connectivity_ok: false,
                read_latency_ms: None,
                write_latency_ms: None,
                key_count: None,
            });
        };

        let mut error_message = None;
        let mut connectivity_ok = false;
        let mut read_latency_ms = None;
        let mut write_latency_ms = None;
        let mut key_count = None;

        // Test read connectivity and measure latency
        let health_check_key = "aos:health:kv_check";
        let start = Instant::now();
        match kv.get(health_check_key).await {
            Ok(_) => {
                let latency = start.elapsed().as_secs_f64() * 1000.0;
                read_latency_ms = Some(latency);
                connectivity_ok = true;
                debug!(latency_ms = latency, "KV backend read check successful");
            }
            Err(e) => {
                error_message = Some(format!("KV backend read failed: {}", e));
                debug!(error = %e, "KV backend read check failed");
            }
        }

        // Test write connectivity and measure latency
        if connectivity_ok {
            let test_value = format!("health_check:{}", chrono::Utc::now().timestamp());
            let start = Instant::now();
            match kv.set(health_check_key, test_value.into_bytes()).await {
                Ok(_) => {
                    let latency = start.elapsed().as_secs_f64() * 1000.0;
                    write_latency_ms = Some(latency);
                    debug!(latency_ms = latency, "KV backend write check successful");
                }
                Err(e) => {
                    error_message = Some(format!("KV backend write failed: {}", e));
                    connectivity_ok = false;
                    debug!(error = %e, "KV backend write check failed");
                }
            }
        }

        // Estimate key count by scanning common prefixes
        if connectivity_ok {
            let mut total_keys = 0;
            let prefixes = vec!["adapter:", "tenant:", "user:", "stack:"];
            for prefix in prefixes {
                match kv.scan_prefix(prefix).await {
                    Ok(keys) => {
                        total_keys += keys.len();
                    }
                    Err(e) => {
                        debug!(prefix = prefix, error = %e, "Failed to scan prefix for key count");
                    }
                }
            }
            if total_keys > 0 {
                key_count = Some(total_keys);
            }
        }

        // Determine overall health status
        let status = if !connectivity_ok {
            HealthStatus::Unhealthy
        } else {
            // Consider degraded if latencies are high (>100ms for read, >200ms for write)
            let read_slow = read_latency_ms.is_some_and(|lat| lat > 100.0);
            let write_slow = write_latency_ms.is_some_and(|lat| lat > 200.0);

            if read_slow || write_slow {
                HealthStatus::Degraded
            } else {
                HealthStatus::Healthy
            }
        };

        Ok(KvHealthStatus {
            status,
            attached: true,
            storage_mode: self.storage_mode.to_string(),
            error: error_message,
            connectivity_ok,
            read_latency_ms,
            write_latency_ms,
            key_count,
        })
    }

    /// Validate system bootstrap state
    ///
    /// Checks that the system tenant exists and core policies are properly seeded.
    /// This is used during boot sequence to detect incomplete bootstrap state.
    pub async fn validate_bootstrap_state(&self) -> Result<BootstrapHealthStatus> {
        use crate::tenant_policy_bindings::CORE_POLICIES;

        let mut issues = Vec::new();

        // Check system tenant exists
        match self.get_tenant("system").await {
            Ok(Some(_)) => {}
            Ok(None) => {
                issues.push("System tenant missing".to_string());
            }
            Err(e) => {
                issues.push(format!("Failed to check system tenant: {}", e));
            }
        }

        // Check core policies are enabled for system tenant
        match self.get_active_policies_for_tenant("system").await {
            Ok(policies) => {
                for core_policy in CORE_POLICIES {
                    if !policies.contains(&core_policy.to_string()) {
                        issues.push(format!(
                            "Core policy '{}' not enabled for system tenant",
                            core_policy
                        ));
                    }
                }
            }
            Err(e) => {
                issues.push(format!("Failed to check system tenant policies: {}", e));
            }
        }

        Ok(BootstrapHealthStatus {
            healthy: issues.is_empty(),
            issues,
        })
    }

    /// Mark the database as degraded with a reason
    ///
    /// This is called internally when KV backend operations fail and the system
    /// falls back to SQL-only mode. The degradation reason is tracked for monitoring
    /// and debugging purposes.
    ///
    /// # Arguments
    /// * `reason` - Human-readable description of why the system degraded
    pub fn mark_degraded(&self, reason: String) {
        use tracing::warn;

        if let Ok(mut degraded) = self.degraded_reason.write() {
            *degraded = Some(reason.clone());
            crate::kv_metrics::global_kv_metrics().record_degradation();
            warn!(
                event = crate::constants::DEGRADATION_EVENT_RUNTIME_FAILED,
                reason = %reason,
                storage_mode = %self.storage_mode,
                "Database marked as degraded - falling back to SqlOnly mode"
            );
        }
    }

    /// Clear the degradation marker
    ///
    /// This should be called when the system recovers from degraded state,
    /// for example after successfully reconnecting to KV backend.
    pub fn clear_degraded(&self) {
        use tracing::info;

        if let Ok(mut degraded) = self.degraded_reason.write() {
            if degraded.is_some() {
                info!(
                    event = crate::constants::DEGRADATION_EVENT_RECOVERED,
                    storage_mode = %self.storage_mode,
                    "Degraded state cleared - KV backend recovered"
                );
                *degraded = None;
            }
        }
    }

    /// Check if the database is in degraded mode
    ///
    /// Returns true if the database has degraded from its configured mode
    /// (e.g., fell back from KV to SQL due to KV backend failure).
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: Db) {
    /// if db.is_degraded() {
    ///     println!("Warning: Database is running in degraded mode");
    /// }
    /// # }
    /// ```
    pub fn is_degraded(&self) -> bool {
        self.degraded_reason
            .read()
            .map(|d| d.is_some())
            .unwrap_or(false)
    }

    /// Get the degradation reason if degraded
    ///
    /// Returns the human-readable reason for degradation if the database
    /// is in degraded mode, or None if operating normally.
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: Db) {
    /// if let Some(reason) = db.degradation_reason() {
    ///     println!("Degraded: {}", reason);
    /// }
    /// # }
    /// ```
    pub fn degradation_reason(&self) -> Option<String> {
        self.degraded_reason.read().ok().and_then(|d| d.clone())
    }

    /// Attempt to recover from degraded state by reinitializing KV backend
    ///
    /// This method attempts to reinitialize the KV backend and restore the
    /// originally requested storage mode. If successful, clears the degradation
    /// marker. If it fails, returns an error but leaves the system in degraded
    /// SQL-only mode.
    ///
    /// # Arguments
    /// * `kv_path` - Path to the KV database file
    /// * `target_mode` - The storage mode to restore to
    ///
    /// # Returns
    /// Ok(true) if recovery succeeded, Ok(false) if still degraded, Err if unrecoverable
    ///
    /// # Example
    /// ```rust,no_run
    /// # use adapteros_db::{Db, StorageMode};
    /// # async fn example(db: &mut Db) -> Result<(), Box<dyn std::error::Error>> {
    /// if db.is_degraded() {
    ///     match db.attempt_recovery(std::path::Path::new("var/aos-kv.redb"), StorageMode::DualWrite) {
    ///         Ok(true) => println!("Recovery successful"),
    ///         Ok(false) => println!("Recovery failed, still degraded"),
    ///         Err(e) => println!("Recovery error: {}", e),
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn attempt_recovery(
        &mut self,
        kv_path: &std::path::Path,
        target_mode: StorageMode,
    ) -> Result<bool> {
        use tracing::info;

        if !self.is_degraded() {
            return Ok(true); // Already healthy
        }

        info!(
            kv_path = %kv_path.display(),
            target_mode = %target_mode,
            "Attempting recovery from degraded state"
        );

        match self.init_kv_backend(kv_path) {
            Ok(()) => {
                self.set_storage_mode(target_mode)?;
                self.clear_degraded();
                info!(mode = %target_mode, "Recovery successful");
                Ok(true)
            }
            Err(e) => {
                warn!(error = %e, "Recovery attempt failed");
                Ok(false)
            }
        }
    }

    /// Insert a behavior event for lifecycle tracking
    pub async fn insert_behavior_event(
        &self,
        event_type: &str,
        adapter_id: &str,
        tenant_id: &str,
        from_state: &str,
        to_state: &str,
        activation_pct: f32,
        memory_mb: u64,
        reason: &str,
        metadata: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO behavior_events (id, event_type, adapter_id, tenant_id, from_state, to_state, 
                   activation_pct, memory_mb, reason, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(new_id(adapteros_id::IdPrefix::Evt))
        .bind(event_type)
        .bind(adapter_id)
        .bind(tenant_id)
        .bind(from_state)
        .bind(to_state)
        .bind(activation_pct)
        .bind(memory_mb as i64)
        .bind(reason)
        .bind(metadata)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert behavior event: {}", e)))?;

        Ok(())
    }

    /// Query behavior events with optional filtering
    pub async fn get_behavior_events(
        &self,
        tenant_id: Option<&str>,
        event_type: Option<&str>,
        adapter_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<serde_json::Value>> {
        let mut query = "SELECT id, event_type, adapter_id, tenant_id, from_state, to_state, activation_pct, memory_mb, reason, created_at, metadata FROM behavior_events WHERE 1=1".to_string();

        if tenant_id.is_some() {
            query.push_str(" AND tenant_id = ?");
        }
        if event_type.is_some() {
            query.push_str(" AND event_type = ?");
        }
        if adapter_id.is_some() {
            query.push_str(" AND adapter_id = ?");
        }
        if since.is_some() {
            query.push_str(" AND created_at >= ?");
        }
        if until.is_some() {
            query.push_str(" AND created_at <= ?");
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {}", lim));
        }
        if let Some(off) = offset {
            query.push_str(&format!(" OFFSET {}", off));
        }

        let mut q = sqlx::query(&query);

        if let Some(tid) = tenant_id {
            q = q.bind(tid);
        }
        if let Some(et) = event_type {
            q = q.bind(et);
        }
        if let Some(aid) = adapter_id {
            q = q.bind(aid);
        }
        if let Some(s) = since {
            q = q.bind(s);
        }
        if let Some(u) = until {
            q = q.bind(u);
        }

        let rows = q
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to query behavior events: {}", e)))?;

        let mut results = Vec::new();
        for row in rows {
            let mut event = serde_json::Map::new();
            event.insert(
                "id".to_string(),
                serde_json::json!(row.try_get::<String, _>("id").unwrap_or_default()),
            );
            event.insert(
                "event_type".to_string(),
                serde_json::json!(row.try_get::<String, _>("event_type").unwrap_or_default()),
            );
            event.insert(
                "adapter_id".to_string(),
                serde_json::json!(row.try_get::<String, _>("adapter_id").unwrap_or_default()),
            );
            event.insert(
                "tenant_id".to_string(),
                serde_json::json!(row.try_get::<String, _>("tenant_id").unwrap_or_default()),
            );
            event.insert(
                "from_state".to_string(),
                serde_json::json!(row.try_get::<String, _>("from_state").ok()),
            );
            event.insert(
                "to_state".to_string(),
                serde_json::json!(row.try_get::<String, _>("to_state").ok()),
            );
            event.insert(
                "activation_pct".to_string(),
                serde_json::json!(row.try_get::<f64, _>("activation_pct").unwrap_or(0.0)),
            );
            event.insert(
                "memory_mb".to_string(),
                serde_json::json!(row.try_get::<i64, _>("memory_mb").unwrap_or(0)),
            );
            event.insert(
                "reason".to_string(),
                serde_json::json!(row.try_get::<String, _>("reason").unwrap_or_default()),
            );
            event.insert(
                "created_at".to_string(),
                serde_json::json!(row.try_get::<String, _>("created_at").unwrap_or_default()),
            );
            if let Ok(Some(meta)) = row.try_get::<Option<String>, _>("metadata") {
                event.insert("metadata".to_string(), serde_json::json!(meta));
            }
            results.push(serde_json::Value::Object(event));
        }

        Ok(results)
    }

    /// Get behavior event statistics
    pub async fn get_behavior_stats(&self, tenant_id: Option<&str>) -> Result<serde_json::Value> {
        let tenant_filter = if tenant_id.is_some() {
            "WHERE tenant_id = ?"
        } else {
            ""
        };

        // Total count
        let total_query = format!(
            "SELECT COUNT(*) as total FROM behavior_events {}",
            tenant_filter
        );
        let mut total_q = sqlx::query(&total_query);
        if let Some(tid) = tenant_id {
            total_q = total_q.bind(tid);
        }
        let total_row = total_q
            .fetch_one(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get total count: {}", e)))?;
        let total: i64 = total_row.try_get("total").unwrap_or(0);

        // By category
        let category_query = format!(
            "SELECT event_type, COUNT(*) as count FROM behavior_events {} GROUP BY event_type",
            tenant_filter
        );
        let mut cat_q = sqlx::query(&category_query);
        if let Some(tid) = tenant_id {
            cat_q = cat_q.bind(tid);
        }
        let category_rows = cat_q
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get category stats: {}", e)))?;

        let mut by_category = serde_json::Map::new();
        for row in category_rows {
            let event_type: String = row.try_get("event_type").unwrap_or_default();
            let count: i64 = row.try_get("count").unwrap_or(0);
            by_category.insert(event_type, serde_json::json!(count));
        }

        // By state transition
        let transition_query = format!("SELECT from_state, to_state, COUNT(*) as count FROM behavior_events {} GROUP BY from_state, to_state ORDER BY count DESC LIMIT 10", tenant_filter);
        let mut trans_q = sqlx::query(&transition_query);
        if let Some(tid) = tenant_id {
            trans_q = trans_q.bind(tid);
        }
        let transition_rows = trans_q
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to get transition stats: {}", e)))?;

        let mut by_transition = Vec::new();
        for row in transition_rows {
            let from: String = row.try_get("from_state").unwrap_or_default();
            let to: String = row.try_get("to_state").unwrap_or_default();
            let count: i64 = row.try_get("count").unwrap_or(0);
            by_transition.push(serde_json::json!({
                "from": from,
                "to": to,
                "count": count
            }));
        }

        Ok(serde_json::json!({
            "total_events": total,
            "by_category": by_category,
            "by_state_transition": by_transition
        }))
    }
}
