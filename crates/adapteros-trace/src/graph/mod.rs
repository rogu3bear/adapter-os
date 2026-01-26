//! adapterOS Graph - Tensor metadata canonicalization and hash graph
//!
//! This crate provides deterministic tensor metadata canonicalization for
//! reproducible hash graphs across hardware, drivers, and compiler revisions.

pub mod canonical;
pub mod hash;
pub mod tensor;

pub use canonical::*;
pub use hash::*;
pub use tensor::*;
