//! Test to apply migrations to a test database
//!
//! This test creates a test database in a temporary directory and applies
//! all migrations from the migrations/ directory. This is used to ensure
//! the database schema is up to date for SQLx compile-time query validation.

use adapteros_db::Db;
use tempfile::TempDir;

#[tokio::test]
async fn apply_all_migrations_to_test_database() {
    // Create temporary directory for test database (auto-cleanup on drop)
    let temp_dir = TempDir::with_prefix("aos-test-migrations-")
        .expect("Failed to create temporary directory for test database");

    // Test database path in temp directory
    let db_path = temp_dir.path().join("aos-cp-test.sqlite3");
    let db_path_str = db_path
        .to_str()
        .expect("Temporary database path should be valid UTF-8");

    println!("Creating test database at: {}", db_path_str);

    let db = Db::connect(db_path_str)
        .await
        .expect("Failed to connect to test database");

    println!("Applying migrations...");
    db.migrate().await.expect("Failed to apply migrations");

    println!("✓ All migrations applied successfully to: {}", db_path_str);
    println!("✓ Database ready for SQLx compile-time validation");

    // TempDir is dropped here, cleaning up the test database automatically
}
