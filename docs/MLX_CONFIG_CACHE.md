# MLX Backend Model Configuration Caching

**Last Updated:** 2025-11-19
**Status:** Implementation Complete
**Location:** `/crates/adapteros-lora-mlx-ffi/src/model_config_cache.rs`

## Overview

The model configuration caching system improves MLX backend performance by avoiding repeated file I/O when accessing model parameters. It provides thread-safe, lazy-loading access to critical model dimensions used throughout inference.

## Architecture

### Components

1. **`ModelConfigCache`** - Immutable configuration snapshot
   - Stores all model parameters (vocab_size, hidden_size, num_layers, etc.)
   - Validates dimensional consistency on creation
   - Provides derived fields (head_dim, num_heads_per_kv_head)
   - Serializable for persistence

2. **`ModelConfigCacheManager`** - Thread-safe caching wrapper
   - Lazy loading from config.json files
   - Interior mutability with Arc<RwLock>
   - Cache validation and refresh capabilities
   - Read-lock optimized for high-frequency access

3. **Backend Integration**
   - `MLXFFIBackend::with_config_path()` - Initialize with config path
   - `MLXFFIBackend::cached_model_config()` - Get or load cached config
   - `MLXFFIBackend::get_cached_config()` - Access without loading
   - `MLXFFIBackend::is_config_cached()` - Check cache status

## Usage Patterns

### Initialization with Cache

```rust
use adapteros_lora_mlx_ffi::MLXFFIBackend;

let model = MLXFFIModel::load("path/to/model")?;
let backend = MLXFFIBackend::with_config_path(model, Some("path/to/model"));
```

### Accessing Cached Configuration

```rust
// Lazy load from file on first access
let config = backend.cached_model_config()?;
println!("Hidden size: {}", config.hidden_size);
println!("Head dimension: {}", config.head_dim);

// Subsequent calls use cache (no I/O)
let config2 = backend.cached_model_config()?; // Fast path
```

### Memory Estimation with Dynamic Configuration

```rust
// Uses actual model dimensions from cache
let adapter_id = 1;
let memory_usage = backend.get_adapter_memory_usage(adapter_id)?;

// Example: rank=16, 4 modules, hidden_size=4096
// Estimate: 4 × 2 × 16 × 4096 × 4 bytes = 4.1 MB
```

### Cache Management

```rust
// Check if cached
if backend.is_config_cached() {
    let config = backend.get_cached_config(); // No file I/O
}

// Clear cache if needed
backend.clear_config_cache();

// Get cache status
let status = backend.config_cache_status();
println!("Config path: {}", status.config_path.display());
```

## Performance Characteristics

### Latency

- **First access:** ~1-5ms (file I/O + parsing)
- **Cached access:** ~100-500ns (memory read + RwLock)
- **Cache miss:** Falls back to model config (in-memory already)

### Memory Impact

- **Per backend:** ~2KB for cached configuration
- **Arc<RwLock> overhead:** ~64 bytes
- **Overall:** Negligible (<0.1% of model size)

### Scalability

- **Thread-safe:** Multiple reader threads can access cache simultaneously
- **Read-optimized:** RwLock favors readers (write lock only on reload)
- **No contention:** Typical use case is read-only after initialization

## Configuration Format

The cache expects standard HuggingFace model config.json:

```json
{
    "vocab_size": 32000,
    "hidden_size": 4096,
    "num_hidden_layers": 32,
    "num_attention_heads": 32,
    "num_key_value_heads": 8,
    "intermediate_size": 11008,
    "max_position_embeddings": 2048,
    "rope_theta": 10000.0
}
```

### Required Fields
- `vocab_size` - Vocabulary size (u64 or u32)
- `hidden_size` - Hidden dimension
- `num_hidden_layers` - Number of transformer layers
- `num_attention_heads` - Number of attention heads
- `num_key_value_heads` - KV heads (optional, defaults to num_attention_heads)
- `intermediate_size` - FFN intermediate dimension

### Optional Fields
- `max_position_embeddings` - Max sequence length (defaults: 2048)
- `rope_theta` - RoPE theta parameter (defaults: 10000.0)

## Validation

### Dimensional Consistency Checks

The cache validates:

1. **Non-zero dimensions** - All critical params > 0
2. **Divisibility** - hidden_size % num_attention_heads == 0
3. **KV head compatibility** - num_attention_heads % num_key_value_heads == 0
4. **Derived fields** - head_dim = hidden_size / num_attention_heads

```rust
// Invalid: 4096 % 33 != 0
let result = ModelConfigCache::new(
    32000, 4096, 32, 33, 8, 11008, 2048, 10000.0
);
assert!(result.is_err()); // Validation fails
```

## Integration with Adapter Memory Estimation

The cache enables accurate memory usage prediction:

```rust
// Old: Hardcoded 4096 hidden_size
// let estimated = rank * 4096 * 2 * num_modules * 4

// New: Uses actual model dimensions
let config = backend.cached_model_config()?;
let estimated = rank * config.hidden_size * 2 * num_modules * 4;
```

**Benefits:**
- Works with any model size (not just 7B)
- Accurate lifecycle decisions (eviction thresholds)
- Better resource planning

## Dynamic Model Support

The caching system enables support for multiple model architectures:

```rust
// Model A: 7B with hidden_size=4096
let backend_a = MLXFFIBackend::from_model_path("models/7b")?;
let config_a = backend_a.cached_model_config()?;

// Model B: 13B with hidden_size=5120
let backend_b = MLXFFIBackend::from_model_path("models/13b")?;
let config_b = backend_b.cached_model_config()?;

// Both work correctly with their own dimensions
assert_ne!(config_a.hidden_size, config_b.hidden_size);
```

## Error Handling

### File I/O Errors

```rust
// File not found or unreadable
let result = cache_manager.get();
// Returns: AosError::Io("Failed to read config from ...")
```

### Parse Errors

```rust
// Invalid JSON
let result = ModelConfigCache::from_json("{ invalid json }");
// Returns: AosError::Parse("Failed to parse config JSON: ...")
```

### Validation Errors

```rust
// Inconsistent dimensions
let result = ModelConfigCache::new(
    32000, 4096, 32, 32, 7, 11008, 2048, 10000.0
);
// Returns: AosError::Validation("num_attention_heads (32) must be divisible by ...")
```

## Testing

### Unit Tests

The cache module includes comprehensive unit tests:

```bash
cargo test -p adapteros-lora-mlx-ffi model_config_cache --lib
```

Test coverage:
- Configuration creation with validation
- JSON parsing with defaults
- Dimensional consistency checks
- Lazy loading and caching
- Cache reload functionality
- Memory estimation

### Test Cases

1. **Valid configuration** - Creates without error
2. **Invalid dimensions** - Rejects on dimensional mismatch
3. **JSON parsing** - Loads standard HF format
4. **Default values** - Fills in optional fields
5. **Lazy loading** - File I/O only on first access
6. **Cache reload** - Updates on file change

## Monitoring and Debugging

### Cache Status

```rust
let status = backend.config_cache_status();
println!("Cached: {}", status.is_cached);
println!("Path: {}", status.config_path.display());
```

### Tracing

Debug logging at multiple levels:

```
DEBUG: "Loaded model configuration into cache"
  vocab_size=32000
  hidden_size=4096
  num_layers=32

TRACE: "Returning cached model configuration"
  config_path="path/to/config.json"
```

### Memory Usage

```rust
// Get adapter memory estimate
let usage = backend.get_adapter_memory_usage(adapter_id)?;
let mb = usage as f32 / (1024.0 * 1024.0);
println!("Adapter memory: {:.2} MB", mb);
```

## Future Enhancements

1. **Time-based invalidation** - Reload if file older than TTL
2. **Multiple model support** - Global cache for shared configs
3. **Persistence** - JSON cache files for faster startup
4. **Metrics** - Cache hit/miss rates for monitoring
5. **Quantization support** - Handle different precision configs

## References

- **Module:** `/crates/adapteros-lora-mlx-ffi/src/model_config_cache.rs`
- **Backend:** `/crates/adapteros-lora-mlx-ffi/src/backend.rs`
- **Tests:** See module `tests` section
- **Integration:** `MLXFFIBackend::with_config_path()` and memory estimation

## Related Documentation

- [MLX Backend Architecture](./architecture-patterns.md#multi-backend-architecture)
- [Memory Management](./ARCHITECTURE_PATTERNS.md#multi-backend-architecture)
- [Model Loading](./AOS_LOADER_TELEMETRY.md)
