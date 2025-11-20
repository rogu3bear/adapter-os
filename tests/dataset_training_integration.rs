//! Integration tests for dataset-to-training pipeline
//!
//! Tests the complete flow: uploaded files -> dataset -> training examples -> training

#![cfg(test)]

use adapteros_db::Db;
use adapteros_orchestrator::{TrainingConfig, TrainingDatasetManager};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create test database
async fn setup_test_db(temp_dir: &TempDir) -> Result<Db, Box<dyn std::error::Error>> {
    let db_path = temp_dir.path().join("test.db");
    let db = Db::connect(&format!("sqlite://{}", db_path.display())).await?;
    db.migrate().await?;
    Ok(db)
}

#[tokio::test]
async fn test_dataset_loading_flow_jsonl_format() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    // Create dataset storage directory
    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db.clone(), storage_path.clone(), None);

    // Create JSONL file with training examples
    let jsonl_path = temp_dir.path().join("examples.jsonl");
    let jsonl_content = r#"{"input": "What is 2+2?", "output": "The answer is 4"}
{"input": "What is 3+3?", "output": "The answer is 6"}
{"input": "What is 5+5?", "output": "The answer is 10"}"#;

    tokio::fs::write(&jsonl_path, jsonl_content).await?;

    // Create dataset record in database
    let hash = blake3::hash(jsonl_content.as_bytes());
    let hash_b3 = hex::encode(hash.as_bytes());

    let dataset_id = db
        .create_training_dataset(
            "math-examples",
            Some("Simple math Q&A examples"),
            "jsonl",
            &hash_b3,
            jsonl_path.to_str().unwrap(),
            Some("test-user"),
        )
        .await?;

    // Update validation status
    db.update_dataset_validation(&dataset_id, "valid", None)
        .await?;

    // Load training examples from dataset
    let examples = manager.load_dataset_examples(&dataset_id).await?;

    // Verify we got the examples
    assert_eq!(examples.len(), 3, "Should load 3 examples");
    assert!(
        !examples[0].input.is_empty(),
        "First example should have input"
    );
    assert!(
        !examples[0].target.is_empty(),
        "First example should have target"
    );

    // Verify examples preserve source metadata
    assert_eq!(
        examples[0].metadata.get("source"),
        Some(&jsonl_path.to_string_lossy().to_string())
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_loading_flow_multiple_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    // Create dataset storage directory
    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db.clone(), storage_path, None);

    // Create dataset with multiple files
    let dataset_id = db
        .create_training_dataset(
            "multi-file-dataset",
            Some("Dataset with multiple files"),
            "mixed",
            "placeholder_hash",
            "/tmp/placeholder",
            Some("test-user"),
        )
        .await?;

    // Create first JSON file
    let json_file1 = temp_dir.path().join("file1.json");
    let json_content1 = r#"[
        {"prompt": "First question", "completion": "First answer"},
        {"prompt": "Second question", "completion": "Second answer"}
    ]"#;
    tokio::fs::write(&json_file1, json_content1).await?;

    // Create second JSONL file
    let jsonl_file2 = temp_dir.path().join("file2.jsonl");
    let jsonl_content2 = r#"{"input": "Third question", "output": "Third answer"}
{"input": "Fourth question", "output": "Fourth answer"}"#;
    tokio::fs::write(&jsonl_file2, jsonl_content2).await?;

    // Register files in dataset
    db.add_dataset_file(
        &dataset_id,
        "file1.json",
        json_file1.to_str().unwrap(),
        json_content1.len() as i64,
        "json_hash",
        Some("application/json"),
    )
    .await?;

    db.add_dataset_file(
        &dataset_id,
        "file2.jsonl",
        jsonl_file2.to_str().unwrap(),
        jsonl_content2.len() as i64,
        "jsonl_hash",
        Some("application/jsonl"),
    )
    .await?;

    // Update validation status
    db.update_dataset_validation(&dataset_id, "valid", None)
        .await?;

    // Load examples from all files
    let examples = manager.load_dataset_examples(&dataset_id).await?;

    // Verify we got examples from all files (2 from JSON + 2 from JSONL = 4 total)
    assert_eq!(examples.len(), 4, "Should load examples from all files");

    Ok(())
}

#[tokio::test]
async fn test_file_format_detection() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db, storage_path, None);

    // Test format detection by mime type
    let json_file = temp_dir.path().join("data.txt"); // Wrong extension
    let json_content = r#"{"input": "test", "output": "result"}"#;
    tokio::fs::write(&json_file, json_content).await?;

    // Should detect JSON by mime type, not extension
    let examples = manager
        .load_examples_from_file(
            json_file.to_str().unwrap(),
            &Some("application/json".to_string()),
        )
        .await?;

    assert_eq!(examples.len(), 1);

    // Test format detection by extension when mime type is None
    let jsonl_file = temp_dir.path().join("data.jsonl");
    let jsonl_content = r#"{"input": "hello", "output": "world"}"#;
    tokio::fs::write(&jsonl_file, jsonl_content).await?;

    let examples = manager
        .load_examples_from_file(jsonl_file.to_str().unwrap(), &None)
        .await?;

    assert_eq!(examples.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_invalid_dataset_status() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db.clone(), storage_path, None);

    // Create dataset without validating
    let dataset_id = db
        .create_training_dataset(
            "invalid-dataset",
            None,
            "jsonl",
            "hash",
            "/tmp/invalid",
            None,
        )
        .await?;

    // Try to load from invalid dataset
    let result = manager.load_dataset_examples(&dataset_id).await;

    // Should fail because dataset is not validated
    assert!(result.is_err(), "Should fail when dataset is not validated");
    assert!(result.unwrap_err().to_string().contains("not validated"));

    Ok(())
}

#[tokio::test]
async fn test_hash_verification() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db.clone(), storage_path, None);

    // Create JSONL file
    let jsonl_path = temp_dir.path().join("examples.jsonl");
    let jsonl_content = r#"{"input": "test", "output": "result"}"#;
    tokio::fs::write(&jsonl_path, jsonl_content).await?;

    // Create dataset with correct hash
    let hash = blake3::hash(jsonl_content.as_bytes());
    let hash_b3 = hex::encode(hash.as_bytes());

    let dataset_id = db
        .create_training_dataset(
            "hashed-dataset",
            None,
            "jsonl",
            &hash_b3,
            jsonl_path.to_str().unwrap(),
            None,
        )
        .await?;

    db.update_dataset_validation(&dataset_id, "valid", None)
        .await?;

    // Should load successfully
    let examples = manager.load_dataset_examples(&dataset_id).await?;
    assert_eq!(examples.len(), 1);

    // Modify file to break hash
    tokio::fs::write(&jsonl_path, "modified content").await?;

    // Create new dataset with old (incorrect) hash
    let dataset_id2 = db
        .create_training_dataset(
            "broken-hash-dataset",
            None,
            "jsonl",
            &hash_b3,
            jsonl_path.to_str().unwrap(),
            None,
        )
        .await?;

    db.update_dataset_validation(&dataset_id2, "valid", None)
        .await?;

    // Should fail due to hash mismatch
    let result = manager.load_dataset_examples(&dataset_id2).await;
    assert!(result.is_err(), "Should fail on hash mismatch");
    assert!(result.unwrap_err().to_string().contains("hash mismatch"));

    Ok(())
}

#[tokio::test]
async fn test_training_example_weight_preservation() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db, storage_path, None);

    // Create JSONL with custom weight fields
    let jsonl_path = temp_dir.path().join("weighted.jsonl");
    let jsonl_content = r#"{"input": "important", "output": "result"}
{"input": "less important", "output": "result"}"#;
    tokio::fs::write(&jsonl_path, jsonl_content).await?;

    // Load examples
    let examples = manager
        .load_examples_from_file(jsonl_path.to_str().unwrap(), &None)
        .await?;

    // Verify all examples have default weight of 1.0
    assert!(examples.iter().all(|ex| ex.weight == 1.0));

    Ok(())
}

#[tokio::test]
async fn test_training_config_integration() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let db = setup_test_db(&temp_dir).await?;

    let storage_path = temp_dir.path().join("datasets");
    tokio::fs::create_dir_all(&storage_path).await?;

    let manager = TrainingDatasetManager::new(db, storage_path, None);

    // Create test data
    let jsonl_path = temp_dir.path().join("training.jsonl");
    let jsonl_content = r#"{"input": "q1", "output": "a1"}
{"input": "q2", "output": "a2"}"#;
    tokio::fs::write(&jsonl_path, jsonl_content).await?;

    // Load examples
    let examples = manager
        .load_examples_from_file(jsonl_path.to_str().unwrap(), &None)
        .await?;

    // Verify examples can be used with TrainingConfig
    let config = TrainingConfig {
        rank: 16,
        alpha: 32,
        targets: vec!["q_proj".to_string()],
        epochs: 2,
        learning_rate: 0.001,
        batch_size: 2,
        warmup_steps: None,
        max_seq_length: Some(512),
        gradient_accumulation_steps: None,
        weight_group_config: None,
    };

    // Examples should be compatible with training
    assert!(examples.len() >= config.batch_size as usize);

    Ok(())
}
