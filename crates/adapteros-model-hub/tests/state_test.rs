//! Integration tests for state persistence

use adapteros_model_hub::state::{DownloadState, FileDownloadState, StateManager};
use std::time::Duration;

#[tokio::test]
async fn test_state_persistence() {
    let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
    let manager = StateManager::new(temp_dir.path().to_path_buf());

    let mut state = DownloadState::new(
        "test-model".to_string(),
        "org/model".to_string(),
        "main".to_string(),
    );

    state.add_file(FileDownloadState::new(
        "model.safetensors".to_string(),
        "https://example.com/model.safetensors".to_string(),
        1000,
        temp_dir
            .path()
            .join("model.partial")
            .to_string_lossy()
            .to_string(),
        "/models/model.safetensors".to_string(),
    ));

    // Save
    manager.save_state(&state).await.unwrap();

    // Load
    let loaded = manager.load_state("test-model").await.unwrap();
    assert!(loaded.is_some());

    let loaded_state = loaded.unwrap();
    assert_eq!(loaded_state.model_id, state.model_id);
    assert_eq!(loaded_state.files.len(), 1);
}

#[tokio::test]
async fn test_crash_recovery() {
    let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
    let manager = StateManager::new(temp_dir.path().to_path_buf());

    // Simulate incomplete download
    let mut state1 = DownloadState::new(
        "incomplete1".to_string(),
        "org/incomplete1".to_string(),
        "main".to_string(),
    );
    state1.add_file(FileDownloadState::new(
        "model.bin".to_string(),
        "https://example.com/model.bin".to_string(),
        1000,
        temp_dir
            .path()
            .join("model.partial")
            .to_string_lossy()
            .to_string(),
        "/models/model.bin".to_string(),
    ));
    manager.save_state(&state1).await.unwrap();

    // Simulate complete download
    let mut state2 = DownloadState::new(
        "complete".to_string(),
        "org/complete".to_string(),
        "main".to_string(),
    );
    state2.add_file(FileDownloadState::new(
        "model.bin".to_string(),
        "https://example.com/model.bin".to_string(),
        1000,
        temp_dir
            .path()
            .join("model.partial")
            .to_string_lossy()
            .to_string(),
        "/models/model.bin".to_string(),
    ));
    state2
        .complete_file("model.bin", "hash123".to_string())
        .unwrap();
    manager.save_state(&state2).await.unwrap();

    // List incomplete downloads for recovery
    let incomplete = manager.list_incomplete_downloads().await.unwrap();
    assert_eq!(incomplete.len(), 1);
    assert_eq!(incomplete[0].model_id, "incomplete1");
}

#[tokio::test]
async fn test_stale_cleanup() {
    let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
    let manager = StateManager::new(temp_dir.path().to_path_buf());

    // Create old state
    let mut old_state =
        DownloadState::new("old".to_string(), "org/old".to_string(), "main".to_string());
    old_state.last_activity = chrono::Utc::now() - chrono::Duration::days(10);
    manager.save_state(&old_state).await.unwrap();

    // Create recent state
    let recent = DownloadState::new(
        "recent".to_string(),
        "org/recent".to_string(),
        "main".to_string(),
    );
    manager.save_state(&recent).await.unwrap();

    // Cleanup states older than 7 days
    let deleted = manager
        .cleanup_stale_states(Duration::from_secs(7 * 24 * 3600))
        .await
        .unwrap();

    assert_eq!(deleted, 1);
    assert!(manager.load_state("old").await.unwrap().is_none());
    assert!(manager.load_state("recent").await.unwrap().is_some());
}
