//! Inference handler re-exports.
//!
//! This crate exposes the production inference handlers from `adapteros-server-api`
//! so route wiring remains consistent with the control plane implementation.

pub use adapteros_server_api::handlers::batch::{
    batch_infer, create_batch_job, get_batch_items, get_batch_status,
};
pub use adapteros_server_api::handlers::inference::{get_inference_provenance, infer};
pub use adapteros_server_api::handlers::streaming_infer::{
    streaming_infer, streaming_infer_with_progress,
};
