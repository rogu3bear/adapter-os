//! Model management endpoints for adapteros server
//!
//! This crate contains model-related API endpoints that will be
//! migrated from adapteros-server-api. It provides handlers and routes
//! for managing base models, model registry, and related operations.

pub mod handlers;
pub mod routes;

pub use routes::models_routes;
