# adapteros-aos

Memory-mapped .aos file loading with LRU caching and hot-swap support for AdapterOS.

## Features

- **Memory-mapped loading**: Efficient zero-copy access to .aos files via mmap
- **LRU caching**: Automatic caching with size-based eviction
- **Atomic hot-swap**: Load and swap adapters without downtime
- **Performance metrics**: Built-in telemetry for observability
- **Composable modules**: Use individual components or the unified manager API

## Architecture

This crate provides a **file-level cache layer**, separate from VRAM management in `adapteros-lora-worker`. It manages memory-mapped files and their lifecycle.

```
File Level (adapteros-aos) ─┐
                             ├─> AdapterLoader (adapteros-lora-lifecycle)
Memory Level (adapteros-    ─┘
lora-worker)                    
```

## Usage

### Using Individual Modules

```rust
use adapteros_aos::MmapAdapterLoader;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loader = MmapAdapterLoader::new();
    let adapter = loader.load(Path::new("adapter.aos")).await?;
    
    println!("Loaded: {} ({})", adapter.adapter_id(), adapter.version());
    Ok(())
}
```

### Using Unified Manager with Caching

```rust
use adapteros_aos::AosManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = AosManager::builder()
        .with_cache(1024 * 1024 * 1024) // 1GB cache
        .with_hot_swap()
        .build()?;

    // First load (cache miss)
    let adapter1 = manager.load("adapter.aos").await?;
    
    // Second load (cache hit)
    let adapter2 = manager.load("adapter.aos").await?;
    
    // Check cache stats
    if let Some(cache) = manager.cache() {
        println!("Cache hit rate: {:.2}%", cache.metrics().hit_rate() * 100.0);
    }
    
    Ok(())
}
```

### Hot-Swapping Adapters

```rust
use adapteros_aos::AosManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = AosManager::builder()
        .with_hot_swap()
        .build()?;

    // Preload new adapter
    manager.preload("slot1", "new_adapter.aos").await?;
    
    // Atomic swap
    manager.commit_swap(&["slot1".to_string()])?;
    
    // Rollback if needed
    // manager.rollback()?;
    
    Ok(())
}
```

## Modules

- **`mmap_loader`**: Memory-mapped .aos file loading
- **`cache`**: LRU cache with size-based eviction
- **`hot_swap`**: Atomic adapter switching with rollback
- **`metrics`**: Performance telemetry
- **`manager`**: Unified API combining all modules

## Performance

Target metrics (from plan):
- **Load latency**: < 10ms for typical .aos file
- **Swap latency**: < 5ms for hot-swap operation
- **Cache hit rate**: > 90% in typical workloads

Run benchmarks:
```bash
cargo bench -p adapteros-aos
```

## Testing

```bash
# Unit and integration tests
cargo test -p adapteros-aos

# With a real .aos file in adapters/ directory
cargo test -p adapteros-aos -- --nocapture
```

## Integration

This crate is designed to be used by:
- `adapteros-lora-lifecycle`: For adapter lifecycle management
- `adapteros-lora-worker`: For loading adapters into VRAM
- `adapteros-cli`: For CLI commands that work with .aos files

## License

MIT OR Apache-2.0






