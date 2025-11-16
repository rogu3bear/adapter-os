//! Performance Testing for Directory Adapter Operations
//!
//! This test suite measures the performance of `upsert_directory_adapter` handler,
//! specifically tracking the time spent in filesystem operations vs. database operations.
//!
//! ## Purpose
//!
//! After async refactoring to parallelize filesystem analysis and database operations,
//! we expect:
//! - Total handler time to decrease by ≥20% (operations run in parallel)
//! - FS/DB ratio to approach 1.0-2.0 (more balanced, less sequential)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all performance tests with output
//! cargo test directory_adapter_performance --nocapture
//!
//! # Run baseline only (to establish measurements)
//! cargo test test_directory_adapter_timing_baseline --nocapture
//!
//! # Run after refactor (to verify improvements)
//! cargo test test_directory_adapter_timing_after_refactor --nocapture
//! ```
//!
//! ## Expected Metrics
//!
//! **Before async refactor (serial execution)**:
//! - Filesystem ops: ~500ms (blocking)
//! - Database ops: ~50ms (sequential after blocking)
//! - Total: ~550ms
//! - Ratio: 10.0 (fs dominates)
//!
//! **After async refactor (parallel execution)**:
//! - Filesystem ops: ~500ms (async)
//! - Database ops: ~50ms (runs in parallel)
//! - Total: ~500ms (overlapped)
//! - Ratio: 1.0-2.0 (more balanced)

use adapteros_db::Db;
use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_server_api::state::AppState;
use adapteros_server_api::types::{DirectoryUpsertRequest, DirectoryUpsertResponse};
use axum::{
    extract::{Extension, State},
    Json,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tempfile::TempDir;
use tracing_subscriber::{layer::SubscriberExt, Registry};

mod helpers;
use helpers::{ImprovementReport, TimingMetrics, TracingCapture};

/// Test fixture with temporary directory structure
struct TestFixture {
    temp_dir: TempDir,
    test_root: PathBuf,
    db: Db,
}

impl TestFixture {
    /// Create a new test fixture with sample code files
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let test_root = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_root)?;

        // Create a realistic directory structure with code files
        Self::create_sample_project(&test_root)?;

        // Initialize test database (in-memory SQLite)
        let db = Db::open_memory().await?;

        // Create test tenant
        db.create_tenant("test-tenant", None, None).await?;

        Ok(Self {
            temp_dir,
            test_root,
            db,
        })
    }

    /// Create a sample project structure with various code files
    fn create_sample_project(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
        // Create src directory
        let src_dir = root.join("src");
        fs::create_dir_all(&src_dir)?;

        // Create main.rs
        fs::write(
            src_dir.join("main.rs"),
            r#"
use std::io;

fn main() -> Result<(), io::Error> {
    println!("Hello, world!");
    let result = calculate_sum(5, 10);
    println!("Sum: {}", result);
    Ok(())
}

fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_sum() {
        assert_eq!(calculate_sum(2, 3), 5);
    }
}
"#,
        )?;

        // Create lib.rs
        fs::write(
            src_dir.join("lib.rs"),
            r#"
pub mod utils;

pub fn process_data(input: &str) -> String {
    input.to_uppercase()
}

pub struct DataProcessor {
    buffer: Vec<u8>,
}

impl DataProcessor {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn add_data(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
    }

    pub fn process(&self) -> Vec<u8> {
        self.buffer.clone()
    }
}
"#,
        )?;

        // Create utils.rs
        fs::write(
            src_dir.join("utils.rs"),
            r#"
use std::collections::HashMap;

pub fn build_map() -> HashMap<String, i32> {
    let mut map = HashMap::new();
    map.insert("one".to_string(), 1);
    map.insert("two".to_string(), 2);
    map
}

pub fn validate_input(input: &str) -> bool {
    !input.is_empty() && input.len() < 100
}
"#,
        )?;

        // Create tests directory
        let tests_dir = root.join("tests");
        fs::create_dir_all(&tests_dir)?;
        fs::write(
            tests_dir.join("integration_test.rs"),
            r#"
use my_project::process_data;

#[test]
fn test_process_data() {
    let result = process_data("hello");
    assert_eq!(result, "HELLO");
}
"#,
        )?;

        // Create README.md
        fs::write(
            root.join("README.md"),
            r#"
# Test Project

This is a sample project for testing directory adapter performance.

## Features

- Data processing
- Utility functions
- Comprehensive tests
"#,
        )?;

        Ok(())
    }

    /// Create AppState for testing
    fn create_app_state(&self) -> AppState {
        use adapteros_config::Config;

        // Create minimal config
        let config = Config::default();

        AppState {
            db: self.db.clone(),
            lifecycle_manager: None, // Not needed for this test
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Create test request
    fn create_request(&self) -> DirectoryUpsertRequest {
        DirectoryUpsertRequest {
            tenant_id: "test-tenant".to_string(),
            root: self.test_root.to_string_lossy().to_string(),
            path: ".".to_string(),
            activate: false, // Don't activate to focus on core operations
        }
    }
}

/// Initialize tracing subscriber with capture layer
fn init_tracing() -> TracingCapture {
    let capture = TracingCapture::new();
    let capture_layer = capture.clone();

    let subscriber = Registry::default().with(capture_layer);

    let _ = tracing::subscriber::set_global_default(subscriber);

    capture
}

#[tokio::test]
async fn test_directory_adapter_timing_baseline() {
    // Initialize tracing capture
    let capture = init_tracing();

    // Create test fixture
    let fixture = TestFixture::new()
        .await
        .expect("Failed to create test fixture");

    let app_state = fixture.create_app_state();
    let request = fixture.create_request();

    // Create mock claims (admin role)
    use adapteros_db::users::{Claims, Role};
    let claims = Claims {
        sub: "test-user".to_string(),
        role: Role::Admin,
        tenant_id: Some("test-tenant".to_string()),
        exp: 9999999999,
        iat: 0,
    };

    // Execute the handler
    capture.clear();

    let result = adapteros_server_api::handlers::upsert_directory_adapter(
        State(app_state),
        Extension(claims),
        Json(request),
    )
    .await;

    assert!(result.is_ok(), "Handler should succeed");

    // Extract timing metrics
    let spans = capture.get_spans();
    let metrics = TimingMetrics::from_spans(&spans);

    println!("\n=== Baseline Performance Metrics ===");
    println!("Filesystem time:     {} ms", metrics.filesystem_time_ms);
    println!("Database time:       {} ms", metrics.database_time_ms);
    println!("Total handler time:  {} ms", metrics.total_handler_time_ms);
    println!("FS/DB ratio:         {:.2}", metrics.fs_db_ratio);
    println!("\nSpan Breakdown:");
    for (name, duration) in &metrics.span_breakdown {
        println!("  {:40} {:6} ms", name, duration);
    }

    // Save baseline for comparison
    let baseline_path = PathBuf::from("test_data/directory_adapter_baseline.json");
    metrics
        .save_baseline(&baseline_path)
        .expect("Failed to save baseline");

    println!("\n✓ Baseline metrics saved to {}", baseline_path.display());

    // Basic sanity checks
    assert!(
        metrics.filesystem_time_ms > 0,
        "Should have filesystem operations"
    );
    assert!(
        metrics.database_time_ms > 0,
        "Should have database operations"
    );
}

#[tokio::test]
#[ignore] // Run manually after async refactor
async fn test_directory_adapter_timing_after_refactor() {
    // Initialize tracing capture
    let capture = init_tracing();

    // Create test fixture
    let fixture = TestFixture::new()
        .await
        .expect("Failed to create test fixture");

    let app_state = fixture.create_app_state();
    let request = fixture.create_request();

    // Create mock claims (admin role)
    use adapteros_db::users::{Claims, Role};
    let claims = Claims {
        sub: "test-user".to_string(),
        role: Role::Admin,
        tenant_id: Some("test-tenant".to_string()),
        exp: 9999999999,
        iat: 0,
    };

    // Execute the handler
    capture.clear();

    let result = adapteros_server_api::handlers::upsert_directory_adapter(
        State(app_state),
        Extension(claims),
        Json(request),
    )
    .await;

    assert!(result.is_ok(), "Handler should succeed");

    // Extract timing metrics
    let spans = capture.get_spans();
    let current_metrics = TimingMetrics::from_spans(&spans);

    // Load baseline
    let baseline_path = PathBuf::from("test_data/directory_adapter_baseline.json");
    let baseline_metrics = TimingMetrics::load_baseline(&baseline_path)
        .expect("Failed to load baseline - run test_directory_adapter_timing_baseline first");

    // Compare metrics
    let report = ImprovementReport::compare(baseline_metrics, current_metrics);
    report.print_report();

    // Assert improvements meet thresholds
    report.assert_improvements(
        20.0, // Minimum 20% improvement in total time
        5.0,  // FS/DB ratio should be ≤5.0 (ideally closer to 1.0-2.0)
    );

    println!("\n✓ Performance improvements verified!");
}

#[tokio::test]
async fn test_tracing_capture_basic() {
    // Test the tracing capture mechanism itself
    let capture = TracingCapture::new();
    let capture_layer = capture.clone();

    let subscriber = Registry::default().with(capture_layer);
    tracing::subscriber::with_default(subscriber, || {
        use tracing::info_span;

        {
            let _span = info_span!("test_span_1").entered();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        {
            let _span = info_span!("test_span_2").entered();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });

    let spans = capture.get_spans();

    assert_eq!(spans.len(), 2, "Should capture 2 spans");
    assert!(spans[0].name == "test_span_1", "First span name");
    assert!(spans[0].duration_ms >= 10, "First span duration");
    assert!(spans[1].name == "test_span_2", "Second span name");
    assert!(spans[1].duration_ms >= 20, "Second span duration");
}

#[test]
fn test_metrics_from_spans() {
    use helpers::SpanRecord;

    let spans = vec![
        SpanRecord {
            name: "upsert_directory_adapter_handler".to_string(),
            duration_ms: 550,
            fields: HashMap::new(),
        },
        SpanRecord {
            name: "directory_adapter_blocking_ops".to_string(),
            duration_ms: 500,
            fields: HashMap::new(),
        },
        SpanRecord {
            name: "db_get_adapter_check".to_string(),
            duration_ms: 30,
            fields: HashMap::new(),
        },
        SpanRecord {
            name: "db_register_adapter".to_string(),
            duration_ms: 20,
            fields: HashMap::new(),
        },
    ];

    let metrics = TimingMetrics::from_spans(&spans);

    assert_eq!(metrics.total_handler_time_ms, 550);
    assert_eq!(metrics.filesystem_time_ms, 500);
    assert_eq!(metrics.database_time_ms, 50);
    assert!((metrics.fs_db_ratio - 10.0).abs() < 0.1);
}
