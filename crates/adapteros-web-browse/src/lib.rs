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
mod retry;
mod service;
mod streaming;

pub mod providers;

// Re-exports
pub use cache::{CacheConfig, WebBrowseCache};
pub use config::{TenantBrowseConfig, WebBrowseConfig};
pub use error::{is_retriable_status, WebBrowseError, WebBrowseResult};
pub use evidence::{BrowseEvidence, EvidenceBuilder};
pub use rate_limit::{RateLimitConfig, RateLimiter};
pub use retry::HttpRetryConfig;
pub use service::{
    DefaultWebBrowseService, ImageSearchRequest, ImageSearchResponse, ImageSearchResult,
    PageFetchRequest, PageFetchResponse, PageImage, RateLimitStatus, UsageStats, WebBrowseService,
    WebSearchRequest, WebSearchResponse, WebSearchResult,
};
pub use streaming::{StreamedContent, StreamingConfig};

/// Tenant ID type alias
pub type TenantId = String;

/// Request ID for tracing
pub type RequestId = String;
