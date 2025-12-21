//! Registry compatibility layer for adapteros-db
//!
//! This module provides functionality that was previously in `adapteros-registry`,
//! consolidated into the main database layer for unified SQLx access.
//!
//! ## Modules
//!
//! - `sync_wrapper` - Synchronous API wrapper for CLI compatibility
//! - `acl` - ACL inheritance and resolution
//! - `lineage` - Lineage validation and revision monotonicity
//! - `model_hash` - Model hash verification and collision detection
//! - `eviction` - Async adapter eviction with secure memory zeroization

pub mod acl;
pub mod eviction;
pub mod lineage;
pub mod model_hash;
pub mod sync_wrapper;

// Re-export commonly used types
pub use acl::AclResolver;
pub use eviction::{EvictionManager, EvictionOrder, EvictionStats, ZeroizationPolicy};
pub use lineage::LineageValidator;
pub use model_hash::{ModelHashVerifier, ModelRecord, ModelRecordInput};
pub use sync_wrapper::{
    SyncModelRecord, SyncModelRecordInput, SyncRegistry, SyncRegistryAdapterRecord,
    SyncRegistryTenantRecord,
};
