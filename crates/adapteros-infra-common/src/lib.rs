//! Foundational infrastructure and common utilities for AdapterOS
//!
//! This crate provides central, side-effect-free logic used across the system:
//! - [`B3Hash`]: BLAKE3 hashing utilities
//! - [`normalization`]: Deterministic repository and path normalization
//! - [`constants`]: System-wide mathematical and unit constants
//! - [`vector_math`]: Basic vector operations for embeddings

pub mod constants;
pub mod error;
pub mod hash;
pub mod id;
pub mod invariants;
pub mod normalization;
pub mod vector_math;

pub use constants::*;
pub use error::{AosError, Result};
pub use hash::B3Hash;
pub use id::CPID;
pub use invariants::*;
pub use normalization::*;
pub use vector_math::*;
