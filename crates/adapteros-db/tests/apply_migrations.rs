//! Test to apply migrations to a test database
//!
//! This test creates a test database at var/aos-cp-test.sqlite3 and applies
//! all migrations from the migrations/ directory. This is used to ensure
//! the database schema is up to date for SQLx compile-time query validation.

use adapteros_db::Db;
use std::path::PathBuf;

#[tokio::test]
async fn apply_all_migrations_to_test_database() {
    // Ensure var directory exists
    let var_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap()
        .join("var");

    std::fs::create_dir_all(&var_dir).expect("Failed to create var directory");

    // Test database path
    let db_path = var_dir.join("aos-cp-test.sqlite3");

    // Remove existing database to start fresh
    if db_path.exists() {
        std::fs::remove_file(&db_path).expect("Failed to remove existing test database");
        println!("Removed existing test database at: {}", db_path.display());
    }

    // Create database and apply migrations
    let db_path_str = db_path.to_str().expect("Invalid database path");
    println!("Creating test database at: {}", db_path_str);

    let db = Db::connect(db_path_str)
        .await
        .expect("Failed to connect to test database");

    println!("Applying migrations...");
    db.migrate()
        .await
        .expect("Failed to apply migrations");

    println!("✓ All migrations applied successfully to: {}", db_path_str);
    println!("✓ Database ready for SQLx compile-time validation");
}
