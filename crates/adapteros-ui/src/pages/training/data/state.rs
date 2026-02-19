//! State management for training data page.
//!
//! Provides types and derived helpers for managing the data lifecycle:
//! - Documents (raw uploaded files)
//! - Datasets (validated training data)
//! - Preprocessed (CoreML feature cache)

use std::collections::HashSet;

/// Data source categories in the training data page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DataSource {
    /// Raw uploaded documents (PDFs, markdown, etc.)
    Documents,
    /// Processed training datasets (validated JSONL)
    #[default]
    Datasets,
    /// CoreML preprocessed feature cache
    Preprocessed,
}

impl DataSource {
    /// Returns the display label for this source.
    pub fn label(&self) -> &'static str {
        match self {
            DataSource::Documents => "Your files",
            DataSource::Datasets => "Training data",
            DataSource::Preprocessed => "Preprocessed",
        }
    }

    /// Returns the icon for this source.
    pub fn icon(&self) -> &'static str {
        match self {
            DataSource::Documents => "📄",
            DataSource::Datasets => "📦",
            DataSource::Preprocessed => "⚡",
        }
    }
}

/// Status of a document in the processing pipeline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DocumentStatus {
    /// Uploaded but not yet processed
    #[default]
    Raw,
    /// Currently being chunked/embedded
    Processing,
    /// Ready for dataset creation
    Indexed,
    /// Processing failed
    Failed,
}

impl std::str::FromStr for DocumentStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "raw" => DocumentStatus::Raw,
            "processing" => DocumentStatus::Processing,
            "indexed" => DocumentStatus::Indexed,
            "failed" => DocumentStatus::Failed,
            _ => DocumentStatus::Raw,
        })
    }
}

impl DocumentStatus {
    pub fn label(&self) -> &'static str {
        match self {
            DocumentStatus::Raw => "Raw",
            DocumentStatus::Processing => "Processing",
            DocumentStatus::Indexed => "Indexed",
            DocumentStatus::Failed => "Failed",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            DocumentStatus::Raw => "📄",
            DocumentStatus::Processing => "⏳",
            DocumentStatus::Indexed => "✅",
            DocumentStatus::Failed => "❌",
        }
    }
}

/// Status of dataset validation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DatasetStatus {
    /// Awaiting validation
    #[default]
    Pending,
    /// Validated and ready for training
    Valid,
    /// Failed validation
    Invalid,
}

impl std::str::FromStr for DatasetStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "pending" | "uploaded" => DatasetStatus::Pending,
            "valid" | "validated" => DatasetStatus::Valid,
            "invalid" | "failed" => DatasetStatus::Invalid,
            _ => DatasetStatus::Pending,
        })
    }
}

impl DatasetStatus {
    pub fn label(&self) -> &'static str {
        match self {
            DatasetStatus::Pending => "Pending",
            DatasetStatus::Valid => "Valid",
            DatasetStatus::Invalid => "Invalid",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            DatasetStatus::Pending => "⏳",
            DatasetStatus::Valid => "✅",
            DatasetStatus::Invalid => "❌",
        }
    }
}

/// Status of CoreML preprocessing cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreprocessStatus {
    /// No CoreML cache exists
    #[default]
    None,
    /// CoreML features are cached and up-to-date
    Cached,
    /// Cache is outdated (model/config changed)
    Stale,
}

impl std::str::FromStr for PreprocessStatus {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "none" | "" => PreprocessStatus::None,
            "cached" | "ready" => PreprocessStatus::Cached,
            "stale" | "outdated" => PreprocessStatus::Stale,
            _ => PreprocessStatus::None,
        })
    }
}

impl PreprocessStatus {
    pub fn label(&self) -> &'static str {
        match self {
            PreprocessStatus::None => "Not Cached",
            PreprocessStatus::Cached => "Cached",
            PreprocessStatus::Stale => "Stale",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            PreprocessStatus::None => "○",
            PreprocessStatus::Cached => "⚡",
            PreprocessStatus::Stale => "⚠️",
        }
    }
}

/// Filter options for the data list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataFilters {
    /// Filter by status string
    pub status: Option<String>,
    /// Filter by format type
    pub format: Option<String>,
    /// Search query for name/description
    pub search_query: String,
}

impl DataFilters {
    /// Check if any filters are active.
    pub fn is_empty(&self) -> bool {
        self.status.is_none() && self.format.is_none() && self.search_query.is_empty()
    }

    /// Clear all filters.
    pub fn clear(&mut self) {
        self.status = None;
        self.format = None;
        self.search_query.clear();
    }
}

/// Sort options for the data list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DataSort {
    /// Sort by name ascending
    NameAsc,
    /// Sort by name descending
    NameDesc,
    /// Sort by date ascending (oldest first)
    DateAsc,
    /// Sort by date descending (newest first)
    #[default]
    DateDesc,
    /// Sort by size ascending (smallest first)
    SizeAsc,
    /// Sort by size descending (largest first)
    SizeDesc,
}

impl DataSort {
    pub fn label(&self) -> &'static str {
        match self {
            DataSort::NameAsc => "Name (A-Z)",
            DataSort::NameDesc => "Name (Z-A)",
            DataSort::DateAsc => "Date (Oldest)",
            DataSort::DateDesc => "Date (Newest)",
            DataSort::SizeAsc => "Size (Smallest)",
            DataSort::SizeDesc => "Size (Largest)",
        }
    }
}

/// Unified state for data management page.
#[derive(Clone, Debug, Default)]
pub struct DataManagementState {
    /// Currently active data source view
    pub active_source: DataSource,
    /// Selected item ID (for detail panel)
    pub selected_id: Option<String>,
    /// Multi-select set for batch operations
    pub selection_set: HashSet<String>,
    /// Active filters
    pub filters: DataFilters,
    /// Sort order
    pub sort: DataSort,
}

impl DataManagementState {
    /// Create new state with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if an item is selected.
    pub fn is_selected(&self, id: &str) -> bool {
        self.selection_set.contains(id)
    }

    /// Toggle selection of an item.
    pub fn toggle_selection(&mut self, id: String) {
        if self.selection_set.contains(&id) {
            self.selection_set.remove(&id);
        } else {
            self.selection_set.insert(id);
        }
    }

    /// Clear all selections.
    pub fn clear_selections(&mut self) {
        self.selection_set.clear();
    }

    /// Select a single item (clears multi-select, sets selected_id).
    pub fn select_item(&mut self, id: String) {
        self.selection_set.clear();
        self.selected_id = Some(id);
    }

    /// Clear the detail selection.
    pub fn clear_detail_selection(&mut self) {
        self.selected_id = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_source_defaults_to_datasets() {
        let state = DataManagementState::new();
        assert_eq!(state.active_source, DataSource::Datasets);
    }

    #[test]
    fn toggle_selection_adds_and_removes() {
        let mut state = DataManagementState::new();
        state.toggle_selection("id-1".to_string());
        assert!(state.is_selected("id-1"));

        state.toggle_selection("id-1".to_string());
        assert!(!state.is_selected("id-1"));
    }

    #[test]
    fn select_item_clears_multi_select() {
        let mut state = DataManagementState::new();
        state.toggle_selection("id-1".to_string());
        state.toggle_selection("id-2".to_string());
        assert_eq!(state.selection_set.len(), 2);

        state.select_item("id-3".to_string());
        assert!(state.selection_set.is_empty());
        assert_eq!(state.selected_id, Some("id-3".to_string()));
    }

    #[test]
    fn filters_is_empty_check() {
        let mut filters = DataFilters::default();
        assert!(filters.is_empty());

        filters.search_query = "test".to_string();
        assert!(!filters.is_empty());

        filters.clear();
        assert!(filters.is_empty());
    }
}
