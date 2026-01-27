//! Pagination types for list API endpoints
//!
//! This module provides standard pagination structures used across
//! all server-api handler crates.

use serde::{Deserialize, Serialize};

/// Pagination parameters for list requests
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaginationParams {
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Number of items per page
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    20
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: default_page(),
            per_page: default_per_page(),
        }
    }
}

impl PaginationParams {
    /// Create new pagination params
    pub fn new(page: u32, per_page: u32) -> Self {
        Self { page, per_page }
    }

    /// Calculate the offset for database queries
    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.per_page
    }

    /// Get the limit for database queries
    pub fn limit(&self) -> u32 {
        self.per_page
    }

    /// Clamp per_page to a maximum value
    pub fn with_max_per_page(mut self, max: u32) -> Self {
        self.per_page = self.per_page.min(max);
        self
    }
}

/// Page information returned with paginated responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    /// Current page number (1-indexed)
    pub page: u32,
    /// Items per page
    pub per_page: u32,
    /// Total number of items across all pages
    pub total_items: u64,
    /// Total number of pages
    pub total_pages: u32,
    /// Whether there is a next page
    pub has_next: bool,
    /// Whether there is a previous page
    pub has_prev: bool,
}

impl PageInfo {
    /// Create page info from pagination params and total count
    pub fn new(params: &PaginationParams, total_items: u64) -> Self {
        let total_pages = if params.per_page == 0 {
            0
        } else {
            ((total_items as f64) / (params.per_page as f64)).ceil() as u32
        };

        Self {
            page: params.page,
            per_page: params.per_page,
            total_items,
            total_pages,
            has_next: params.page < total_pages,
            has_prev: params.page > 1,
        }
    }
}

/// Paginated response wrapper
#[derive(Debug, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    /// The page of items
    pub items: Vec<T>,
    /// Pagination information
    pub page_info: PageInfo,
}

impl<T> PaginatedResponse<T> {
    /// Create a new paginated response
    pub fn new(items: Vec<T>, params: &PaginationParams, total_items: u64) -> Self {
        Self {
            items,
            page_info: PageInfo::new(params, total_items),
        }
    }

    /// Create an empty paginated response
    pub fn empty(params: &PaginationParams) -> Self {
        Self {
            items: Vec::new(),
            page_info: PageInfo::new(params, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params_defaults() {
        let params = PaginationParams::default();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 20);
    }

    #[test]
    fn test_pagination_params_new() {
        let params = PaginationParams::new(5, 50);
        assert_eq!(params.page, 5);
        assert_eq!(params.per_page, 50);
    }

    #[test]
    fn test_pagination_offset() {
        let params = PaginationParams::new(1, 20);
        assert_eq!(params.offset(), 0);

        let params = PaginationParams::new(2, 20);
        assert_eq!(params.offset(), 20);

        let params = PaginationParams::new(3, 10);
        assert_eq!(params.offset(), 20);
    }

    #[test]
    fn test_pagination_offset_underflow() {
        let params = PaginationParams::new(0, 20);
        assert_eq!(params.offset(), 0); // Should not underflow
    }

    #[test]
    fn test_pagination_limit() {
        let params = PaginationParams::new(1, 10);
        assert_eq!(params.limit(), 10);

        let params = PaginationParams::new(5, 100);
        assert_eq!(params.limit(), 100);
    }

    #[test]
    fn test_page_info() {
        let params = PaginationParams::new(2, 10);
        let info = PageInfo::new(&params, 25);

        assert_eq!(info.page, 2);
        assert_eq!(info.per_page, 10);
        assert_eq!(info.total_items, 25);
        assert_eq!(info.total_pages, 3);
        assert!(info.has_next);
        assert!(info.has_prev);
    }

    #[test]
    fn test_page_info_first_page() {
        let params = PaginationParams::new(1, 10);
        let info = PageInfo::new(&params, 25);

        assert!(!info.has_prev);
        assert!(info.has_next);
    }

    #[test]
    fn test_page_info_last_page() {
        let params = PaginationParams::new(3, 10);
        let info = PageInfo::new(&params, 25);

        assert!(info.has_prev);
        assert!(!info.has_next);
    }

    #[test]
    fn test_page_info_single_page() {
        let params = PaginationParams::new(1, 20);
        let info = PageInfo::new(&params, 10);

        assert_eq!(info.total_pages, 1);
        assert!(!info.has_prev);
        assert!(!info.has_next);
    }

    #[test]
    fn test_page_info_empty() {
        let params = PaginationParams::new(1, 20);
        let info = PageInfo::new(&params, 0);

        assert_eq!(info.total_items, 0);
        assert_eq!(info.total_pages, 0);
        assert!(!info.has_prev);
        assert!(!info.has_next);
    }

    #[test]
    fn test_page_info_exact_multiple() {
        let params = PaginationParams::new(1, 10);
        let info = PageInfo::new(&params, 30);

        assert_eq!(info.total_pages, 3);
        assert!(!info.has_prev);
        assert!(info.has_next);
    }

    #[test]
    fn test_page_info_beyond_last_page() {
        let params = PaginationParams::new(10, 10);
        let info = PageInfo::new(&params, 25);

        assert_eq!(info.page, 10);
        assert_eq!(info.total_pages, 3);
        assert!(info.has_prev);
        assert!(!info.has_next); // page 10 > total_pages 3
    }

    #[test]
    fn test_page_info_zero_per_page() {
        let params = PaginationParams::new(1, 0);
        let info = PageInfo::new(&params, 100);

        assert_eq!(info.total_pages, 0);
        assert!(!info.has_next);
    }

    #[test]
    fn test_paginated_response() {
        let items = vec!["a", "b", "c"];
        let params = PaginationParams::new(1, 3);
        let response = PaginatedResponse::new(items, &params, 10);

        assert_eq!(response.items.len(), 3);
        assert_eq!(response.page_info.total_items, 10);
        assert_eq!(response.page_info.total_pages, 4);
    }

    #[test]
    fn test_paginated_response_empty() {
        let params = PaginationParams::new(1, 20);
        let response: PaginatedResponse<String> = PaginatedResponse::empty(&params);

        assert_eq!(response.items.len(), 0);
        assert_eq!(response.page_info.total_items, 0);
        assert_eq!(response.page_info.total_pages, 0);
    }

    #[test]
    fn test_with_max_per_page() {
        let params = PaginationParams::new(1, 100).with_max_per_page(50);
        assert_eq!(params.per_page, 50);

        let params = PaginationParams::new(1, 20).with_max_per_page(50);
        assert_eq!(params.per_page, 20);
    }

    #[test]
    fn test_with_max_per_page_zero() {
        let params = PaginationParams::new(1, 100).with_max_per_page(0);
        assert_eq!(params.per_page, 0);
    }

    // Serde roundtrip tests
    #[test]
    fn test_pagination_params_serialize() {
        let params = PaginationParams::new(3, 50);
        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains(r#""page":3"#));
        assert!(json.contains(r#""per_page":50"#));
    }

    #[test]
    fn test_pagination_params_deserialize() {
        let json = r#"{"page":5,"per_page":25}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.page, 5);
        assert_eq!(params.per_page, 25);
    }

    #[test]
    fn test_pagination_params_deserialize_defaults() {
        let json = r#"{}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.page, 1);
        assert_eq!(params.per_page, 20);
    }

    #[test]
    fn test_pagination_params_deserialize_partial() {
        let json = r#"{"page":3}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.page, 3);
        assert_eq!(params.per_page, 20); // default

        let json = r#"{"per_page":100}"#;
        let params: PaginationParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.page, 1); // default
        assert_eq!(params.per_page, 100);
    }

    #[test]
    fn test_page_info_serialize() {
        let params = PaginationParams::new(2, 10);
        let info = PageInfo::new(&params, 25);
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains(r#""page":2"#));
        assert!(json.contains(r#""per_page":10"#));
        assert!(json.contains(r#""total_items":25"#));
        assert!(json.contains(r#""total_pages":3"#));
        assert!(json.contains(r#""has_next":true"#));
        assert!(json.contains(r#""has_prev":true"#));
    }

    #[test]
    fn test_page_info_deserialize() {
        let json = r#"{
            "page":2,
            "per_page":10,
            "total_items":25,
            "total_pages":3,
            "has_next":true,
            "has_prev":true
        }"#;
        let info: PageInfo = serde_json::from_str(json).unwrap();
        assert_eq!(info.page, 2);
        assert_eq!(info.per_page, 10);
        assert_eq!(info.total_items, 25);
        assert_eq!(info.total_pages, 3);
        assert!(info.has_next);
        assert!(info.has_prev);
    }

    #[test]
    fn test_paginated_response_serialize() {
        let items = vec!["a", "b", "c"];
        let params = PaginationParams::new(1, 3);
        let response = PaginatedResponse::new(items, &params, 10);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""items":["a","b","c"]"#));
        assert!(json.contains(r#""total_items":10"#));
        assert!(json.contains(r#""total_pages":4"#));
    }

    #[test]
    fn test_paginated_response_deserialize() {
        let json = r#"{
            "items": ["x", "y", "z"],
            "page_info": {
                "page": 1,
                "per_page": 3,
                "total_items": 10,
                "total_pages": 4,
                "has_next": true,
                "has_prev": false
            }
        }"#;
        let response: PaginatedResponse<String> = serde_json::from_str(json).unwrap();
        assert_eq!(response.items, vec!["x", "y", "z"]);
        assert_eq!(response.page_info.total_items, 10);
        assert_eq!(response.page_info.total_pages, 4);
        assert!(response.page_info.has_next);
        assert!(!response.page_info.has_prev);
    }

    #[test]
    fn test_paginated_response_roundtrip() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestItem {
            id: u32,
            name: String,
        }

        let items = vec![
            TestItem {
                id: 1,
                name: "item1".to_string(),
            },
            TestItem {
                id: 2,
                name: "item2".to_string(),
            },
        ];
        let params = PaginationParams::new(2, 2);
        let response = PaginatedResponse::new(items.clone(), &params, 10);

        let json = serde_json::to_string(&response).unwrap();
        let parsed: PaginatedResponse<TestItem> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.items, items);
        assert_eq!(parsed.page_info.page, 2);
        assert_eq!(parsed.page_info.per_page, 2);
        assert_eq!(parsed.page_info.total_items, 10);
    }

    // Edge cases
    #[test]
    fn test_large_pagination_values() {
        let params = PaginationParams::new(1000, 1000);
        assert_eq!(params.offset(), 999_000);
        assert_eq!(params.limit(), 1000);
    }

    #[test]
    fn test_page_info_large_values() {
        let params = PaginationParams::new(100, 100);
        let info = PageInfo::new(&params, 1_000_000);
        assert_eq!(info.total_pages, 10_000);
        assert!(info.has_prev);
        assert!(info.has_next);
    }

    #[test]
    fn test_pagination_params_clone() {
        let params = PaginationParams::new(5, 30);
        let cloned = params.clone();
        assert_eq!(cloned.page, params.page);
        assert_eq!(cloned.per_page, params.per_page);
    }

    #[test]
    fn test_page_info_clone() {
        let params = PaginationParams::new(2, 10);
        let info = PageInfo::new(&params, 25);
        let cloned = info.clone();
        assert_eq!(cloned.page, info.page);
        assert_eq!(cloned.total_items, info.total_items);
        assert_eq!(cloned.has_next, info.has_next);
    }
}
