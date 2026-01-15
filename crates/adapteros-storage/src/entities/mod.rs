//! Entity KV schemas for adapterOS storage migration
//!
//! This module defines the canonical entity types that replace SQL tables
//! in the key-value storage backend. Each entity type includes conversion
//! implementations from existing SQL types to ensure seamless migration.

pub mod adapter;
pub mod stack;
pub mod tenant;
pub mod user;

pub use adapter::AdapterKv;
pub use stack::AdapterStackKv;
pub use tenant::TenantKv;
pub use user::{Role, RoleParseError, UserKv};
