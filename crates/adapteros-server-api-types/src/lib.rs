//! Shared types for adapteros-server-api crates
//!
//! This crate provides common types used across the split server-api modules:
//! - Error types and error handling utilities
//! - Response wrappers for consistent API responses
//! - Pagination types for list endpoints

pub mod error;
pub mod pagination;
pub mod response;

// Re-exports for convenience
pub use error::ApiError;
pub use pagination::{PageInfo, PaginatedResponse, PaginationParams};
pub use response::{ApiResponse, EmptyResponse};
