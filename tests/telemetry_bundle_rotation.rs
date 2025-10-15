//! Test telemetry bundle rotation and signing

use adapteros_telemetry::BundleWriter;
use tempfile::TempDir;

#[test]
fn test_bundle_creation() {
    let temp_dir = TempDir::new().unwrap();
    let writer = BundleWriter::new(temp_dir.path(), 100, 1024 * 1024).unwrap();

    assert!(writer.public_key().len() > 0);
}

#[test]
fn test_event_writing() {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = BundleWriter::new(temp_dir.path(), 100, 1024 * 1024).unwrap();

    // Write some test events
    for i in 0..10 {
        let event = serde_json::json!({
            "type": "test",
            "data": format!("event_{}", i)
        });
        writer.write_event(&event).unwrap();
    }

    // Verify bundle file was created
    let bundle_files: Vec<_> = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "ndjson")
                .unwrap_or(false)
        })
        .collect();

    assert_eq!(bundle_files.len(), 1);
}

#[test]
fn test_bundle_rotation_at_threshold() {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = BundleWriter::new(temp_dir.path(), 10, 1024 * 1024).unwrap();

    // Write exactly 10 events (threshold)
    for i in 0..10 {
        let event = serde_json::json!({
            "type": "test",
            "data": format!("event_{}", i)
        });
        writer.write_event(&event).unwrap();
    }

    // Write one more event to trigger rotation
    let event = serde_json::json!({
        "type": "test",
        "data": "trigger_rotation"
    });
    writer.write_event(&event).unwrap();

    // Force rotation
    writer.flush().unwrap();

    // Verify signature file was created
    let sig_files: Vec<_> = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "sig")
                .unwrap_or(false)
        })
        .collect();

    assert!(sig_files.len() >= 1, "Expected at least one .sig file");
}

#[test]
fn test_signature_format() {
    let temp_dir = TempDir::new().unwrap();
    let mut writer = BundleWriter::new(temp_dir.path(), 5, 1024 * 1024).unwrap();

    // Write events and force rotation
    for i in 0..5 {
        let event = serde_json::json!({
            "type": "test",
            "data": format!("event_{}", i)
        });
        writer.write_event(&event).unwrap();
    }
    writer.flush().unwrap();

    // Find and parse signature file
    let sig_file = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s == "sig")
                .unwrap_or(false)
        })
        .unwrap();

    let sig_content = std::fs::read_to_string(sig_file.path()).unwrap();
    let sig_json: serde_json::Value = serde_json::from_str(&sig_content).unwrap();

    // Verify signature metadata fields
    assert!(sig_json["merkle_root"].is_string());
    assert!(sig_json["signature"].is_string());
    assert!(sig_json["public_key"].is_string());
    assert!(sig_json["event_count"].is_number());
    assert!(sig_json["timestamp"].is_number());

    assert_eq!(sig_json["event_count"].as_u64().unwrap(), 5);
}

#[test]
fn test_merkle_root_determinism() {
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();

    let mut writer1 = BundleWriter::new(temp_dir1.path(), 100, 1024 * 1024).unwrap();
    let mut writer2 = BundleWriter::new(temp_dir2.path(), 100, 1024 * 1024).unwrap();

    // Write identical events to both writers
    for i in 0..5 {
        let event = serde_json::json!({
            "type": "test",
            "index": i,
            "data": "deterministic_data"
        });
        writer1.write_event(&event).unwrap();
        writer2.write_event(&event).unwrap();
    }

    writer1.flush().unwrap();
    writer2.flush().unwrap();

    // Read signature files
    let read_merkle_root = |dir: &TempDir| -> String {
        let sig_file = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| {
                e.path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s == "sig")
                    .unwrap_or(false)
            })
            .unwrap();

        let sig_content = std::fs::read_to_string(sig_file.path()).unwrap();
        let sig_json: serde_json::Value = serde_json::from_str(&sig_content).unwrap();
        sig_json["merkle_root"].as_str().unwrap().to_string()
    };

    let merkle1 = read_merkle_root(&temp_dir1);
    let merkle2 = read_merkle_root(&temp_dir2);

    // Merkle roots should be identical for identical events
    assert_eq!(merkle1, merkle2);
}


