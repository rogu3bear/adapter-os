# Quick Start - AOS 2.0 Test Suite

## Running Tests

### Run everything
```bash
cargo test -p adapteros-aos
```

### Run just the core tests (fastest)
```bash
cargo test -p adapteros-aos --tests --lib
```

### Run specific test file
```bash
# Unit tests (15 tests)
cargo test -p adapteros-aos --test aos_v2_parser_tests

# Fixture tests (12 tests)
cargo test -p adapteros-aos --test fixtures_tests

# Integration tests (10 tests)
cargo test -p adapteros-aos --test integration_tests
```

### Run with verbose output
```bash
cargo test -p adapteros-aos -- --nocapture
```

## Test Files Overview

| File | Purpose | Tests |
|------|---------|-------|
| `aos_v2_parser_tests.rs` | Unit tests for parser | 15 |
| `fixtures_tests.rs` | Tests with generated fixtures | 12 |
| `integration_tests.rs` | End-to-end integration tests | 10 |
| `fixture_generator.rs` | Helper to generate test data | 3 self-tests |

**Total:** 42 tests (all passing)

## What Gets Tested

### Happy Path ✅
- Reading valid .aos files
- Parsing headers correctly
- Extracting manifests (JSON)
- Extracting weights (safetensors)
- Large files (1MB+)
- Multiple files concurrently

### Error Cases ✅
- Corrupted files
- Invalid headers
- Missing data
- Wrong versions
- Files that don't exist

## Test Data

All test fixtures are **generated programmatically** - no external files needed!

The `fixture_generator.rs` module creates:
- Valid .aos files
- Corrupted files (bad data)
- Files with wrong versions
- Files with invalid headers
- Files with missing manifests
- Empty weight files
- Large files (1MB)

## Quick Examples

### Test a specific scenario
```bash
# Test header parsing
cargo test -p adapteros-aos test_header_parsing_valid

# Test error handling
cargo test -p adapteros-aos test_corrupted_manifest_json

# Test large files
cargo test -p adapteros-aos test_large_fixture_performance
```

### Watch for test failures
```bash
cargo watch -x "test -p adapteros-aos"
```

## File Structure

```
tests/
├── README.md                    # Full documentation
├── TEST_SUMMARY.md              # Test results summary
├── QUICK_START.md              # This file
├── aos_v2_parser_tests.rs      # Unit tests (427 lines)
├── fixture_generator.rs         # Fixture generator (216 lines)
├── fixtures_tests.rs            # Fixture tests (257 lines)
├── integration_tests.rs         # Integration tests (241 lines)
└── fixtures/                    # Empty (fixtures generated in-memory)
```

## Test Principles

1. **Fast** - All tests run in < 100ms
2. **Isolated** - Each test is independent
3. **Deterministic** - Same input = same output
4. **Clear** - Descriptive names and messages
5. **Comprehensive** - Success and failure cases

## Common Test Patterns

### Create a test .aos file
```rust
use fixture_generator::{generate_valid_aos, TestManifest};

let temp_dir = TempDir::new()?;
let path = temp_dir.path().join("test.aos");
generate_valid_aos(&path)?;
```

### Parse header
```rust
use adapteros_aos::aos2_writer::AOS2Writer;

let (manifest_offset, manifest_len) = AOS2Writer::read_header(&path)?;
```

### Extract manifest
```rust
let mut file = File::open(&path)?;
let mut buffer = vec![0u8; manifest_offset as usize + manifest_len as usize];
file.read_exact(&mut buffer)?;

let manifest_bytes = &buffer[manifest_offset as usize..];
let manifest: TestManifest = serde_json::from_slice(manifest_bytes)?;
```

## Debugging Tests

### Run single test with output
```bash
cargo test -p adapteros-aos test_header_parsing_valid -- --nocapture
```

### Run tests in single thread
```bash
cargo test -p adapteros-aos -- --test-threads=1
```

### Show test execution time
```bash
cargo test -p adapteros-aos -- --show-output
```

## Expected Output

```
running 15 tests
test test_header_parsing_valid ... ok
test test_manifest_extraction ... ok
test test_safetensors_extraction ... ok
...
test result: ok. 15 passed; 0 failed
```

## Adding New Tests

1. Choose the right file:
   - Unit test → `aos_v2_parser_tests.rs`
   - Fixture test → `fixtures_tests.rs`
   - Integration test → `integration_tests.rs`

2. Add test function:
```rust
#[test]
fn test_my_new_feature() -> Result<()> {
    // Setup
    let path = ...;

    // Execute
    let result = ...;

    // Verify
    assert!(result.is_ok(), "Should succeed");

    Ok(())
}
```

3. Run it:
```bash
cargo test -p adapteros-aos test_my_new_feature
```

## Need Help?

- **Full docs:** See `tests/README.md`
- **Results:** See `tests/TEST_SUMMARY.md`
- **Code:** Look at existing tests for examples

## Status: ✅ All Tests Passing

```
Library Tests:      2 ✅
Unit Tests:        15 ✅
Fixture Tests:     12 ✅
Integration Tests: 10 ✅
Generator Tests:    3 ✅
━━━━━━━━━━━━━━━━━━━━━━
Total:             42 ✅
```

**Test suite is production-ready!**
