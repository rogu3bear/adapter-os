# Type Validation Test Suite

A comprehensive schema validation framework for the adapterOS Control Plane API that ensures type compatibility across the full stack: Rust → JSON → TypeScript.

## Quick Links

- **New to the suite?** Start with [QUICK_START.md](QUICK_START.md) (5 min read)
- **Need details?** Read [TYPE_VALIDATION_SUITE.md](TYPE_VALIDATION_SUITE.md) (30 min read)
- **Want an overview?** See [TYPE_VALIDATION_DELIVERABLES.md](TYPE_VALIDATION_DELIVERABLES.md) (10 min read)
- **Looking for navigation?** Check [TYPE_VALIDATION_INDEX.md](TYPE_VALIDATION_INDEX.md) (reference)

## Overview

This test suite validates that API types work correctly across three critical layers:

1. **Round-Trip Serialization** (19 tests)
   - Rust → JSON → Rust without data loss
   - Field naming consistency
   - Type precision preservation

2. **OpenAPI Schema Compatibility** (9 tests)
   - Required field presence
   - Correct field types
   - Nested structure validation
   - Schema compliance

3. **Frontend Type Compatibility** (12 tests)
   - TypeScript interface alignment
   - Field naming (snake_case)
   - Type serialization correctness
   - Response structure patterns

## File Structure

```
tests/
├── type_validation/
│   ├── mod.rs                       # Test utilities (143 lines)
│   ├── round_trip.rs                # Round-trip tests (532 lines, 19 tests)
│   ├── openapi_compat.rs            # OpenAPI tests (530 lines, 9 tests)
│   └── frontend_compat.rs           # Frontend tests (582 lines, 12 tests)
├── type_validation_integration.rs   # Integration test (117 lines)
├── README_TYPE_VALIDATION.md        # This file
├── QUICK_START.md                   # Quick reference (250 lines)
├── TYPE_VALIDATION_SUITE.md         # Full guide (446 lines)
├── TYPE_VALIDATION_DELIVERABLES.md  # Project summary (348 lines)
└── TYPE_VALIDATION_INDEX.md         # Navigation guide (280 lines)
```

**Total: 2,698 lines (1,904 test code + 1,324 documentation)**

## Quick Start

### Run All Tests
```bash
cargo test --test type_validation_integration
```

### Run by Category
```bash
cargo test round_trip::        # Serialization tests
cargo test openapi_compat::    # OpenAPI tests
cargo test frontend_compat::   # Frontend tests
```

### Run Specific Test
```bash
cargo test test_infer_response_round_trip
cargo test test_field_names_use_snake_case
```

## Test Coverage

### By Type (13+ types)
- InferResponse/Request
- InferenceTrace
- RouterDecision
- BatchInferRequest/Response
- AdapterResponse/Manifest
- ErrorResponse
- HealthResponse
- ModelRuntimeHealth
- PaginationParams
- PaginatedResponse
- And more...

### By Validation Layer
| Layer | Tests | Coverage |
|-------|-------|----------|
| Round-Trip | 19 | Serialization round-trips |
| OpenAPI | 9 | Schema compliance |
| Frontend | 12 | TypeScript compatibility |
| **Total** | **40+** | **Comprehensive** |

## Key Features

### 1. Comprehensive Validation
Multi-layer testing catches issues at serialization, schema, and frontend levels.

### 2. Reusable Utilities
```rust
validate_snake_case_fields(&json_obj)     // Check all fields
is_valid_snake_case(&field_name)          // Validate single field
validate_field_type(&value, "string")     // Type checking
validate_required_fields(&obj, &fields)   // Field presence
```

### 3. Well-Documented
30+ inline examples, troubleshooting guides, and pattern templates.

### 4. Easy to Extend
Clear patterns make it simple to add tests for new types.

## Adding a New Type Test

### Step 1: Round-Trip Test
Add to `/tests/type_validation/round_trip.rs`:

```rust
#[tokio::test]
async fn test_my_type_round_trip() {
    let original = MyType { field: "value" };
    let json = serde_json::to_value(&original)?;
    let deserialized: MyType = serde_json::from_value(json)?;
    assert_eq!(original, deserialized);
}
```

### Step 2: OpenAPI Test
Add to `/tests/type_validation/openapi_compat.rs`:

```rust
#[tokio::test]
async fn test_my_type_openapi_compatible() {
    let value = MyType { field: "value" };
    let json = serde_json::to_value(&value)?;
    assert_has_required_fields(&json, &["field"]);
    assert!(json.get("field").unwrap().is_string());
}
```

### Step 3: Frontend Test
Add to `/tests/type_validation/frontend_compat.rs`:

```rust
#[tokio::test]
async fn test_my_type_frontend_compatible() {
    let value = MyType { field: "value" };
    let json = serde_json::to_value(&value)?;
    let violations = validate_all_fields_snake_case(&json, "MyType");
    assert!(violations.is_empty());
}
```

## Field Naming Rules

All fields must use **snake_case**:

✅ Valid:
- `user_id`
- `token_count`
- `input_tokens`
- `_private_field`

❌ Invalid:
- `userId` (camelCase)
- `UserID` (PascalCase)
- `user-id` (kebab-case)

Use serde rename if needed:
```rust
#[serde(rename = "user_id")]
pub user_id: String,
```

## Common Patterns

### Testing Optional Fields
```rust
// None - field omitted
let with_none = MyType { field: None };
assert!(!json.get("field").is_some());

// Some - field present
let with_some = MyType { field: Some("value") };
assert!(json.get("field").is_some());
```

### Testing Arrays
```rust
let items = vec![1, 2, 3];
assert!(json.get("items").unwrap().is_array());
assert_eq!(json.get("items").unwrap().as_array().unwrap().len(), 3);
```

### Testing Type Precision
```rust
let original = MyType { value: 3.14159 };
let json = serde_json::to_value(&original)?;
let deserialized: MyType = serde_json::from_value(json)?;
assert!((original.value - deserialized.value).abs() < 1e-10);
```

## Troubleshooting

### "Field is not in snake_case"
**Solution:** Rename field or add `#[serde(rename = "field_name")]`

### "Type mismatch: expected X, got Y"
**Solution:** Check field serialization type or use custom serializer

### "Required field missing"
**Solution:** Remove `#[serde(skip_serializing_if)]` or ensure field is set

See [QUICK_START.md](QUICK_START.md) for more troubleshooting.

## Documentation

| Document | Purpose | Length | Read Time |
|----------|---------|--------|-----------|
| [QUICK_START.md](QUICK_START.md) | Quick reference | 250 lines | 5 min |
| [TYPE_VALIDATION_SUITE.md](TYPE_VALIDATION_SUITE.md) | Full guide | 446 lines | 30 min |
| [TYPE_VALIDATION_DELIVERABLES.md](TYPE_VALIDATION_DELIVERABLES.md) | Project summary | 348 lines | 10 min |
| [TYPE_VALIDATION_INDEX.md](TYPE_VALIDATION_INDEX.md) | Navigation | 280 lines | Reference |

## Statistics

| Metric | Value |
|--------|-------|
| Test Functions | 40+ |
| Types Covered | 13+ |
| Test Code | 1,904 lines |
| Documentation | 1,324 lines |
| Code Examples | 30+ |
| Test Utilities | 4 |

## Integration

Add to CI/CD pipeline:
```bash
# Run validation tests
cargo test --test type_validation_integration

# Or run specific modules
cargo test round_trip::
```

## Design Principles

1. **Comprehensive Coverage** - Multiple validation layers
2. **Reusable Utilities** - Avoid duplication
3. **Clear Documentation** - Context for every test
4. **Maintainability** - Organized by concern
5. **Developer Experience** - Easy to extend

## Success Metrics

- 40+ test functions
- 13+ API types covered
- 3 validation layers
- 30+ code examples
- Production-grade quality

## Next Steps

1. **Run tests:** `cargo test --test type_validation_integration`
2. **Read guide:** See [QUICK_START.md](QUICK_START.md)
3. **Add tests:** Follow patterns in test files
4. **Integrate:** Add to CI/CD pipeline

## Support

For questions or issues:
1. Check [QUICK_START.md](QUICK_START.md) troubleshooting section
2. Review [TYPE_VALIDATION_SUITE.md](TYPE_VALIDATION_SUITE.md)
3. Look at examples in test files

---

**Created:** 2024
**Status:** Complete and documented
**Quality:** Production-grade test infrastructure
