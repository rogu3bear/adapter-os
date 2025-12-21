# Type Validation Test Suite - Quick Start Guide

## What Is This?

A comprehensive testing framework that ensures API types work correctly across the full stack:
- **Rust types** → Serialize to JSON
- **JSON** → Deserialize back to Rust (round-trip)
- **JSON** → Matches OpenAPI schema
- **JSON** → Matches TypeScript interfaces

## File Structure

```
tests/
├── type_validation/
│   ├── mod.rs                      # Test utilities & exports
│   ├── round_trip.rs               # 19 serialization tests
│   ├── openapi_compat.rs           # 9 OpenAPI validation tests
│   └── frontend_compat.rs          # 12 frontend compatibility tests
├── type_validation_integration.rs  # Integration test file
├── TYPE_VALIDATION_SUITE.md        # Full documentation (446 lines)
├── TYPE_VALIDATION_DELIVERABLES.md # Project summary
└── QUICK_START.md                  # This file
```

**Total:** 2,700+ lines of tests and documentation

## Quick Commands

### Run All Tests
```bash
cargo test --test type_validation_integration
```

### Run Specific Test
```bash
cargo test test_infer_response_round_trip
cargo test test_field_names_use_snake_case
cargo test test_optional_fields_omitted
```

### Run by Module
```bash
cargo test round_trip::        # Serialization tests
cargo test openapi_compat::    # OpenAPI tests
cargo test frontend_compat::   # Frontend tests
```

### Show Test Output
```bash
cargo test --test type_validation_integration -- --nocapture
```

## What Gets Tested?

### Round-Trip Serialization (19 tests)
- Basic types (strings, numbers, booleans)
- Complex nested structures
- Optional fields
- Arrays and collections
- Numeric precision (f64, i64)
- Large payloads
- Snake_case field naming

**Key Test:**
```rust
let original = InferResponse { ... };
let json = serde_json::to_value(&original)?;
let deserialized: InferResponse = serde_json::from_value(json)?;
assert_eq!(original, deserialized);
```

### OpenAPI Schema Compatibility (9 tests)
- Required field presence
- Field type correctness
- Snake_case naming
- Nested object structure
- Array consistency
- Timestamp format (ISO 8601)
- Numeric precision

**Key Test:**
```rust
let json = serde_json::to_value(&response)?;
assert!(json.get("required_field").is_some());
assert!(json.get("field_a").unwrap().is_string());
```

### Frontend Type Compatibility (12 tests)
- Field naming (snake_case only)
- Type compatibility (string, number, bool, array)
- Optional field handling (omit vs. null)
- Pagination structure
- Response structure
- Timestamp consistency

**Key Test:**
```rust
let json = serde_json::to_value(&response)?;
let violations = validate_snake_case_fields(&json);
assert!(violations.is_empty());
```

## Using Test Utilities

### 1. Check Field Naming
```rust
use type_validation::validate_snake_case_fields;

let json = json!({"user_id": "123"});
let violations = validate_snake_case_fields(&json);
assert!(violations.is_empty());
```

### 2. Validate Single Field
```rust
use type_validation::is_valid_snake_case;

assert!(is_valid_snake_case("user_id"));
assert!(!is_valid_snake_case("userId"));  // ❌ camelCase
```

### 3. Validate Type
```rust
use type_validation::validate_field_type;

let value = json!("test");
validate_field_type(&value, "string").unwrap();
```

### 4. Check Required Fields
```rust
use type_validation::validate_required_fields;

let obj = json!({"id": "123"});
let missing = validate_required_fields(&obj, &["id", "name"]);
// missing = vec!["Missing required field: name"]
```

## Adding Tests for New Types

### Step 1: Create Round-Trip Test
Add to `/tests/type_validation/round_trip.rs`:

```rust
#[tokio::test]
async fn test_my_new_type_round_trip() {
    let original = MyNewType {
        field_a: "value".to_string(),
        field_b: 42,
    };

    let json = serde_json::to_value(&original)
        .expect("serialize failed");

    let deserialized: MyNewType = serde_json::from_value(json.clone())
        .expect("deserialize failed");

    assert_eq!(original.field_a, deserialized.field_a);
    assert_eq!(original.field_b, deserialized.field_b);
}
```

### Step 2: Create OpenAPI Test
Add to `/tests/type_validation/openapi_compat.rs`:

```rust
#[tokio::test]
async fn test_my_new_type_openapi_compatible() {
    let value = MyNewType {
        field_a: "value".to_string(),
        field_b: 42,
    };

    let json = serde_json::to_value(&value)
        .expect("serialize failed");

    assert_has_required_fields(&json, &["field_a", "field_b"]);
    assert!(json.get("field_a").unwrap().is_string());
    assert!(json.get("field_b").unwrap().is_number());
}
```

### Step 3: Create Frontend Test
Add to `/tests/type_validation/frontend_compat.rs`:

```rust
#[tokio::test]
async fn test_my_new_type_frontend_compatible() {
    let value = MyNewType {
        field_a: "value".to_string(),
        field_b: 42,
    };

    let json = serde_json::to_value(&value)
        .expect("serialize failed");

    let violations = validate_all_fields_snake_case(&json, "MyNewType");
    assert!(violations.is_empty());
}
```

## Field Naming Rules

**All API fields must use snake_case:**

✅ Valid:
- `user_id`
- `token_count`
- `input_tokens`
- `router_latency_us`
- `_private_field`
- `field123`

❌ Invalid:
- `userId` (camelCase)
- `UserID` (PascalCase)
- `user-id` (kebab-case)

**Use serde rename if needed:**
```rust
#[derive(Serialize, Deserialize)]
pub struct MyType {
    #[serde(rename = "user_id")]
    pub user_id: String,
}
```

## Common Patterns

### Testing Optional Fields
```rust
// When None - should be omitted
let with_none = MyType { field: None };
let json = serde_json::to_value(&with_none)?;
assert!(!json.get("field").is_some());

// When Some - should be present
let with_some = MyType { field: Some("value") };
let json = serde_json::to_value(&with_some)?;
assert!(json.get("field").is_some());
```

### Testing Arrays
```rust
let value = MyType {
    items: vec![1, 2, 3],
};
let json = serde_json::to_value(&value)?;
assert!(json.get("items").unwrap().is_array());
assert_eq!(json.get("items").unwrap().as_array().unwrap().len(), 3);
```

### Testing Numeric Precision
```rust
let original = MyType { value: 3.14159265 };
let json = serde_json::to_value(&original)?;
let deserialized: MyType = serde_json::from_value(json)?;
assert!((original.value - deserialized.value).abs() < 1e-10);
```

## Troubleshooting

### "Field 'X' is not in snake_case"
**Problem:** Field was serialized with wrong naming

**Solution:**
1. Check field name in struct
2. Add `#[serde(rename = "correct_name")]` if needed

### "Type mismatch: expected X, got Y"
**Problem:** Field serialized to wrong JSON type

**Solution:**
1. Verify Rust type is correct
2. Use custom serializer if needed

### "Required field 'X' missing"
**Problem:** Required field wasn't serialized

**Solution:**
1. Remove `#[serde(skip_serializing_if = ...)]` or
2. Ensure field is always set

## Types Currently Covered

| Type | Tests | Status |
|------|-------|--------|
| InferResponse | 1 | ✅ Full |
| InferRequest | 1 | ✅ Full |
| InferenceTrace | 1 | ✅ Full |
| RouterDecision | 2 | ✅ Full |
| BatchInferRequest | 1 | ✅ Full |
| BatchInferResponse | 1 | ✅ Full |
| AdapterResponse | 2 | ✅ Full |
| AdapterManifest | 1 | ✅ Full |
| ErrorResponse | 2 | ✅ Full |
| HealthResponse | 2 | ✅ Full |
| ModelRuntimeHealth | 1 | ✅ Full |
| PaginationParams | 2 | ✅ Full |
| PaginatedResponse | 1 | ✅ Full |

## Key Files

| File | Purpose | Lines |
|------|---------|-------|
| `mod.rs` | Test utilities | 143 |
| `round_trip.rs` | Serialization tests | 532 |
| `openapi_compat.rs` | OpenAPI tests | 530 |
| `frontend_compat.rs` | Frontend tests | 582 |
| `TYPE_VALIDATION_SUITE.md` | Full documentation | 446 |
| `TYPE_VALIDATION_DELIVERABLES.md` | Project summary | 348 |

## Next Steps

1. **Run the tests:**
   ```bash
   cargo test --test type_validation_integration
   ```

2. **Read the full documentation:**
   - `TYPE_VALIDATION_SUITE.md` - Complete guide
   - `TYPE_VALIDATION_DELIVERABLES.md` - Project overview

3. **Add tests for new types:**
   - Follow the three-step pattern above
   - Use existing tests as templates

4. **Integrate with CI/CD:**
   ```bash
   # Add to your CI pipeline
   cargo test --test type_validation_integration
   ```

## Support

For detailed information:
- **Full suite guide:** See `TYPE_VALIDATION_SUITE.md`
- **Project details:** See `TYPE_VALIDATION_DELIVERABLES.md`
- **Test examples:** See test files in `type_validation/`
- **Integration example:** See `type_validation_integration.rs`

---

**Test Coverage:** 40+ tests across 16+ types
**Documentation:** 800+ lines with 30+ examples
**Total Code:** 1,800+ lines of test code
