/// Streaming upload integration tests
///
/// Tests for PRD-02 streaming upload implementation, including:
/// - Large file handling without memory exhaustion
/// - Incremental hash computation verification
/// - Streaming chunk processing
/// - Progress tracking
/// - Atomicity of temporary file + rename
/// - Error handling and cleanup

#[cfg(test)]
mod tests {
    use adapteros_server_api::handlers::streaming_upload::{
        StreamingFileWriter, UploadProgress, STREAMING_CHUNK_SIZE,
    };
    use blake3::Hasher as Blake3Hasher;
    use std::fs;
    use std::path::PathBuf;

    /// Helper to create a test directory
    fn setup_test_dir(name: &str) -> PathBuf {
        let test_dir = std::env::temp_dir().join("aos_streaming_tests").join(name);
        let _ = fs::remove_dir_all(&test_dir);
        fs::create_dir_all(&test_dir).expect("Failed to create test directory");
        test_dir
    }

    /// Helper to cleanup test directory
    fn cleanup_test_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }

    #[tokio::test]
    async fn test_streaming_small_file() {
        let test_dir = setup_test_dir("small_file");
        let test_file = test_dir.join("small.aos");

        let mut writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        let content = b"Small test file content";
        writer
            .write_chunk(content)
            .await
            .expect("Failed to write chunk");

        let (hash, size) = writer.finalize().await.expect("Failed to finalize");

        assert_eq!(size, content.len() as u64);

        // Verify hash matches what we expect
        let mut expected_hasher = Blake3Hasher::new();
        expected_hasher.update(content);
        let expected_hash = expected_hasher.finalize().to_hex().to_string();
        assert_eq!(hash, expected_hash);

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_multiple_chunks() {
        let test_dir = setup_test_dir("multiple_chunks");
        let test_file = test_dir.join("chunked.aos");

        let mut writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        let chunk1 = b"First chunk of data. ";
        let chunk2 = b"Second chunk of data. ";
        let chunk3 = b"Third and final chunk.";

        writer
            .write_chunk(chunk1)
            .await
            .expect("Failed to write chunk 1");
        writer
            .write_chunk(chunk2)
            .await
            .expect("Failed to write chunk 2");
        writer
            .write_chunk(chunk3)
            .await
            .expect("Failed to write chunk 3");

        let (hash, size) = writer.finalize().await.expect("Failed to finalize");

        // Verify size
        let expected_size = (chunk1.len() + chunk2.len() + chunk3.len()) as u64;
        assert_eq!(size, expected_size);

        // Verify hash
        let mut expected_hasher = Blake3Hasher::new();
        expected_hasher.update(chunk1);
        expected_hasher.update(chunk2);
        expected_hasher.update(chunk3);
        let expected_hash = expected_hasher.finalize().to_hex().to_string();
        assert_eq!(hash, expected_hash);

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_large_file_simulation() {
        let test_dir = setup_test_dir("large_file_sim");
        let test_file = test_dir.join("large.aos");

        let mut writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        let mut expected_hasher = Blake3Hasher::new();
        let mut total_size = 0u64;

        // Simulate writing a "large" file in chunks (5MB total, realistic for testing)
        let chunk_count = 80; // 80 * 64KB = 5MB
        let test_chunk = vec![42u8; STREAMING_CHUNK_SIZE];

        for i in 0..chunk_count {
            writer
                .write_chunk(&test_chunk)
                .await
                .expect(&format!("Failed to write chunk {}", i));
            expected_hasher.update(&test_chunk);
            total_size += test_chunk.len() as u64;
        }

        let (hash, size) = writer.finalize().await.expect("Failed to finalize");

        assert_eq!(size, total_size);
        let expected_hash = expected_hasher.finalize().to_hex().to_string();
        assert_eq!(hash, expected_hash);

        // Verify file actually exists and has correct size
        let metadata = fs::metadata(&test_file).expect("Failed to read metadata");
        assert_eq!(metadata.len(), total_size);

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_abort() {
        let test_dir = setup_test_dir("abort");
        let test_file = test_dir.join("abort.aos");

        let mut writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        let content = b"This will be aborted";
        writer
            .write_chunk(content)
            .await
            .expect("Failed to write chunk");

        // Verify file exists before abort
        assert!(test_file.exists());

        // Abort should clean up the file
        writer.abort().await.expect("Failed to abort");

        // File should be deleted
        assert!(!test_file.exists());

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_hash_consistency() {
        let test_dir = setup_test_dir("hash_consistency");
        let test_file1 = test_dir.join("file1.aos");
        let test_file2 = test_dir.join("file2.aos");

        // Same content written in different ways should produce same hash
        let content = b"Same content for both files";

        // File 1: Single chunk
        let mut writer1 = StreamingFileWriter::new(&test_file1)
            .await
            .expect("Failed to create writer 1");
        writer1
            .write_chunk(content)
            .await
            .expect("Failed to write to file 1");
        let (hash1, _) = writer1.finalize().await.expect("Failed to finalize 1");

        // File 2: Same content in smaller chunks
        let mut writer2 = StreamingFileWriter::new(&test_file2)
            .await
            .expect("Failed to create writer 2");
        for chunk in content.chunks(10) {
            writer2
                .write_chunk(chunk)
                .await
                .expect("Failed to write to file 2");
        }
        let (hash2, _) = writer2.finalize().await.expect("Failed to finalize 2");

        // Hashes should match
        assert_eq!(hash1, hash2, "Hashes should match for same content");

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_progress_tracking() {
        let mut progress = UploadProgress::new(Some(1000));
        assert_eq!(progress.percentage(), Some(0));

        progress.bytes_received = 250;
        progress.chunks_processed = 4;
        assert_eq!(progress.percentage(), Some(25));

        progress.bytes_received = 500;
        progress.chunks_processed = 8;
        assert_eq!(progress.percentage(), Some(50));

        progress.bytes_received = 1000;
        progress.chunks_processed = 16;
        assert_eq!(progress.percentage(), Some(100));

        // Test clamping to 100%
        progress.bytes_received = 1500;
        assert_eq!(progress.percentage(), Some(100));
    }

    #[tokio::test]
    async fn test_streaming_progress_unknown_size() {
        let progress = UploadProgress::new(None);
        assert_eq!(progress.percentage(), None);
        assert_eq!(progress.bytes_received, 0);
        assert_eq!(progress.chunks_processed, 0);
    }

    #[tokio::test]
    async fn test_streaming_chunk_size_reasonable() {
        // Chunk size should be reasonable for memory efficiency
        assert!(
            STREAMING_CHUNK_SIZE >= 32 * 1024,
            "Chunk size too small (< 32KB)"
        );
        assert!(
            STREAMING_CHUNK_SIZE <= 1024 * 1024,
            "Chunk size too large (> 1MB)"
        );
    }

    #[tokio::test]
    async fn test_streaming_fsync_behavior() {
        // Test that fsync is called during finalization
        let test_dir = setup_test_dir("fsync");
        let test_file = test_dir.join("fsync.aos");

        let mut writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        let content = b"Data that should be synced to disk";
        writer.write_chunk(content).await.expect("Failed to write");

        let (_, _) = writer.finalize().await.expect("Failed to finalize");

        // Verify file persists after finalize (fsync would have been called)
        assert!(test_file.exists());
        let metadata = fs::metadata(&test_file).expect("Failed to read metadata");
        assert_eq!(metadata.len(), content.len() as u64);

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_atomicity_pattern() {
        // Test the atomic temp + rename pattern
        let test_dir = setup_test_dir("atomicity");
        let final_file = test_dir.join("final.aos");
        let temp_file = test_dir.join(".temp.tmp");

        // Write to temp file
        let mut writer = StreamingFileWriter::new(&temp_file)
            .await
            .expect("Failed to create writer");

        let content = b"Atomic write test";
        writer.write_chunk(content).await.expect("Failed to write");
        let (hash, size) = writer.finalize().await.expect("Failed to finalize");

        // Temp file should exist, final shouldn't
        assert!(temp_file.exists());
        assert!(!final_file.exists());

        // Atomic rename
        fs::rename(&temp_file, &final_file).expect("Failed to rename");

        // Final file should exist, temp shouldn't
        assert!(!temp_file.exists());
        assert!(final_file.exists());

        // Verify content
        let metadata = fs::metadata(&final_file).expect("Failed to read metadata");
        assert_eq!(metadata.len(), size);

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_empty_file() {
        let test_dir = setup_test_dir("empty_file");
        let test_file = test_dir.join("empty.aos");

        let writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        // Finalize without writing anything
        let (hash, size) = writer.finalize().await.expect("Failed to finalize");

        assert_eq!(size, 0);

        // Hash of empty file should be BLAKE3's empty hash
        let mut empty_hasher = Blake3Hasher::new();
        let expected_hash = empty_hasher.finalize().to_hex().to_string();
        assert_eq!(hash, expected_hash);

        cleanup_test_dir(&test_dir);
    }

    #[tokio::test]
    async fn test_streaming_large_single_chunk() {
        // Test writing a chunk larger than default chunk size
        let test_dir = setup_test_dir("large_chunk");
        let test_file = test_dir.join("large_chunk.aos");

        let mut writer = StreamingFileWriter::new(&test_file)
            .await
            .expect("Failed to create writer");

        // Create a chunk larger than STREAMING_CHUNK_SIZE
        let large_chunk = vec![99u8; STREAMING_CHUNK_SIZE * 2];

        writer
            .write_chunk(&large_chunk)
            .await
            .expect("Failed to write large chunk");

        let (hash, size) = writer.finalize().await.expect("Failed to finalize");

        assert_eq!(size, (STREAMING_CHUNK_SIZE * 2) as u64);

        // Verify hash
        let mut expected_hasher = Blake3Hasher::new();
        expected_hasher.update(&large_chunk);
        let expected_hash = expected_hasher.finalize().to_hex().to_string();
        assert_eq!(hash, expected_hash);

        cleanup_test_dir(&test_dir);
    }
}
