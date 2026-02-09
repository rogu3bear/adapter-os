//! Telemetry invariant verifier.
//!
//! This crate is a compatibility wrapper around the canonical verifier implementation in
//! `adapteros-telemetry`. Keeping this crate allows downstream users to depend on the historical
//! crate name without duplicating logic.

pub use adapteros_telemetry::verifier::{
    default_invariants, run_verifier, run_with_specs, workspace_root, InvariantReport,
    InvariantSpec, VerifierReport,
};

