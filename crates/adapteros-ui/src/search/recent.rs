//! Recent items management
//!
//! Tracks recently accessed pages and entities with localStorage persistence.

use serde::{Deserialize, Serialize};

#[allow(dead_code)]
const RECENT_ITEMS_KEY: &str = "adapteros_recent_items";
#[allow(dead_code)]
const MAX_RECENT_ITEMS: usize = 20;

fn canonicalize_recent_path(path: &str) -> String {
    match path {
        "/dashboard" => "/".to_string(),
        _ => path.to_string(),
    }
}

/// Type of recent item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecentItemType {
    /// A navigation page
    Page,
    /// An adapter entity
    Adapter,
    /// A model entity
    Model,
    /// A worker entity
    Worker,
    /// An action/command
    Action,
}

impl RecentItemType {
    /// Icon SVG path for this type
    pub fn icon_path(&self) -> &'static str {
        match self {
            Self::Page => "M3 12l2-2m0 0l7-7 7 7M5 10v10a1 1 0 001 1h3m10-11l2 2m-2-2v10a1 1 0 01-1 1h-3m-6 0a1 1 0 001-1v-4a1 1 0 011-1h2a1 1 0 011 1v4a1 1 0 001 1m-6 0h6",
            Self::Adapter => "M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10",
            Self::Model => "M9 3v2m6-2v2M9 19v2m6-2v2M5 9H3m2 6H3m18-6h-2m2 6h-2M7 19h10a2 2 0 002-2V7a2 2 0 00-2-2H7a2 2 0 00-2 2v10a2 2 0 002 2zM9 9h6v6H9V9z",
            Self::Worker => "M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01",
            Self::Action => "M13 10V3L4 14h7v7l9-11h-7z",
        }
    }
}

/// A recently accessed item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentItem {
    /// Type of item
    pub item_type: RecentItemType,
    /// Unique identifier
    pub id: String,
    /// Display label
    pub label: String,
    /// Navigation path
    pub path: String,
    /// Unix timestamp (milliseconds)
    pub timestamp: f64,
    /// Optional subtitle/description
    pub subtitle: Option<String>,
}

impl RecentItem {
    /// Create a new recent item
    pub fn new(
        item_type: RecentItemType,
        id: impl Into<String>,
        label: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        let path = canonicalize_recent_path(&path.into());
        Self {
            item_type,
            id: id.into(),
            label: label.into(),
            path,
            timestamp: current_timestamp(),
            subtitle: None,
        }
    }

    /// Add a subtitle to this item
    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Create a page item
    pub fn page(id: &str, label: &str, path: &str) -> Self {
        Self::new(RecentItemType::Page, id, label, path)
    }

    /// Create an adapter item
    pub fn adapter(id: &str, name: &str, adapter_id: &str) -> Self {
        Self::new(
            RecentItemType::Adapter,
            id,
            name,
            format!("/adapters/{}", id),
        )
        .with_subtitle(adapter_id)
    }

    /// Create a model item
    pub fn model(id: &str, name: &str) -> Self {
        Self::new(RecentItemType::Model, id, name, format!("/models/{}", id))
    }

    /// Create a worker item
    pub fn worker(id: &str, status: &str) -> Self {
        Self::new(
            RecentItemType::Worker,
            id,
            format!("Worker {}", adapteros_id::short_id(id)),
            format!("/workers/{}", id),
        )
        .with_subtitle(status)
    }
}

/// Manager for recent items with localStorage persistence
#[derive(Debug, Clone)]
pub struct RecentItemsManager {
    items: Vec<RecentItem>,
}

impl Default for RecentItemsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RecentItemsManager {
    /// Create a new manager, loading from localStorage
    pub fn new() -> Self {
        Self {
            items: Self::load_from_storage(),
        }
    }

    /// Get all recent items
    pub fn items(&self) -> &[RecentItem] {
        &self.items
    }

    /// Add an item to recents (or move to front if exists)
    pub fn add(&mut self, item: RecentItem) {
        let mut item = item;
        item.path = canonicalize_recent_path(&item.path);
        let canonical_path = item.path.clone();

        // Remove existing entry with same path
        self.items
            .retain(|i| canonicalize_recent_path(&i.path) != canonical_path);

        // Add to front
        self.items.insert(0, item);

        // Trim to max size
        self.items.truncate(MAX_RECENT_ITEMS);

        // Persist
        self.save_to_storage();
    }

    /// Remove an item by path
    pub fn remove(&mut self, path: &str) {
        let canonical_path = canonicalize_recent_path(path);
        self.items
            .retain(|i| canonicalize_recent_path(&i.path) != canonical_path);
        self.save_to_storage();
    }

    /// Clear all recent items
    pub fn clear(&mut self) {
        self.items.clear();
        self.save_to_storage();
    }

    /// Load items from localStorage
    fn load_from_storage() -> Vec<RecentItem> {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten())
                .and_then(|s| s.get_item(RECENT_ITEMS_KEY).ok().flatten())
                .and_then(|json| serde_json::from_str::<Vec<RecentItem>>(&json).ok())
                .map(|mut items| {
                    for item in &mut items {
                        item.path = canonicalize_recent_path(&item.path);
                    }
                    items
                })
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Vec::new()
        }
    }

    /// Save items to localStorage
    fn save_to_storage(&self) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
            {
                if let Ok(json) = serde_json::to_string(&self.items) {
                    let _ = storage.set_item(RECENT_ITEMS_KEY, &json);
                }
            }
        }
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_dedup() {
        let mut manager = RecentItemsManager { items: Vec::new() };

        manager.add(RecentItem::page("dashboard", "Dashboard", "/dashboard"));
        manager.add(RecentItem::page("adapters", "Adapters", "/adapters"));
        manager.add(RecentItem::page("dashboard", "Dashboard", "/")); // Duplicate

        assert_eq!(manager.items.len(), 2);
        assert_eq!(manager.items[0].id, "dashboard"); // Should be first (most recent)
        assert_eq!(manager.items[0].path, "/");
    }

    #[test]
    fn test_max_items() {
        let mut manager = RecentItemsManager { items: Vec::new() };

        for i in 0..30 {
            manager.add(RecentItem::page(
                &format!("page-{}", i),
                &format!("Page {}", i),
                &format!("/page-{}", i),
            ));
        }

        assert_eq!(manager.items.len(), MAX_RECENT_ITEMS);
    }

    #[test]
    fn test_model_recent_item_uses_detail_path() {
        let item = RecentItem::model("mdl_123", "Qwen 7B");
        assert_eq!(item.path, "/models/mdl_123");
    }
}
