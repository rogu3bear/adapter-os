//! Query helper utilities to reduce duplication across database operations
//!
//! This module provides common patterns used throughout the codebase:
//! - Error mapping helpers
//! - Dynamic query builders
//! - Batch operation tracking
//! - Common row mapping utilities

use adapteros_core::{AosError, Result};
use sqlx::Database;
use std::fmt::Display;

/// Map sqlx errors to AosError::Database with context
///
/// This helper reduces the repetitive `.map_err(|e| AosError::Database(format!("...: {}", e)))`
/// pattern that appears 333 times in the codebase.
///
/// # Examples
///
/// ```rust,no_run
/// # use adapteros_db::query_helpers::db_err;
/// # use sqlx::SqlitePool;
/// # async fn example(pool: &SqlitePool) -> adapteros_core::Result<()> {
/// let result = sqlx::query("SELECT * FROM adapters")
///     .fetch_all(pool)
///     .await
///     .map_err(db_err("fetch adapters"))?;
/// # Ok(())
/// # }
/// ```
#[inline]
pub fn db_err(context: &str) -> impl Fn(sqlx::Error) -> AosError + '_ {
    move |e| AosError::Database(format!("Failed to {}: {}", context, e))
}

/// Map serialization errors to AosError::Serialization
///
/// Helper for reducing repetitive serde_json error mapping.
///
/// # Examples
///
/// ```rust,no_run
/// # use adapteros_db::query_helpers::serde_err;
/// # fn example() -> adapteros_core::Result<String> {
/// let json = serde_json::to_string(&vec!["a", "b"])
///     .map_err(serde_err)?;
/// # Ok(json)
/// # }
/// ```
#[inline]
pub fn serde_err(e: serde_json::Error) -> AosError {
    AosError::Serialization(e)
}

/// Batch operation result tracker
///
/// Reduces duplication of the pattern:
/// ```rust,ignore
/// let mut successful = 0;
/// let mut failed = 0;
/// for item in items {
///     match operation(item).await {
///         Ok(()) => successful += 1,
///         Err(e) => {
///             failed += 1;
///             warn!("...");
///         }
///     }
/// }
/// if failed > 0 {
///     return Err(...);
/// }
/// ```
///
/// # Examples
///
/// ```rust,no_run
/// # use adapteros_db::query_helpers::BatchTracker;
/// # async fn evict_adapter(id: &str) -> adapteros_core::Result<()> { Ok(()) }
/// # async fn example() -> adapteros_core::Result<()> {
/// let mut tracker = BatchTracker::new("eviction");
///
/// for adapter_id in &["a", "b", "c"] {
///     tracker.track(evict_adapter(adapter_id).await);
/// }
///
/// tracker.finish()?; // Returns error if any operations failed
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct BatchTracker {
    operation: String,
    successful: usize,
    failed: usize,
}

impl BatchTracker {
    /// Create a new batch operation tracker
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            successful: 0,
            failed: 0,
        }
    }

    /// Track a single operation result
    pub fn track<T, E>(&mut self, result: std::result::Result<T, E>) -> Option<T>
    where
        E: Display,
    {
        match result {
            Ok(value) => {
                self.successful += 1;
                Some(value)
            }
            Err(e) => {
                self.failed += 1;
                tracing::warn!(
                    operation = %self.operation,
                    error = %e,
                    "Batch operation failed for item"
                );
                None
            }
        }
    }

    /// Get the number of successful operations
    pub fn successful(&self) -> usize {
        self.successful
    }

    /// Get the number of failed operations
    pub fn failed(&self) -> usize {
        self.failed
    }

    /// Finish the batch and return error if any operations failed
    pub fn finish(self) -> Result<()> {
        tracing::info!(
            operation = %self.operation,
            successful = self.successful,
            failed = self.failed,
            "Batch operation completed"
        );

        if self.failed > 0 {
            Err(AosError::Worker(format!(
                "Batch {}: {} successful, {} failed",
                self.operation, self.successful, self.failed
            )))
        } else {
            Ok(())
        }
    }

    /// Finish the batch and return statistics without error
    pub fn finish_with_stats(self) -> (usize, usize) {
        tracing::info!(
            operation = %self.operation,
            successful = self.successful,
            failed = self.failed,
            "Batch operation completed"
        );
        (self.successful, self.failed)
    }
}

/// Dynamic query builder for filtering queries
///
/// Reduces duplication of the pattern where we build queries with optional filters:
/// ```rust,ignore
/// let mut query = "SELECT * FROM table WHERE tenant_id = ?".to_string();
/// let mut params = vec![tenant_id];
/// if let Some(x) = filter_x {
///     query.push_str(" AND x = ?");
///     params.push(x);
/// }
/// ```
///
/// # Examples
///
/// ```rust,no_run
/// # use adapteros_db::query_helpers::FilterBuilder;
/// let mut builder = FilterBuilder::new("SELECT * FROM audit_logs WHERE tenant_id = ?");
/// builder.add_param("default");
///
/// builder.add_filter("user_id", Some("admin"));
/// builder.add_filter("action", Some("delete"));
/// builder.add_filter("resource_type", None::<String>); // Skipped
///
/// builder.push_str(" ORDER BY timestamp DESC LIMIT ?");
/// builder.add_param(100);
///
/// let (query, params) = builder.build();
/// // query = "SELECT * FROM audit_logs WHERE tenant_id = ? AND user_id = ? AND action = ? ORDER BY timestamp DESC LIMIT ?"
/// // params = ["default", "admin", "delete", "100"]
/// ```
pub struct FilterBuilder {
    query: String,
    params: Vec<String>,
}

impl FilterBuilder {
    /// Create a new filter builder with initial query
    pub fn new(base_query: impl Into<String>) -> Self {
        Self {
            query: base_query.into(),
            params: Vec::new(),
        }
    }

    /// Add a required parameter (always included)
    pub fn add_param(&mut self, value: impl ToString) -> &mut Self {
        self.params.push(value.to_string());
        self
    }

    /// Add an optional filter condition
    ///
    /// If the value is Some, appends " AND {column} = ?" to the query and adds the value to params.
    /// If None, does nothing.
    pub fn add_filter<T: ToString>(&mut self, column: &str, value: Option<T>) -> &mut Self {
        if let Some(v) = value {
            self.query.push_str(&format!(" AND {} = ?", column));
            self.params.push(v.to_string());
        }
        self
    }

    /// Add a raw SQL fragment (use carefully)
    pub fn push_str(&mut self, sql: &str) -> &mut Self {
        self.query.push_str(sql);
        self
    }

    /// Build the final query and parameters
    pub fn build(self) -> (String, Vec<String>) {
        (self.query, self.params)
    }

    /// Get a reference to the current query
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Get a reference to the current params
    pub fn params(&self) -> &[String] {
        &self.params
    }
}

// Note: execute_filtered_query was removed due to trait bound complexity.
// Users should build queries manually using the FilterBuilder pattern shown in audit.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_builder_basic() {
        let mut builder = FilterBuilder::new("SELECT * FROM table WHERE id = ?");
        builder.add_param("123");

        let (query, params) = builder.build();
        assert_eq!(query, "SELECT * FROM table WHERE id = ?");
        assert_eq!(params, vec!["123"]);
    }

    #[test]
    fn test_filter_builder_optional_filters() {
        let mut builder = FilterBuilder::new("SELECT * FROM table WHERE id = ?");
        builder.add_param("123");
        builder.add_filter("name", Some("test"));
        builder.add_filter("age", None::<i32>);
        builder.add_filter("active", Some(true));

        let (query, params) = builder.build();
        assert_eq!(
            query,
            "SELECT * FROM table WHERE id = ? AND name = ? AND active = ?"
        );
        assert_eq!(params, vec!["123", "test", "true"]);
    }

    #[test]
    fn test_batch_tracker() {
        let mut tracker = BatchTracker::new("test");

        tracker.track(Ok::<_, String>(()));
        tracker.track(Ok::<_, String>(()));
        tracker.track(Err("error".to_string()));
        tracker.track(Ok::<_, String>(()));

        assert_eq!(tracker.successful(), 3);
        assert_eq!(tracker.failed(), 1);

        let result = tracker.finish();
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_tracker_all_success() {
        let mut tracker = BatchTracker::new("test");

        tracker.track(Ok::<_, String>(()));
        tracker.track(Ok::<_, String>(()));

        assert_eq!(tracker.successful(), 2);
        assert_eq!(tracker.failed(), 0);

        let result = tracker.finish();
        assert!(result.is_ok());
    }

    #[test]
    fn test_db_err() {
        let err = db_err("test operation")(sqlx::Error::RowNotFound);
        match err {
            AosError::Database(msg) => {
                assert!(msg.starts_with("Failed to test operation:"));
            }
            _ => panic!("Expected Database error"),
        }
    }
}
