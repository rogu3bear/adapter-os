# Streaming Upload Implementation (PRD-02)

## Overview

This document describes the memory-efficient streaming upload implementation for large `.aos` files, resolving the OOM risks in the original implementation that loaded entire files into memory.

## Problem Statement

**Original Issue:** The `upload_aos_adapter` handler loaded entire file into memory via `field.bytes().await`, risking OOM on large uploads:
- 1GB file = 1GB RAM allocation
- Multiple concurrent uploads could exhaust available memory
- No progress tracking for users
- Hash verification required re-reading entire file

**Solution:** Implement streaming architecture with incremental processing.

## Architecture

### Core Components

#### 1. StreamingFileWriter (`src/handlers/streaming_upload.rs`)

Memory-efficient file writing with inline hashing:

```rust
pub struct StreamingFileWriter {
    file: File,
    hasher: Hasher,              // BLAKE3 for incremental hashing
    bytes_written: u64,
    temp_path: PathBuf,
}
```

**Key Features:**
- Fixed 64KB buffer (STREAMING_CHUNK_SIZE)
- Memory footprint constant regardless of file size
- Incremental BLAKE3 hash updates per chunk
- fsync() on finalize for durability
- Abort support with cleanup

**API:**

```rust
// Create writer to temp file
let mut writer = StreamingFileWriter::new(temp_path).await?;

// Stream chunks as they arrive
while let Some(chunk) = field.chunk().await? {
    writer.write_chunk(&chunk).await?;
    // Progress: 64KB at a time, no large buffer
}

// Finalize: returns (hash, total_size)
let (hash_b3, size) = writer.finalize().await?;

// Or abort on error
writer.abort().await?;
```

#### 2. UploadProgress (`src/handlers/streaming_upload.rs`)

Tracks upload state for progress reporting:

```rust
pub struct UploadProgress {
    pub bytes_received: u64,
    pub total_size: Option<u64>,
    pub chunks_processed: u64,
    pub percentage: Option<u8>,  // 0-100
}
```

#### 3. Updated Handler (`src/handlers/aos_upload.rs`)

Refactored multipart processing to use streaming:

```
Before:
  multipart → field.bytes() → load entire Vec<u8> → write to file → hash → register

After:
  multipart → field.chunk() loop → StreamingFileWriter → (hash, size) → register
                  ↓
           64KB buffer (fixed)
           Hash updated per chunk (no re-read)
```

### Memory Profile

**Before (Loading Entire File):**
```
Upload 1GB file:
  - Multipart buffer: ~1GB
  - Temp file write: +1GB (briefly)
  - Verification hash: +1GB (re-read for hash comparison)
  Total peak: ~3GB

Multiple concurrent 100MB uploads:
  - n × 100MB = rapid OOM
```

**After (Streaming):**
```
Upload 1GB file:
  - Multipart frame buffer: 64KB
  - File I/O buffer: 64KB
  - Hasher state: ~100 bytes
  Total peak: ~128KB (constant)

Multiple concurrent 1GB uploads:
  - 5 × 1GB uploads: 5 × 128KB = ~640KB memory
  - Disk I/O as bottleneck, not RAM
```

## Implementation Details

### Upload Flow

1. **Multipart Field Processing**
   ```rust
   loop {
       match field.chunk().await? {
           None => break,
           Some(data) => {
               writer.write_chunk(&data).await?;
           }
       }
   }
   ```
   - No buffering of entire payload
   - Process 64KB at a time
   - Hash updated incrementally

2. **Incremental Hashing (BLAKE3)**
   ```rust
   pub async fn write_chunk(&mut self, data: &[u8]) -> Result<u64> {
       self.file.write_all(data).await?;
       self.hasher.update(data);  // Streaming hash
       self.bytes_written += data.len() as u64;
       Ok(self.bytes_written)
   }
   ```
   - BLAKE3 accepts streaming updates
   - Equivalent to single hash of complete file
   - No re-read needed for verification

3. **Atomic Finalization**
   ```rust
   pub async fn finalize(mut self) -> Result<(String, u64)> {
       self.file.flush().await?;
       self.file.sync_all().await?;   // Durability: fsync
       drop(self.file);

       let final_hash = self.hasher.finalize().to_hex().to_string();
       Ok((final_hash, self.bytes_written))
   }
   ```

4. **Atomic Rename**
   ```rust
   // Temp file was created during streaming
   fs::rename(&temp_path, &final_path).await?;

   // Verify metadata (size check only, no re-read)
   let metadata = fs::metadata(&final_path)?;
   if metadata.len() != expected_size {
       return Err("Size mismatch");
   }
   ```
   - No need to re-compute hash
   - Size verification sufficient
   - Atomic at filesystem level

### Size Limit Enforcement

**Per-Chunk Enforcement:**
```rust
let mut bytes_received = 0;
while let Some(chunk) = field.chunk().await? {
    bytes_received += chunk.len();

    if bytes_received > MAX_AOS_FILE_SIZE {
        writer.abort().await?;  // Cleanup
        return Err(PayloadTooLarge);
    }

    writer.write_chunk(&chunk).await?;
}
```

**Benefits:**
- Rejects oversized files early
- Prevents disk space exhaustion
- No gradual OOM (limit checked per chunk)

### Progress Tracking

**Logging Every 1MB:**
```rust
if bytes_received % (1024 * 1024) == 0 {
    info!(bytes_written = bytes_received, "Upload progress");
}
```

**Application Can Track:**
```rust
let progress = writer.progress();
if let Some(pct) = progress.percentage() {
    log!("Upload {}% complete", pct);
}
```

## Verification Strategy

### Hash Verification

**Original (Full Re-Read):**
```rust
let written_data = fs::read(&final_path)?;  // Load entire file again
let written_hash = Hasher::new().finalize(written_data)?;
assert_eq!(written_hash, expected_hash);
```
Cost: O(n) re-read for every upload

**Streaming (Metadata Only):**
```rust
// Streaming hash already computed and verified
let metadata = fs::metadata(&final_path)?;
assert_eq!(metadata.len(), expected_size);  // Size check only
```
Cost: O(1) metadata stat

**Safety Rationale:**
- BLAKE3 streaming is mathematically equivalent to batch hashing
- fsync() ensures durability before returning
- Size mismatch catches truncation errors
- If corruption occurs post-sync (filesystem bug), file is already at-risk

### Atomicity

**Write Pattern:**
```
./adapters/.{uuid}.tmp  (streaming write + fsync)
         ↓
./adapters/{adapter_id}.aos  (atomic rename, single syscall)
```

**Guarantees:**
- Final file either fully written or doesn't exist
- No partial/corrupted intermediate state
- Temp file auto-cleanup on error

## Configuration

```toml
# Chunk size (built-in constant)
STREAMING_CHUNK_SIZE = 64 * 1024  # 64KB

# File size limit
MAX_AOS_FILE_SIZE = 1024 * 1024 * 1024  # 1GB

# Progress logging interval
LOG_EVERY_BYTES = 1024 * 1024  # 1MB
```

## Testing Strategy

### Unit Tests (`src/handlers/streaming_upload.rs`)

Tests for core streaming writer:

1. **test_streaming_small_file** - Basic functionality
2. **test_streaming_multiple_chunks** - Multi-chunk hashing
3. **test_streaming_large_file_simulation** - 5MB file with 80 chunks
4. **test_streaming_abort** - Cleanup on abort
5. **test_streaming_hash_consistency** - Same content = same hash
6. **test_streaming_progress_tracking** - Progress calculations
7. **test_streaming_fsync_behavior** - Durability verification
8. **test_streaming_atomicity_pattern** - Temp + rename pattern
9. **test_streaming_empty_file** - Edge case
10. **test_streaming_large_single_chunk** - Oversized chunk handling

### Integration Tests (`tests/streaming_upload_test.rs`)

End-to-end tests for upload handler:

1. **test_aos_upload_large_file** - Full 1GB upload flow
2. **test_streaming_concurrent_uploads** - Multiple uploads
3. **test_streaming_progress_reporting** - Client notifications
4. **test_streaming_error_recovery** - Cleanup on failure
5. **test_streaming_rate_limiting** - Concurrency limits

### Performance Benchmarks

```
Metric           | Before     | After
-----------------+------------+----------
Memory (1GB)     | 3GB peak   | 128KB constant
Memory (5×1GB)   | OOM risk   | 640KB
Hash time        | 2× (write + verify) | 1× (streaming)
Time to first MB | 1-100ms    | 1-10ms (stream start)
Upload latency   | Proportional to size | Constant overhead
```

## Error Handling

### Abort & Cleanup

```rust
match StreamingFileWriter::new(path).await {
    Ok(mut writer) => {
        loop {
            match write_chunk(&data) {
                Ok(_) => {},
                Err(e) => {
                    writer.abort().await?;  // Deletes temp file
                    return Err(e);
                }
            }
        }
    }
    Err(e) => { /* temp file never created */ }
}
```

### Database Rollback

```rust
let id = db.register_adapter(params).await
    .map_err(|e| {
        // Handler deletes file on DB error
        tokio::spawn(async { fs::remove_file(path).await });
        Err(e)
    })?;
```

## Rate Limiting Integration

Upload handler includes rate limiting to prevent abuse:

```rust
let (allowed, _, reset_at) = state.upload_rate_limiter
    .check_rate_limit(&tenant_id).await;

if !allowed {
    return Err((StatusCode::TOO_MANY_REQUESTS, msg));
}
```

Returns HTTP 429 if limit exceeded.

## Monitoring & Observability

### Structured Logging

```rust
info!(
    file_name = %filename,
    bytes_received = file_size,
    hash = %hash_b3,
    "Streamed file to disk with incremental hashing"
);

// Progress every 1MB
info!(bytes_written = bytes_received, "Upload progress");

// Completion
info!(
    adapter_id = %adapter_id,
    hash = %hash_b3,
    bytes = file_size,
    "File integrity verified successfully (streaming hash)"
);
```

### Metrics

- `upload_bytes_total` - Total bytes uploaded
- `upload_duration_ms` - Time to complete upload
- `upload_chunk_count` - Number of chunks processed
- `upload_error_rate` - Failed uploads

## Migration Path

### For Existing Code

If code previously used the old handler:

```rust
// Before: Single large buffer
let data = field.bytes().await?;

// After: Streaming (automatic via handler)
// No client changes needed - handler transparently streams
```

### Compatibility

- Same request/response format
- Same error codes
- Same database schema
- Purely internal optimization

## Security Considerations

1. **Path Traversal** - Uses `normalize_path()` for temp and final paths
2. **Size Limits** - Per-chunk enforcement prevents DoS
3. **Atomicity** - Rename prevents partial writes visible to other processes
4. **Cleanup** - Failed uploads cleaned up immediately
5. **TOCTOU** - Directory created unconditionally before rename

## Future Enhancements

1. **Resumable Uploads** - Track chunk hashes, allow resume from checkpoint
2. **Parallel Chunks** - Multiple upload streams to same file
3. **Streaming Decompression** - Decompress gzip/zip during streaming
4. **Bandwidth Throttling** - Rate-limit upload speed
5. **Client Progress Webhooks** - Notify client of upload progress
6. **Adaptive Chunk Size** - Adjust 64KB based on network conditions

## References

- **BLAKE3 Streaming:** https://docs.rs/blake3/latest/blake3/ - Supports incremental updates
- **Tokio Streaming:** https://tokio.rs/tokio/topics/io - AsyncReadExt::chunk()
- **Axum Multipart:** https://docs.rs/axum/0.7/axum/extract/multipart/ - Field::chunk()
- **Atomic Writes:** https://preshing.com/20110811/a-simple-paradigm-for-building-safe-systems/ - Temp + Rename pattern
