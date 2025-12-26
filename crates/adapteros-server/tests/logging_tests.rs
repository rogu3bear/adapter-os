//! Tests for the logging module's cleanup_old_logs functionality.

use std::fs::File;
use std::io::Write;
use std::time::{Duration, SystemTime};
use tempfile::TempDir;

/// Helper to create a file with a specific age (by modifying mtime)
async fn create_file_with_age(dir: &std::path::Path, name: &str, age_days: u64) {
    let file_path = dir.join(name);
    let mut file = File::create(&file_path).expect("Failed to create test file");
    file.write_all(b"test content")
        .expect("Failed to write test content");
    drop(file);

    // Set modification time to the past
    let past_time = SystemTime::now() - Duration::from_secs(age_days * 86400 + 100);
    filetime::set_file_mtime(&file_path, filetime::FileTime::from_system_time(past_time))
        .expect("Failed to set file mtime");
}

/// Helper to create a recent file (uses current time)
async fn create_recent_file(dir: &std::path::Path, name: &str) {
    let file_path = dir.join(name);
    let mut file = File::create(&file_path).expect("Failed to create test file");
    file.write_all(b"test content")
        .expect("Failed to write test content");
}

#[tokio::test]
async fn test_cleanup_old_logs_removes_files_older_than_retention() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let log_dir = temp_dir.path();

    // Create files older than 7 days (retention period)
    create_file_with_age(log_dir, "old_log_1.log", 10).await;
    create_file_with_age(log_dir, "old_log_2.log", 15).await;
    create_file_with_age(log_dir, "old_log_3.log", 30).await;

    let retention_days = 7;
    let deleted_count =
        adapteros_server::logging::cleanup_old_logs(log_dir.to_str().unwrap(), retention_days)
            .await
            .expect("cleanup_old_logs should succeed");

    assert_eq!(deleted_count, 3, "Should delete all 3 old files");

    // Verify files are gone
    let entries: Vec<_> = std::fs::read_dir(log_dir)
        .expect("Failed to read dir")
        .collect();
    assert_eq!(entries.len(), 0, "Directory should be empty after cleanup");
}

#[tokio::test]
async fn test_cleanup_old_logs_preserves_recent_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let log_dir = temp_dir.path();

    // Create a mix of old and recent files
    create_file_with_age(log_dir, "old_log.log", 10).await;
    create_recent_file(log_dir, "recent_log_1.log").await;
    create_recent_file(log_dir, "recent_log_2.log").await;
    create_file_with_age(log_dir, "borderline.log", 3).await; // Within retention

    let retention_days = 7;
    let deleted_count =
        adapteros_server::logging::cleanup_old_logs(log_dir.to_str().unwrap(), retention_days)
            .await
            .expect("cleanup_old_logs should succeed");

    assert_eq!(deleted_count, 1, "Should only delete the 10-day old file");

    // Verify recent files remain
    let entries: Vec<_> = std::fs::read_dir(log_dir)
        .expect("Failed to read dir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 3, "Should have 3 files remaining");

    let names: Vec<String> = entries
        .iter()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"recent_log_1.log".to_string()));
    assert!(names.contains(&"recent_log_2.log".to_string()));
    assert!(names.contains(&"borderline.log".to_string()));
}

#[tokio::test]
async fn test_cleanup_old_logs_handles_empty_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let log_dir = temp_dir.path();

    // Directory exists but is empty
    let retention_days = 7;
    let deleted_count =
        adapteros_server::logging::cleanup_old_logs(log_dir.to_str().unwrap(), retention_days)
            .await
            .expect("cleanup_old_logs should succeed");

    assert_eq!(deleted_count, 0, "Should return 0 for empty directory");
}

#[tokio::test]
async fn test_cleanup_old_logs_handles_missing_directory() {
    // Use a path that doesn't exist
    let non_existent_path = "/tmp/non_existent_log_dir_12345_test";

    // Ensure it doesn't exist
    let _ = std::fs::remove_dir_all(non_existent_path);

    let retention_days = 7;
    let deleted_count =
        adapteros_server::logging::cleanup_old_logs(non_existent_path, retention_days)
            .await
            .expect("cleanup_old_logs should succeed for missing directory");

    assert_eq!(
        deleted_count, 0,
        "Should return 0 for non-existent directory"
    );
}
