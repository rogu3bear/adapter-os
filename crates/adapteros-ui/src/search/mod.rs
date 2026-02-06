//! Search infrastructure
//!
//! Client-side fuzzy search for pages, entities, and actions.
//! Supports Command Palette (Ctrl+K) and Global Search.

pub mod contextual;
pub mod fuzzy;
pub mod index;
pub mod providers;
pub mod recent;
pub mod types;

pub use contextual::{contextual_result_matches, generate_contextual_actions};
pub use fuzzy::fuzzy_score;
pub use index::{get_pages, PageDefinition, SearchIndex};
pub use providers::EntityCache;
pub use recent::{RecentItem, RecentItemType, RecentItemsManager};
pub use types::{group_results, SearchAction, SearchResult, SearchResultType};
