//! Key-value storage infrastructure

pub mod backend;
pub mod indexing;

pub use backend::{BatchOp, KvBackend, KvBatch};
pub use indexing::IndexManager;
