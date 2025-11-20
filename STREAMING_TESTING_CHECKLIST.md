# Streaming Upload Testing Checklist

## PRD-02: Large File Streaming Implementation

### Test Status Overview

- [ ] Unit Tests (Streaming Module)
- [ ] Integration Tests (Handler)
- [ ] Performance Tests (Memory/CPU)
- [ ] Load Tests (Concurrency)
- [ ] Edge Cases
- [ ] Error Scenarios
- [ ] Security Tests
- [ ] Documentation Tests

---

## 1. Unit Tests: StreamingFileWriter

Location: `crates/adapteros-server-api/src/handlers/streaming_upload.rs`

### Core Functionality

- [x] **test_streaming_writer_creation**
  - Create new streaming writer
  - Verify file handle opens
  - Check temp file exists
  - Status: IMPLEMENTED

- [x] **test_streaming_write_chunks**
  - Write multiple chunks sequentially
  - Verify all data written
  - Check chunk order preserved
  - Status: IMPLEMENTED

- [x] **test_streaming_hash_incremental**
  - Write in chunks
  - Compute streaming hash
  - Compare with batch hash
  - Verify equivalence
  - Status: IMPLEMENTED

### Edge Cases

- [x] **test_streaming_empty_file**
  - Finalize without writing
  - Verify hash of empty content
  - Check size = 0
  - Status: IMPLEMENTED

- [x] **test_streaming_large_single_chunk**
  - Write chunk > 64KB
  - Handle oversized input
  - Verify correct hashing
  - Status: IMPLEMENTED

### Error Handling

- [x] **test_streaming_abort**
  - Create writer
  - Write data
  - Call abort()
  - Verify temp file deleted
  - Status: IMPLEMENTED

- [x] **test_streaming_fsync_behavior**
  - Verify fsync called on finalize
  - Check file persists
  - Verify metadata correct
  - Status: IMPLEMENTED

### Progress Tracking

- [x] **test_upload_progress_percentage**
  - Progress with known size
  - Percentage calculation 0-100
  - Clamping to 100%
  - Status: IMPLEMENTED

- [x] **test_upload_progress_unknown_size**
  - Progress with None size
  - Verify None percentage
  - Check bytes tracked
  - Status: IMPLEMENTED

### Atomicity

- [x] **test_streaming_atomicity_pattern**
  - Write to temp file
  - Atomic rename
  - Verify states before/after
  - Status: IMPLEMENTED

---

## 2. Integration Tests: Upload Handler

Location: `crates/adapteros-server-api/tests/streaming_upload_test.rs`

### Basic Upload Flow

- [ ] **test_streaming_small_file**
  - Upload 100 byte file
  - Verify response status 200
  - Check adapter_id in response
  - Verify hash matches
  - Check file exists
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_streaming_medium_file**
  - Upload 10MB file
  - Verify streaming worked
  - Check memory stayed constant
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_streaming_large_file**
  - Upload 100MB file
  - Monitor memory usage
  - Verify completion
  - Status: READY FOR IMPLEMENTATION

### Multipart Processing

- [ ] **test_multipart_field_extraction**
  - Parse multipart fields
  - Verify adapter_name extracted
  - Check tier/category/scope
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_multipart_with_metadata**
  - Upload with rank/alpha
  - Verify values in response
  - Check database record
  - Status: READY FOR IMPLEMENTATION

### Database Integration

- [ ] **test_database_registration**
  - Upload file
  - Verify adapter record created
  - Check hash stored
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_database_collision_handling**
  - Multiple rapid uploads
  - Verify UUID collision retry
  - Check all registered correctly
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_database_rollback_on_error**
  - Upload succeeds
  - Database fails
  - Verify file deleted
  - Check no orphaned adapter
  - Status: READY FOR IMPLEMENTATION

### Atomic Operations

- [ ] **test_atomic_rename**
  - Upload file
  - Verify temp → final rename
  - Check no partial files
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_atomic_database_insert**
  - File on disk
  - Database insert fails
  - Verify cleanup occurs
  - Status: READY FOR IMPLEMENTATION

---

## 3. Error Scenarios

### File Size Limits

- [ ] **test_file_too_large**
  - Upload > 1GB
  - Detect at chunk boundary
  - Return 413 Payload Too Large
  - Verify temp cleaned up
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_size_limit_per_chunk**
  - Send chunks that sum > 1GB
  - Verify limit checked per chunk
  - Prevent gradual growth
  - Status: READY FOR IMPLEMENTATION

### Network Errors

- [ ] **test_interrupted_upload**
  - Start upload
  - Simulate network error mid-stream
  - Verify cleanup
  - Check recovery possible
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_slow_network**
  - Throttle upload speed
  - Verify streaming works
  - Check progress updates
  - Status: READY FOR IMPLEMENTATION

### Disk Errors

- [ ] **test_disk_full**
  - Fill disk during write
  - Catch I/O error
  - Clean up temp file
  - Return 500
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_permission_denied**
  - No write access to ./adapters
  - Catch permission error
  - Return 500
  - Status: READY FOR IMPLEMENTATION

### Validation Errors

- [ ] **test_invalid_file_extension**
  - Upload non-.aos file
  - Return 400 Bad Request
  - No file saved
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_invalid_tier_value**
  - Upload with invalid tier
  - Return 400 Bad Request
  - No adapter registered
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_invalid_rank_bounds**
  - Upload with rank > 512
  - Return 400 Bad Request
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_invalid_alpha_bounds**
  - Upload with alpha > 100
  - Return 400 Bad Request
  - Status: READY FOR IMPLEMENTATION

---

## 4. Concurrency & Load Tests

### Concurrent Uploads

- [ ] **test_concurrent_5_uploads**
  - 5 simultaneous uploads
  - Each 100MB
  - Monitor memory
  - Verify all succeed
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_concurrent_10_uploads**
  - 10 simultaneous uploads
  - Each 50MB
  - Verify memory stays bounded
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_max_concurrent_uploads**
  - Push system limit
  - Verify rate limiting
  - Check 429 responses
  - Status: READY FOR IMPLEMENTATION

### Rate Limiting

- [ ] **test_rate_limit_per_tenant**
  - Tenant A: multiple uploads
  - Hits rate limit
  - Returns 429
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_rate_limit_reset**
  - Hit rate limit
  - Wait for window to reset
  - Upload succeeds again
  - Status: READY FOR IMPLEMENTATION

### Stress Tests

- [ ] **test_1gb_file_stream**
  - Upload 1GB file
  - Verify memory ~128KB
  - Complete successfully
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_many_small_uploads**
  - Upload 1000 × 1MB files
  - Verify completion rate
  - Check no memory leaks
  - Status: READY FOR IMPLEMENTATION

---

## 5. Hash & Verification Tests

### Hash Computation

- [ ] **test_hash_consistency**
  - Same content → same hash
  - Different content → different hash
  - BLAKE3 determinism verified
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_hash_matches_direct_blake3**
  - Stream file
  - Compute BLAKE3 directly
  - Verify identical hashes
  - Status: READY FOR IMPLEMENTATION

### Verification Strategy

- [ ] **test_size_verification**
  - Upload file
  - Verify metadata size
  - Check against streamed size
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_no_full_file_reread**
  - Upload large file
  - Verify no full re-read in logs
  - Status: READY FOR IMPLEMENTATION

---

## 6. Progress Tracking Tests

### Progress Reporting

- [ ] **test_progress_every_mb**
  - Upload file
  - Verify logs every 1MB
  - Check progress structure
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_progress_percentage**
  - Upload with Content-Length
  - Calculate percentage
  - Verify 0-100 range
  - Status: READY FOR IMPLEMENTATION

---

## 7. Security Tests

### Path Traversal Prevention

- [ ] **test_path_traversal_temp**
  - Attempt ../../../ in temp path
  - Verify normalize_path() rejects
  - No path traversal possible
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_path_traversal_final**
  - Attempt traversal in final path
  - Verify rejection
  - File created in correct dir
  - Status: READY FOR IMPLEMENTATION

### DoS Prevention

- [ ] **test_chunk_size_dos**
  - Send millions of tiny chunks
  - Verify rate limiting
  - CPU bounded
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_large_chunk_dos**
  - Send huge chunks
  - Verify per-chunk processing
  - Memory bounded
  - Status: READY FOR IMPLEMENTATION

### Input Validation

- [ ] **test_missing_file_field**
  - Upload without file
  - Return 400
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_missing_adapter_name**
  - Upload without name
  - Verify default generated
  - Status: READY FOR IMPLEMENTATION

### Cleanup on Failure

- [ ] **test_cleanup_on_permission_error**
  - Upload with bad adapter_name length
  - Verify no temp file left
  - Status: READY FOR IMPLEMENTATION

- [ ] **test_cleanup_on_db_error**
  - Upload succeeds
  - DB insert fails
  - Verify file cleaned up
  - Status: READY FOR IMPLEMENTATION

---

## 8. Documentation Tests

### API Documentation

- [x] **docs/STREAMING_UPLOAD.md**
  - Architecture explained
  - Implementation details
  - Configuration guide
  - Status: COMPLETE

- [x] **STREAMING_IMPLEMENTATION_SUMMARY.md**
  - Overview of changes
  - Key achievements
  - Integration points
  - Status: COMPLETE

- [x] **examples/streaming_upload_example.rs**
  - Client-side usage example
  - Server flow conceptually
  - Memory profile shown
  - Status: COMPLETE

---

## 9. Performance Verification

### Memory Profile

- [ ] **Verify 64KB chunk size**
  - Check STREAMING_CHUNK_SIZE
  - Confirm balanced tradeoff
  - Status: READY

- [ ] **Measure memory per upload**
  - 1GB upload
  - Peak memory < 1MB
  - Constant throughout
  - Status: READY

- [ ] **Measure concurrent memory**
  - 5 × 1GB concurrent
  - Total memory < 5MB
  - Status: READY

### Speed Profile

- [ ] **Hash computation speed**
  - 1GB hash in < 1 second
  - Single pass
  - No bottleneck
  - Status: READY

- [ ] **Verification speed**
  - Metadata check < 10ms
  - Not O(n)
  - Status: READY

---

## 10. Regression Tests

### API Compatibility

- [ ] **Same request format**
  - multipart/form-data works
  - All fields parsed
  - Status: READY

- [ ] **Same response format**
  - AosUploadResponse matches
  - All fields populated
  - Status: READY

- [ ] **Same error codes**
  - 400: Invalid request
  - 403: Forbidden
  - 413: Payload too large
  - 429: Rate limit
  - 500: Server error
  - Status: READY

### Database Compatibility

- [ ] **Same schema**
  - No migration needed
  - Existing queries work
  - Status: READY

- [ ] **Same adapter record**
  - Fields identical
  - Hash computed same way
  - Status: READY

---

## Test Execution Plan

### Phase 1: Unit Tests (Local)
```bash
cargo test -p adapteros-server-api handlers::streaming_upload
```
- Tests in streaming_upload.rs
- Estimated: 5-10 minutes
- No external dependencies

### Phase 2: Integration Tests
```bash
cargo test -p adapteros-server-api aos_upload::tests
```
- Tests in aos_upload_test.rs + streaming_upload_test.rs
- Requires database
- Estimated: 15-30 minutes

### Phase 3: Load Tests (Optional)
```bash
cargo test --release -- --nocapture --test-threads=1
```
- Concurrent upload tests
- Memory monitoring
- Estimated: 30+ minutes

### Phase 4: Manual Testing (QA)
- Upload 1GB file via web UI
- Verify progress updates
- Check logs
- Monitor memory

---

## Success Criteria

### Functional
- [x] Streaming write implemented
- [x] Incremental hashing works
- [x] Progress tracked
- [x] Atomicity maintained
- [ ] All tests pass

### Non-Functional
- [ ] Memory < 1MB per upload
- [ ] 5× concurrent 1GB: < 5MB memory
- [ ] Hash computation: single pass
- [ ] No file re-reads

### Security
- [ ] Path traversal prevented
- [ ] DoS prevented via limits
- [ ] Size enforcement per chunk
- [ ] Cleanup on all errors

### Compatibility
- [ ] API unchanged
- [ ] Database unchanged
- [ ] Error codes same
- [ ] Deployable immediately

---

## Known Issues & Limitations

1. **Resumable Uploads**: Not yet implemented
   - Tracked chunks would enable resume
   - Future enhancement

2. **Streaming Decompression**: Not yet implemented
   - Could decompress gzip during upload
   - Future enhancement

3. **Bandwidth Throttling**: Not yet implemented
   - Could rate-limit upload speed
   - Future enhancement

4. **Client Progress Webhooks**: Not yet implemented
   - Could notify client of progress
   - Future enhancement

---

## Test Environment Setup

### Requirements
- Rust 1.70+
- Tokio runtime
- SQLite database
- 2GB disk space

### Configuration
```env
DATABASE_URL=sqlite:///tmp/test.db
LOG_LEVEL=debug
UPLOAD_MAX_SIZE=1073741824  # 1GB
STREAMING_CHUNK_SIZE=65536  # 64KB
```

### Cleanup
```bash
rm -rf /tmp/aos_streaming_tests
rm /tmp/test.db
```

---

## Sign-Off

- [ ] All unit tests pass
- [ ] Integration tests complete
- [ ] Performance verified
- [ ] Security review passed
- [ ] Documentation reviewed
- [ ] QA sign-off
- [ ] Ready for production deployment

---

**Last Updated:** 2025-11-19
**Status:** IMPLEMENTATION COMPLETE, TESTING READY
