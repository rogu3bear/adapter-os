// Adapter Handlers Module
//
// This module provides REST API endpoints for adapter management including:
// - Lifecycle management (activation, promotion, demotion)
// - Lineage and detail views
// - Strength configuration
// - Pinning (eviction protection)
// - Hot-swapping
// - Statistics
// - Category policies
// - Import/Export
// - Training snapshots and provenance
// - Archive/Unarchive
// - Duplication

// Re-export read handlers (list_adapters, get_adapter, etc.)
pub use super::adapters_read::*;

// ============================================================================
// Existing Utility Submodules (keep these with existing naming)
// ============================================================================

pub(crate) mod fs_utils;
pub(crate) mod hashing;
pub(crate) mod paths;
pub(crate) mod progress;
pub(crate) mod repo;
pub(crate) mod tenant;

// ============================================================================
// NEW Handler Submodules
// ============================================================================

mod archive;
mod category_policies;
mod duplicate;
mod export;
mod import;
mod lifecycle;
mod lineage;
mod pinning;
pub(crate) mod preflight_adapter;
mod stats;
mod strength;
mod swap;
mod training_snapshots;
mod version_archive;

// ============================================================================
// Re-export all handler functions for API compatibility
// ============================================================================

// Lifecycle handlers
pub use lifecycle::{
    __path_activate_adapter, __path_demote_adapter_lifecycle, __path_promote_adapter_lifecycle,
    activate_adapter, demote_adapter_lifecycle, promote_adapter_lifecycle,
    AdapterActivateRequest, LifecycleTransitionRequest, LifecycleTransitionResponse,
};

// Lineage handlers
pub use lineage::{
    __path_get_adapter_detail, __path_get_adapter_lineage, get_adapter_detail,
    get_adapter_lineage, AdapterDetailResponse, AdapterLineageResponse, LineageNode,
};

// Strength handlers
pub use strength::{
    __path_update_adapter_strength, update_adapter_strength, UpdateAdapterStrengthRequest,
};

// Pinning handlers
pub use pinning::{
    __path_get_pin_status, __path_pin_adapter, __path_unpin_adapter, get_pin_status, pin_adapter,
    unpin_adapter, PinAdapterRequest, PinAdapterResponse, PinStatusResponse, UnpinAdapterResponse,
};

// Swap handlers
pub use swap::{__path_swap_adapters, swap_adapters};

// Stats handlers
pub use stats::{__path_get_adapter_stats, get_adapter_stats};

// Category policies handlers
pub use category_policies::{
    __path_get_category_policy, __path_list_category_policies, __path_update_category_policy,
    get_category_policy, list_category_policies, update_category_policy,
};

// Import handlers
pub use import::{__path_import_adapter, import_adapter};

// Training snapshots handlers
pub use training_snapshots::{
    __path_export_training_provenance, __path_get_adapter_training_snapshot,
    export_training_provenance, get_adapter_training_snapshot,
};

// Archive handlers
pub use archive::{
    __path_archive_adapter, __path_get_archive_status, __path_unarchive_adapter, archive_adapter,
    get_archive_status, unarchive_adapter, ArchiveAdapterRequest, ArchiveAdapterResponse,
    ArchiveStatusResponse, UnarchiveAdapterResponse,
};

// Export handlers
pub use export::{__path_export_adapter, export_adapter};

// Version archive handlers
pub use version_archive::{
    __path_archive_adapter_version, __path_unarchive_adapter_version, archive_adapter_version,
    unarchive_adapter_version,
};

// Duplicate handlers
pub use duplicate::{__path_duplicate_adapter, duplicate_adapter, DuplicateAdapterRequest};

// ============================================================================
// Re-export adapter functions from parent handlers module for routes.rs
// ============================================================================

// Note: Some functions have moved to submodules:
// - adapter_lifecycle: promote_adapter_state
// - adapter_health: get_adapter_activations, get_adapter_health, verify_gpu_integrity
// - adapter_versions: get_adapter_version, list_adapter_versions
pub use super::adapter_health::{
    __path_get_adapter_activations, __path_get_adapter_health, __path_verify_gpu_integrity,
    get_adapter_activations, get_adapter_health, verify_gpu_integrity,
};
pub use super::adapter_lifecycle::{__path_promote_adapter_state, promote_adapter_state};
pub use super::adapter_versions::{
    __path_get_adapter_version, __path_list_adapter_versions, get_adapter_version,
    list_adapter_versions,
};
pub use super::{
    __path_get_adapter, __path_get_adapter_metrics, __path_get_commit, __path_get_commit_diff,
    __path_get_quality_metrics, __path_get_system_metrics, __path_list_adapters,
    __path_list_commits, get_adapter, get_adapter_metrics, get_adapter_repository,
    get_adapter_repository_policy, get_commit, get_commit_diff, get_quality_metrics,
    get_system_metrics, list_adapter_repositories, list_adapters, list_commits,
};
