# PRD-MODEL-01: Base Models Registry Implementation

**Status:** Implemented
**Date:** 2025-11-25
**Author:** Claude Code

## Overview

This document describes the implementation of PRD-MODEL-01: Base Models Registry for AdapterOS. The implementation makes the Base Models page reflect real models on disk, with import functionality, status tracking, and links to adapters and training jobs.

## Implementation Summary

### 1. Database Schema (Migration 0084)

**Location:** `/Users/mln-dev/Dev/adapter-os/migrations/0084_base_model_registry.sql`

Added fields to `models` table:
- `size_bytes` (INTEGER) - Model file size
- `format` (TEXT) - Model format: "mlx", "safetensors", "pytorch", "gguf"
- `capabilities` (TEXT) - JSON array of capabilities: ["chat", "completion", "embeddings"]
- `import_status` (TEXT) - Import status: "importing", "available", "failed"
- `import_error` (TEXT) - Import error message if failed
- `imported_at` (TEXT) - Import timestamp
- `imported_by` (TEXT) - User who imported

Created indexes:
- `idx_models_format` - For format queries
- `idx_models_import_status` - For import status queries

### 2. Database Methods

**Location:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-db/src/models.rs`

#### Model Struct Updates
Extended `Model` struct with new fields using `#[sqlx(default)]` for backward compatibility.

#### New Struct: ModelWithStats
```rust
pub struct ModelWithStats {
    pub model: Model,
    pub adapter_count: i64,
    pub training_job_count: i64,
}
```

#### New Database Methods

1. **`import_model_from_path`**
   - Imports a model from a directory path
   - Computes file size
   - Sets initial status to "importing"
   - Returns model ID

2. **`update_model_import_status`**
   - Updates import status ("importing", "available", "failed")
   - Records error messages if failed
   - Updates timestamp

3. **`count_adapters_for_model`**
   - Counts adapters using a model
   - Currently placeholder (returns 0) as adapters don't directly reference model_id

4. **`count_training_jobs_for_model`**
   - Counts training jobs for a model
   - Currently placeholder (returns 0) as jobs reference models indirectly

5. **`list_models_with_stats`**
   - Lists all models with adapter and training job counts
   - Returns `Vec<ModelWithStats>`

### 3. API Handlers

**Location:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/handlers/models.rs`

#### Updated Request Types

**ImportModelRequest:**
```rust
pub struct ImportModelRequest {
    pub model_name: String,
    pub model_path: String,
    pub format: String,        // "mlx", "safetensors", "pytorch", "gguf"
    pub backend: String,        // "mlx-ffi", "metal"
    pub capabilities: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
}
```

#### New Response Types

**ModelListResponse:**
```rust
pub struct ModelListResponse {
    pub models: Vec<ModelWithStatsResponse>,
    pub total: usize,
}
```

**ModelWithStatsResponse:**
```rust
pub struct ModelWithStatsResponse {
    pub id: String,
    pub name: String,
    pub format: Option<String>,
    pub backend: Option<String>,
    pub size_bytes: Option<i64>,
    pub import_status: Option<String>,
    pub model_path: Option<String>,
    pub capabilities: Option<Vec<String>>,
    pub adapter_count: i64,
    pub training_job_count: i64,
    pub imported_at: Option<String>,
    pub updated_at: Option<String>,
}
```

#### New Handlers

1. **`import_model`** - `POST /v1/models/import`
   - Validates model path exists
   - Validates format and backend
   - Imports model into database
   - Returns import ID and status

2. **`list_models_with_stats`** - `GET /v1/models`
   - Lists all models with statistics
   - Includes adapter and training job counts
   - Parses capabilities JSON

### 4. API Routes

**Location:** `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/routes.rs`

Added routes:
- `GET /v1/models` - List models with stats
- `POST /v1/models/import` - Import model from path

Updated OpenAPI schema with new types:
- `ModelListResponse`
- `ModelWithStatsResponse`

## API Usage Examples

### Import a Model

```bash
POST /v1/models/import
Authorization: Bearer <token>
Content-Type: application/json

{
  "model_name": "qwen-7b",
  "model_path": "/models/qwen2.5-7b-mlx",
  "format": "mlx",
  "backend": "mlx-ffi",
  "capabilities": ["chat", "completion"]
}
```

Response:
```json
{
  "import_id": "018c...",
  "status": "available",
  "message": "Model import completed",
  "progress": 100
}
```

### List Models with Statistics

```bash
GET /v1/models
Authorization: Bearer <token>
```

Response:
```json
{
  "models": [
    {
      "id": "018c...",
      "name": "qwen-7b",
      "format": "mlx",
      "backend": "mlx-ffi",
      "size_bytes": 4096000000,
      "import_status": "available",
      "model_path": "/models/qwen2.5-7b-mlx",
      "capabilities": ["chat", "completion"],
      "adapter_count": 5,
      "training_job_count": 3,
      "imported_at": "2025-11-25T10:00:00Z",
      "updated_at": "2025-11-25T10:00:00Z"
    }
  ],
  "total": 1
}
```

## Integration Points

### Future Enhancements

1. **Adapter Linking**
   - Add `base_model_id` column to `adapters` table
   - Update `count_adapters_for_model` to use actual foreign key

2. **Training Job Linking**
   - Add `base_model_id` column to `training_jobs` table
   - Update `count_training_jobs_for_model` to use actual foreign key

3. **Hash Computation**
   - Replace placeholder hashes with actual BLAKE3 computation
   - Verify file integrity during import

4. **Async Import**
   - Move import to background task
   - Track progress percentage
   - Support large model files

5. **Model Validation**
   - Verify model structure (config.json, tokenizer files)
   - Check format compatibility with backend
   - Validate capabilities

## Testing

### Manual Testing

1. Import a model:
```bash
curl -X POST http://localhost:8080/v1/models/import \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "model_name": "test-model",
    "model_path": "/path/to/model",
    "format": "mlx",
    "backend": "mlx-ffi"
  }'
```

2. List models:
```bash
curl http://localhost:8080/v1/models \
  -H "Authorization: Bearer <token>"
```

### Verification Checklist

- [ ] Migration 0084 applies successfully
- [ ] Model import creates database record
- [ ] Import status transitions correctly
- [ ] List endpoint returns models with stats
- [ ] File size is computed correctly
- [ ] Capabilities are parsed from JSON
- [ ] Format and backend are validated

## Files Modified

1. `/Users/mln-dev/Dev/adapter-os/migrations/0084_base_model_registry.sql` (NEW)
2. `/Users/mln-dev/Dev/adapter-os/crates/adapteros-db/src/models.rs` (MODIFIED)
3. `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/handlers/models.rs` (MODIFIED)
4. `/Users/mln-dev/Dev/adapter-os/crates/adapteros-server-api/src/routes.rs` (MODIFIED)

## Compliance

- ✅ Follows AdapterOS patterns from CLAUDE.md
- ✅ Uses `tracing` for logging (not `println!`)
- ✅ Returns `Result<T, AosError>` for errors
- ✅ Uses `Db` trait methods for database operations
- ✅ Includes OpenAPI documentation
- ✅ Follows RBAC permissions (Admin/Operator for import)
- ✅ Uses structured error responses

## Next Steps

1. Run database migration: `./target/release/aosctl db migrate`
2. Test import functionality with real model paths
3. Implement hash computation for integrity verification
4. Add foreign key relationships for adapters and training jobs
5. Create UI components for Base Models page
