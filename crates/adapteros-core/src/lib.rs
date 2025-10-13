//! AdapterOS Core Types
//!
//! Foundational types and utilities for the AdapterOS system.
//!
//! This crate provides:
//! - Error handling with [`AosError`] and [`Result`]
//! - Cryptographic hashing with [`B3Hash`] (BLAKE3)
//! - Checkpoint IDs with [`CPID`]
//! - Deterministic seed derivation for RNG
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::{B3Hash, CPID, derive_seed};
//!
//! // Hash some data
//! let hash = B3Hash::hash(b"hello world");
//! println!("Hash: {}", hash.to_hex());
//!
//! // Derive a checkpoint ID
//! let cpid = CPID::from_hash(&hash);
//! println!("CPID: {}", cpid);
//!
//! // Derive deterministic seeds
//! let seed = derive_seed(&hash, "component_a");
//! ```

pub mod error;
pub mod hash;
pub mod id;
pub mod seed;

pub use error::{AosError, Result};
pub use hash::B3Hash;
pub use id::CPID;
pub use seed::{derive_seed, derive_seed_indexed};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::{AosError, B3Hash, Result, CPID};
}
