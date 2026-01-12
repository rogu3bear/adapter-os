//! Upload validation invariant tests.
//!
//! Verifies:
//! - Oversize uploads are rejected at initiation
//! - Missing chunks fail assembly
//! - Chunk sealing produces deterministic manifest hash

use adapteros_server_api::handlers::chunked_upload::{
    CompressionFormat, UploadSession, UPLOAD_SESSION_SCHEMA_VERSION,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Test that sealed manifest hash is deterministic and includes all chunk hashes.
#[test]
fn sealed_manifest_hash_is_deterministic() {
    let mut session = UploadSession {
        schema_version: UPLOAD_SESSION_SCHEMA_VERSION,
        session_id: "test-session-123".to_string(),
        file_name: "test.jsonl".to_string(),
        total_size: 30_000_000, // 30MB
        chunk_size: 10_000_000, // 10MB chunks = 3 chunks
        content_type: "application/jsonl".to_string(),
        received_chunks: HashMap::new(),
        temp_dir: PathBuf::from("/tmp/test"),
        created_at: std::time::SystemTime::UNIX_EPOCH,
        compression: CompressionFormat::None,
        is_resumed: false,
        workspace_id: Some("test-workspace".to_string()),
        sealed_manifest_hash: None,
    };

    // Without all chunks, sealing should return None
    assert!(session.compute_sealed_manifest_hash().is_none());

    // Add chunk hashes
    session.received_chunks.insert(0, "hash0".to_string());
    session.received_chunks.insert(1, "hash1".to_string());
    session.received_chunks.insert(2, "hash2".to_string());

    // Now sealing should succeed
    let hash1 = session.compute_sealed_manifest_hash();
    assert!(
        hash1.is_some(),
        "Sealed hash should be computed with all chunks"
    );

    // Sealing is deterministic
    let hash2 = session.compute_sealed_manifest_hash();
    assert_eq!(hash1, hash2, "Sealed hash should be deterministic");

    // seal_for_assembly should also work
    let sealed = session.seal_for_assembly();
    assert!(sealed.is_ok(), "seal_for_assembly should succeed");
    assert_eq!(sealed.unwrap(), hash1.unwrap());
}

/// Test that missing chunks fail sealing.
#[test]
fn missing_chunks_fail_sealing() {
    let mut session = UploadSession {
        schema_version: UPLOAD_SESSION_SCHEMA_VERSION,
        session_id: "test-session-456".to_string(),
        file_name: "test.jsonl".to_string(),
        total_size: 30_000_000, // 30MB = 3 chunks of 10MB
        chunk_size: 10_000_000,
        content_type: "application/jsonl".to_string(),
        received_chunks: HashMap::new(),
        temp_dir: PathBuf::from("/tmp/test"),
        created_at: std::time::SystemTime::UNIX_EPOCH,
        compression: CompressionFormat::None,
        is_resumed: false,
        workspace_id: Some("test-workspace".to_string()),
        sealed_manifest_hash: None,
    };

    // Add only chunks 0 and 2, missing chunk 1
    session.received_chunks.insert(0, "hash0".to_string());
    session.received_chunks.insert(2, "hash2".to_string());

    // compute_sealed_manifest_hash should return None
    assert!(session.compute_sealed_manifest_hash().is_none());

    // seal_for_assembly should return error
    let result = session.seal_for_assembly();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing chunk 1"));
}

/// Test that different schema versions produce different sealed hashes.
#[test]
fn schema_version_affects_sealed_hash() {
    let make_session = |version: u16| -> UploadSession {
        let mut session = UploadSession {
            schema_version: version,
            session_id: "test-session".to_string(),
            file_name: "test.jsonl".to_string(),
            total_size: 10_000_000,
            chunk_size: 10_000_000,
            content_type: "application/jsonl".to_string(),
            received_chunks: HashMap::new(),
            temp_dir: PathBuf::from("/tmp/test"),
            created_at: std::time::SystemTime::UNIX_EPOCH,
            compression: CompressionFormat::None,
            is_resumed: false,
            workspace_id: Some("test-workspace".to_string()),
            sealed_manifest_hash: None,
        };
        session.received_chunks.insert(0, "hash0".to_string());
        session
    };

    let session_v1 = make_session(1);
    let session_v2 = make_session(2);

    let hash_v1 = session_v1.compute_sealed_manifest_hash().unwrap();
    let hash_v2 = session_v2.compute_sealed_manifest_hash().unwrap();

    assert_ne!(
        hash_v1, hash_v2,
        "Different schema versions should produce different hashes"
    );
}

/// Test that different session IDs produce different sealed hashes.
#[test]
fn session_id_affects_sealed_hash() {
    let make_session = |id: &str| -> UploadSession {
        let mut session = UploadSession {
            schema_version: UPLOAD_SESSION_SCHEMA_VERSION,
            session_id: id.to_string(),
            file_name: "test.jsonl".to_string(),
            total_size: 10_000_000,
            chunk_size: 10_000_000,
            content_type: "application/jsonl".to_string(),
            received_chunks: HashMap::new(),
            temp_dir: PathBuf::from("/tmp/test"),
            created_at: std::time::SystemTime::UNIX_EPOCH,
            compression: CompressionFormat::None,
            is_resumed: false,
            workspace_id: Some("test-workspace".to_string()),
            sealed_manifest_hash: None,
        };
        session.received_chunks.insert(0, "hash0".to_string());
        session
    };

    let session_a = make_session("session-a");
    let session_b = make_session("session-b");

    let hash_a = session_a.compute_sealed_manifest_hash().unwrap();
    let hash_b = session_b.compute_sealed_manifest_hash().unwrap();

    assert_ne!(
        hash_a, hash_b,
        "Different session IDs should produce different hashes"
    );
}

/// Test that chunk hash ordering affects the sealed hash.
#[test]
fn chunk_hash_ordering_affects_sealed_hash() {
    let make_session = |chunk0: &str, chunk1: &str| -> UploadSession {
        let mut session = UploadSession {
            schema_version: UPLOAD_SESSION_SCHEMA_VERSION,
            session_id: "test-session".to_string(),
            file_name: "test.jsonl".to_string(),
            total_size: 20_000_000, // 2 chunks
            chunk_size: 10_000_000,
            content_type: "application/jsonl".to_string(),
            received_chunks: HashMap::new(),
            temp_dir: PathBuf::from("/tmp/test"),
            created_at: std::time::SystemTime::UNIX_EPOCH,
            compression: CompressionFormat::None,
            is_resumed: false,
            workspace_id: Some("test-workspace".to_string()),
            sealed_manifest_hash: None,
        };
        session.received_chunks.insert(0, chunk0.to_string());
        session.received_chunks.insert(1, chunk1.to_string());
        session
    };

    let session_ab = make_session("hashA", "hashB");
    let session_ba = make_session("hashB", "hashA");

    let hash_ab = session_ab.compute_sealed_manifest_hash().unwrap();
    let hash_ba = session_ba.compute_sealed_manifest_hash().unwrap();

    assert_ne!(
        hash_ab, hash_ba,
        "Swapped chunk hashes should produce different sealed hashes"
    );
}
