//! Web browse providers
//!
//! Implementations for various search and content providers.

pub mod fetch;
pub mod search;

pub use fetch::{PageFetcher, PageFetcherConfig};
pub use search::{BraveSearchProvider, SearchProvider, SearchProviderConfig};
