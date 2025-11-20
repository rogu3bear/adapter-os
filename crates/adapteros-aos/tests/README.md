# AOS 2.0 Format Test Suite

This directory contains comprehensive tests for the AOS 2.0 archive format parser and writer.

## Test Files

### `aos_v2_parser_tests.rs`
Unit tests for the AOS 2.0 parser focusing on header parsing, manifest extraction, and error handling.

**Tests:**
- `test_header_parsing_valid` - Verify correct header parsing with valid data
- `test_header_parsing_invalid_too_small` - Handle incomplete headers gracefully
- `test_header_parsing_empty_file` - Handle empty files
- `test_manifest_extraction` - Extract and parse manifest JSON correctly
- `test_safetensors_extraction` - Extract weights section correctly
- `test_corrupted_manifest_json` - Detect and handle corrupted JSON
- `test_wrong_version_in_manifest` - Parse manifests with incorrect version
- `test_oversized_manifest_offset` - Handle offsets beyond file size
- `test_zero_manifest_length` - Handle zero-length manifests
- `test_multiple_archives_same_session` - Multiple independent archives
- `test_large_manifest` - Handle large manifests and weights
- `test_nonexistent_file` - Handle missing files
- `test_header_little_endian_encoding` - Verify little-endian encoding
- `test_minimal_valid_archive` - Minimal valid archive (empty weights)
- `test_roundtrip_consistency` - Write and read back with full consistency

### `fixtures_tests.rs`
Tests using pre-generated fixture files covering various error conditions.

**Tests:**
- `test_valid_fixture_loads` - Valid archive loads successfully
- `test_corrupted_fixture_detected` - Corrupted data is detected
- `test_invalid_header_fixture` - Invalid header fails gracefully
- `test_missing_manifest_fixture` - Missing manifest is detected
- `test_empty_weights_fixture` - Empty weights section handled
- `test_large_fixture_performance` - Large files parse efficiently
- `test_wrong_version_fixture` - Wrong version is detected
- `test_all_fixtures_exist` - All expected fixtures are generated
- `test_fixture_file_sizes` - Fixture sizes are as expected

### `integration_tests.rs`
Full end-to-end integration tests for complete workflows.

**Tests:**
- `test_full_roundtrip_write_read` - Complete write/read cycle
- `test_multiple_archives_independent` - Multiple archives don't interfere
- `test_error_recovery` - Recovery from errors
- `test_concurrent_reads` - Thread-safe concurrent reading
- `test_archive_validation_workflow` - Multi-step validation process
- `test_archive_size_limits` - Size limit handling
- `test_deterministic_writes` - Deterministic output for same input

### `fixture_generator.rs`
Helper module to generate test fixtures programmatically.

**Functions:**
- `generate_valid_aos()` - Create valid test archive
- `generate_corrupted_aos()` - Create corrupted archive
- `generate_wrong_version_aos()` - Create archive with wrong version
- `generate_invalid_header_aos()` - Create archive with invalid header
- `generate_missing_manifest_aos()` - Create archive with missing manifest
- `generate_empty_weights_aos()` - Create archive with empty weights
- `generate_large_aos()` - Create large (1MB) test archive

## Running Tests

### Run all tests
```bash
cargo test -p adapteros-aos
```

### Run specific test file
```bash
cargo test -p adapteros-aos --test aos_v2_parser_tests
cargo test -p adapteros-aos --test fixtures_tests
cargo test -p adapteros-aos --test integration_tests
```

### Run with output
```bash
cargo test -p adapteros-aos -- --nocapture
```

### Run specific test
```bash
cargo test -p adapteros-aos test_header_parsing_valid
```

## Test Coverage

The test suite covers:

### Valid Cases
- ✅ Valid header parsing
- ✅ Valid manifest extraction
- ✅ Valid safetensors extraction
- ✅ Empty weights (minimal archive)
- ✅ Large archives (1MB+)
- ✅ Multiple archives
- ✅ Concurrent reads

### Error Cases
- ✅ Corrupted files
- ✅ Invalid headers (too small, malformed)
- ✅ Empty files
- ✅ Corrupted JSON manifests
- ✅ Wrong version numbers
- ✅ Oversized offsets (beyond file size)
- ✅ Zero-length manifests
- ✅ Missing manifests
- ✅ Nonexistent files

### Properties
- ✅ Little-endian encoding
- ✅ Roundtrip consistency (write then read)
- ✅ Deterministic writes (same input = same output)
- ✅ Thread safety (concurrent reads)
- ✅ Independence (multiple archives don't interfere)
- ✅ Performance (large files parse quickly)

## AOS 2.0 Format Specification

The tests validate compliance with the AOS 2.0 format:

```
[0-3]    manifest_offset (u32, little-endian)
[4-7]    manifest_len (u32, little-endian)
[8...]   weights (safetensors format)
[offset] manifest (JSON)
```

### Header (8 bytes)
- Bytes 0-3: `manifest_offset` (u32 LE) - offset to manifest JSON
- Bytes 4-7: `manifest_len` (u32 LE) - length of manifest JSON

### Weights Section
- Starts at byte 8
- Variable length
- Safetensors binary format

### Manifest Section
- Starts at `manifest_offset`
- Length is `manifest_len`
- JSON format with required fields:
  - `version`: "2.0"
  - `adapter_id`: unique identifier
  - `rank`: adapter rank (u32)
  - `base_model`: base model name
  - `created_at`: ISO 8601 timestamp

## Test Fixtures

All fixtures are generated programmatically via `fixture_generator.rs` to ensure:
- Consistency across test runs
- No external dependencies
- Deterministic test data
- Easy reproduction

## Adding New Tests

To add a new test:

1. Choose the appropriate test file:
   - Unit tests → `aos_v2_parser_tests.rs`
   - Fixture-based tests → `fixtures_tests.rs`
   - End-to-end tests → `integration_tests.rs`

2. Add test function with `#[test]` attribute

3. Use helper functions from `fixture_generator.rs` to create test data

4. Follow existing naming conventions:
   - `test_<what>_<condition>` (e.g., `test_header_parsing_valid`)

5. Add clear assertions with descriptive messages

6. Document the test in this README

## Test Principles

All tests follow these principles:

1. **Isolated** - Each test is independent
2. **Deterministic** - Same input always produces same result
3. **Fast** - Tests run quickly (< 100ms each)
4. **Clear** - Descriptive names and messages
5. **Comprehensive** - Cover happy paths and error cases
6. **Documented** - README explains what each test does

## Dependencies

Test dependencies are declared in `Cargo.toml`:
- `tempfile` - Temporary files and directories
- `serde`/`serde_json` - JSON parsing
- Standard library - File I/O, threading

No external test fixtures or data files are required.
