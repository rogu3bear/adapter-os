#![cfg(all(test, feature = "extended-tests"))]

//! Comprehensive determinism verification tests for adapterOS
//!
//! This module provides specialized verification utilities and tests for:
//! - Cross-run consistency verification
//! - Platform validation
//! - Hash chain validation
//! - Event sequence determinism
//! - HKDF seeding verification
//! - Canonical hashing verification
//! - Evidence-grounded response verification

pub mod cross_run;
pub mod platform_validation;
pub mod hash_chain;
pub mod event_sequence;
pub mod hkdf_seeding;
pub mod canonical_hashing;
pub mod evidence_grounded;
pub mod utils;

// Re-export key utilities for easy access
pub use utils::*;
