# MLX Config Cache Quick Start

## What Was Added?

A caching system for model configurations in the MLX backend that:
- Avoids repeated file I/O when accessing model parameters
- Works with models of any size (not just 7B)
- Enables accurate adapter memory estimation
- Uses thread-safe lazy loading

## Key Files

| File | Purpose |
|------|---------|
| `src/model_config_cache.rs` | Cache implementation (500+ lines) |
| `src/backend.rs` | Backend integration |
| `src/lib.rs` | Module exports |
| `examples/config_caching_example.rs` | Usage examples |
| `docs/MLX_CONFIG_CACHE.md` | Full documentation |

## 5-Minute Quick Start

### 1. Create Backend with Caching

```rust
use adapteros_lora_mlx_ffi::MLXFFIBackend;

let model = MLXFFIModel::load("models/qwen-7b")?;
let backend = MLXFFIBackend::with_config_path(model, "models/qwen-7b");
```

### 2. Access Configuration

```rust
// First access loads from file (1-5ms)
let config = backend.cached_model_config()?;
println!("Hidden: {}", config.hidden_size);

// Subsequent accesses use cache (100-500ns)
let config = backend.cached_model_config()?;
```

### 3. Check Cache Status

```rust
if backend.is_config_cached() {
    println!("Cache active!");
}

let status = backend.config_cache_status();
println!("Config: {}", status.config_path.display());
```

### 4. Use for Memory Estimation

```rust
// Accurate memory estimate for adapters
let memory = backend.get_adapter_memory_usage(adapter_id)?;
let mb = memory as f32 / (1024.0 * 1024.0);
println!("Adapter memory: {:.2} MB", mb);
```

## What Changed in Backend?

### Before
```rust
// Hardcoded 4096 for all models
let estimate = rank * 4096 * 2 * modules * 4;
```

### After
```rust
// Uses actual model dimensions
let config = backend.cached_model_config()?;
let estimate = rank * config.hidden_size * 2 * modules * 4;
```

## Performance Impact

| Operation | Latency | Notes |
|-----------|---------|-------|
| First config access | 1-5ms | File I/O + JSON parse |
| Cached access | 100-500ns | Memory read only |
| Memory overhead | ~2KB | Per backend instance |

**Speedup:** ~1000x for repeated access patterns

## Supported Configuration Format

Standard HuggingFace `config.json`:

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

## Error Handling

```rust
match backend.cached_model_config() {
    Ok(config) => println!("Hidden: {}", config.hidden_size),
    Err(e) => println!("Failed: {}", e),
}
```

**Possible errors:**
- `AosError::Io` - File not found
- `AosError::Parse` - Invalid JSON
- `AosError::Validation` - Bad dimensions

## Backward Compatibility

Existing code continues to work:

```rust
// Old: No caching (still works)
let backend = MLXFFIBackend::new(model);

// New: With caching
let backend = MLXFFIBackend::with_config_path(model, model_dir);
```

## Testing

Run the unit tests:

```bash
cargo test --lib model_config_cache
```

Test coverage includes:
- Configuration creation and validation
- JSON parsing with defaults
- Lazy loading behavior
- Cache reload functionality
- Memory estimation accuracy

## Dynamic Model Support

```rust
// Model A: 7B (hidden=4096)
let backend_a = MLXFFIBackend::with_config_path(
    MLXFFIModel::load("models/7b")?,
    "models/7b"
)?;

// Model B: 13B (hidden=5120)
let backend_b = MLXFFIBackend::with_config_path(
    MLXFFIModel::load("models/13b")?,
    "models/13b"
)?;

// Each uses correct dimensions
assert_ne!(
    backend_a.cached_model_config()?.hidden_size,
    backend_b.cached_model_config()?.hidden_size
);
```

## Public API

### MLXFFIBackend Methods

- `with_config_path(model, dir)` - Initialize with caching
- `cached_model_config()` - Get/load cached config
- `get_cached_config()` - Access without loading
- `is_config_cached()` - Check if loaded
- `clear_config_cache()` - Clear cache
- `config_cache_status()` - Get metadata

### ModelConfigCache Fields

- `vocab_size` - Vocabulary size
- `hidden_size` - Hidden dimension
- `num_hidden_layers` - Number of layers
- `num_attention_heads` - Attention heads
- `num_key_value_heads` - KV heads
- `intermediate_size` - FFN size
- `max_position_embeddings` - Seq length
- `rope_theta` - RoPE parameter
- `head_dim` - Derived: hidden_size / num_attention_heads
- `num_heads_per_kv_head` - Derived: num_attention_heads / num_key_value_heads

## Next Steps

1. **Integrate** - Use `with_config_path()` instead of `new()` when model dir available
2. **Monitor** - Check cache status during inference
3. **Profile** - Measure memory estimation accuracy improvements
4. **Document** - Add config caching to deployment guides

## Common Issues

**Issue:** Cache not loading
```rust
if !backend.is_config_cached() {
    let config = backend.cached_model_config()?; // Loads now
}
```

**Issue:** Wrong memory estimate
```rust
// Ensure using cached version, not fallback
let config = backend.cached_model_config()?;
println!("Using hidden_size: {}", config.hidden_size);
```

**Issue:** File not found
```rust
// Pass directory containing config.json
backend = MLXFFIBackend::with_config_path(model, "/path/to/model");
```

## References

- **Full Guide:** `/docs/MLX_CONFIG_CACHE.md`
- **Examples:** `/crates/adapteros-lora-mlx-ffi/examples/config_caching_example.rs`
- **Implementation:** `/crates/adapteros-lora-mlx-ffi/src/model_config_cache.rs`
- **Integration:** `/crates/adapteros-lora-mlx-ffi/src/backend.rs`
