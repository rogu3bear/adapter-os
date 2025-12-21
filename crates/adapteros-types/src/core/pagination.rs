//! Pagination types for list endpoints

use serde::{Deserialize, Serialize};

/// Pagination parameters for list requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PaginationParams {
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,

    /// Items per page
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    50
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: default_page(),
            limit: default_limit(),
        }
    }
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PaginatedResponse<T> {
    /// List of items in this page
    pub data: Vec<T>,

    /// Total number of items across all pages
    pub total: u64,

    /// Current page number (1-indexed)
    pub page: u32,

    /// Items per page
    pub limit: u32,

    /// Total number of pages
    pub pages: u32,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response
    pub fn new(data: Vec<T>, total: u64, page: u32, limit: u32) -> Self {
        let pages = ((total as f64) / (limit as f64)).ceil() as u32;
        Self {
            data,
            total,
            page,
            limit,
            pages,
        }
    }
}
