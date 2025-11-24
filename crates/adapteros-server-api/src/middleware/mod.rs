//! Middleware modules for AdapterOS API
//!
//! Provides cross-cutting concerns:
//! - API versioning and deprecation
//! - Request ID tracking
//! - Compression
//! - Caching (ETags, conditional requests)

pub mod versioning;
pub mod request_id;
pub mod compression;
pub mod caching;

pub use versioning::{versioning_middleware, ApiVersion, DeprecationInfo};
pub use request_id::request_id_middleware;
pub use compression::compression_middleware;
pub use caching::{caching_middleware, CacheControl};
