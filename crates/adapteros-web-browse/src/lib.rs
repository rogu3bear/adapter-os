//! Web Browse Service for AdapterOS
//!
//! Provides controlled web browsing capabilities for live data retrieval,
//! enabling AI responses to be grounded in current information.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    WebBrowseService                             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
//! │  │   Search    │  │  PageFetch  │  │   Image     │             │
//! │  │  Provider   │  │   Provider  │  │   Search    │             │
//! │  └─────────────┘  └─────────────┘  └─────────────┘             │
//! │         │                │                │                     │
//! │         └────────────────┼────────────────┘                     │
//! │                          │                                      │
//! │                   ┌──────┴──────┐                               │
//! │                   │    Cache    │  (L1: moka, L2: DB)           │
//! │                   └─────────────┘                               │
//! │                          │                                      │
//! │                   ┌──────┴──────┐                               │
//! │                   │ Rate Limiter│  (per-tenant)                 │
//! │                   └─────────────┘                               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Security
//!
//! - Runs in isolated process with restricted egress
//! - Input sanitization for all queries
//! - Domain allowlist/blocklist enforcement
//! - Rate limiting per tenant

mod cache;
mod config;
mod error;
mod evidence;
mod rate_limit;
mod service;

pub mod providers;

// Re-exports
pub use cache::{CacheConfig, WebBrowseCache};
pub use config::{TenantBrowseConfig, WebBrowseConfig};
pub use error::{WebBrowseError, WebBrowseResult};
pub use evidence::{BrowseEvidence, EvidenceBuilder};
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use service::{
    ImageSearchRequest, ImageSearchResponse, ImageSearchResult, PageFetchRequest,
    PageFetchResponse, WebBrowseService, WebSearchRequest, WebSearchResponse, WebSearchResult,
};

/// Tenant ID type alias
pub type TenantId = String;

/// Request ID for tracing
pub type RequestId = String;
