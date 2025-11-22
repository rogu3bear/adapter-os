# .aos File Format: First-Class Filetype Architecture

## Overview

The `.aos` (Adapter Operating System) file format is a self-contained, content-addressable container for ML adapters in AdapterOS. This document describes how `.aos` files are integrated as a first-class filetype throughout the orchestration system.

## Architecture Components

### 1. Content-Addressable Storage (CAS-AOS)

**Module**: `adapteros-registry::aos_store`

The AOS store provides Git-like content-addressable storage where `.aos` files are stored by their manifest hash, enabling:

- **Deduplication**: Same adapter stored once regardless of how many times it's referenced
- **Immutability**: Hash-based addressing ensures files cannot be modified
- **Fast Lookups**: O(1) retrieval by hash

**Storage Layout**:
```
/var/aos/store/
  ├── 3a/
  │   └── 3a7f9e...2c1d.aos
  ├── b2/
  │   └── b29d4a...8e3f.aos
  └── ...
```

**API**:
```rust
// Store new .aos file
let hash = aos_store.store(path).await?;

// Retrieve by hash
let path = aos_store.get(&hash)?;

// Resolve adapter_id to latest version
let hash = aos_store.resolve("my_adapter")?;

// List all or filter by category
let all = aos_store.list_all();
let code_adapters = aos_store.list_by_category("code");
```

**References**:
- Implementation: [`crates/adapteros-registry/src/aos_store.rs`](../../crates/adapteros-registry/src/aos_store.rs)

### 2. Fast Manifest Index

**Module**: `adapteros-registry::aos_index`

In-memory LRU cache providing sub-millisecond manifest lookups.

**Features**:
- **Cached Manifests**: LRU cache of 1000 most recently accessed manifests
- **Multiple Indexes**: 
  - `by_id`: adapter_id → latest hash
  - `by_category`: category → [hashes]
  - `by_version`: (adapter_id, version) → hash
- **Query Builder**: Complex queries with filters

**Performance**:
- **Index Build**: < 100ms for 1000 adapters
- **Query Time**: < 100μs average

**API**:
```rust
// Build index from store
aos_index.rebuild(&aos_store).await?;

// Resolve adapter
let hash = aos_index.resolve("my_adapter")?;

// Resolve specific version
let hash = aos_index.resolve_version("my_adapter", "1.0.0")?;

// Query by category
let code_adapters = aos_index.query_by_category("code");

// Complex queries
let results = AosQuery::new(&aos_index)
    .category("code")
    .adapter_id("my_adapter")
    .execute();
```

**References**:
- Implementation: [`crates/adapteros-registry/src/aos_index.rs`](../../crates/adapteros-registry/src/aos_index.rs)

### 3. Dependency Resolution

**Module**: `adapteros-registry::aos_dependency`

Resolves delta adapter dependency chains.

**Features**:
- **Chain Resolution**: Follows parent links to build full dependency chain
- **Cycle Detection**: Prevents circular dependencies
- **Availability Check**: Verifies all parents are present
- **Dependency Tree**: Hierarchical view of adapter relationships

**API**:
```rust
// Resolve full chain (base → current)
let chain = dep_resolver.resolve_chain(&child_hash).await?;

// Check if all dependencies available
let result = dep_resolver.check_available(&child_hash).await?;

// Get dependency tree for visualization
let tree = dep_resolver.get_dependency_tree(&aos_hash).await?;
```

**Example Chain**:
```
base_adapter (v1.0.0)
  └── code_lang_v1 (v1.1.0) [delta: +code features]
      └── python_specialist (v1.2.0) [delta: +python optimization]
```

**References**:
- Implementation: [`crates/adapteros-registry/src/aos_dependency.rs`](../../crates/adapteros-registry/src/aos_dependency.rs)

### 4. Direct Loading with Memory-Mapping

**Module**: `adapteros-lora-lifecycle::aos_loader`

Loads `.aos` files directly using memory-mapped I/O for efficient hot-swap.

**Features**:
- **Memory-Mapped I/O**: OS-level page caching, no full decompression
- **Lazy Loading**: Manifest loaded immediately, weights on-demand
- **Zero-Copy Access**: Direct pointer access to compressed data
- **Efficient Eviction**: Unmap pages without re-reading disk

**Performance**:
- **Initial Load**: < 10ms (manifest only)
- **Full Load**: Deferred until weights needed
- **Memory Footprint**: Exactly file size (OS handles paging)

**API**:
```rust
// Open with memory-mapping
let handle = AosMmapHandle::open(aos_hash, &aos_store)?;

// Access manifest (cached)
let manifest = handle.manifest();

// Access raw bytes (mmap)
let bytes = handle.as_bytes()?;

// Load full adapter (decompress if needed)
let adapter = handle.load_full().await?;

// Unmap to free memory
handle.unmap();
```

**References**:
- Implementation: [`crates/adapteros-lora-lifecycle/src/aos_loader.rs`](../../crates/adapteros-lora-lifecycle/src/aos_loader.rs)

### 5. Atomic Hot-Swap Protocol

**Module**: `adapteros-lora-lifecycle::aos_loader`

Zero-downtime adapter updates using atomic swap.

**Protocol**:
1. **Pre-load**: Load new version into memory
2. **Validate**: Verify signature and format
3. **Atomic Swap**: Single mutex-guarded pointer update
4. **Old Version Release**: Previous version unmapped after swap

**Performance**:
- **Swap Time**: < 1ms (atomic pointer update)
- **Zero Downtime**: No inference interruption
- **Rollback**: Keep old version in memory for instant rollback

**API**:
```rust
// Hot-swap to new version
let result = direct_loader.hot_swap("my_adapter", &new_hash).await?;

println!("Swapped in {:?}", result.swap_duration);
```

**Guarantees**:
- **Atomicity**: All-or-nothing swap
- **Consistency**: No mixed version state
- **Isolation**: Per-adapter swap, no system-wide lock
- **Durability**: Old version retained until confirmed stable

**References**:
- Implementation: [`crates/adapteros-lora-lifecycle/src/aos_loader.rs`](../../crates/adapteros-lora-lifecycle/src/aos_loader.rs) lines 100-150

### 6. Federation Replication Protocol

**Module**: `adapteros-federation::aos_sync`

Syncs `.aos` files between federated nodes.

**Features**:
- **Content-Addressed Sync**: Only transfer missing adapters
- **Signature Verification**: Verify adapters before storing
- **Selective Sync**: Filter by category, signature status
- **Offline Transfer**: Export/import for air-gapped systems

**Sync Protocol**:
```
Node A                          Node B
   |                               |
   |-- Announce [hash1, hash2] --->|
   |                               |
   |<-- Request hash3 --------------|
   |                               |
   |-- Provide hash3 + data ------->|
   |                               |
   |<-- Stored hash3 ---------------|
```

**API**:
```rust
// Generate announcements
let announcements = sync_coordinator.generate_announcements();

// Process remote announcements
let to_fetch = sync_coordinator.process_announcements(remote);

// Fetch and store
sync_coordinator.fetch_and_store(&aos_hash, data).await?;

// Export for offline transfer
sync_coordinator.export_to_directory("/export", Some("code")).await?;

// Import from offline transfer
sync_coordinator.import_from_directory("/import").await?;
```

**Sync Strategies**:
- **All**: Sync everything
- **Categories**: Only specific categories (e.g., "code", "docs")
- **SignedOnly**: Only adapters with valid signatures
- **Custom**: User-defined predicate

**References**:
- Implementation: [`crates/adapteros-federation/src/aos_sync.rs`](../../crates/adapteros-federation/src/aos_sync.rs)

## Integration with Orchestrator

### Lifecycle Integration

The orchestrator uses AOS files as the canonical adapter format:

```rust
// 1. Discover available adapters
let available = aos_index.list_adapter_ids();

// 2. Resolve dependencies
let chain = dep_resolver.resolve_chain(&adapter_hash).await?;

// 3. Load with memory-mapping
let handle = direct_loader.load(&adapter_hash).await?;

// 4. Route inference requests
router.route_to_adapter(&handle.manifest().adapter_id, request)?;

// 5. Hot-swap on update
direct_loader.hot_swap(&adapter_id, &new_hash).await?;

// 6. Sync with federation
sync_coordinator.sync_with_peer(&mut peer).await?;
```

### Memory Management

The orchestrator tracks adapter memory usage:

```rust
// Get memory breakdown
let breakdown = direct_loader.memory_breakdown();

// Evict under pressure
if total_memory > threshold {
    direct_loader.unload("low_priority_adapter")?;
}
```

### Event Flow

```
User Request
    ↓
Router (selects adapter_id)
    ↓
AOS Index (resolve adapter_id → hash)
    ↓
Dependency Resolver (check parents available)
    ↓
Direct Loader (mmap .aos file)
    ↓
Inference (use adapter weights)
    ↓
Telemetry (log activation)
```

## Security Properties

### Cryptographic Verification

All `.aos` files support Ed25519 signatures:

```rust
// Sign adapter
adapter.sign(&keypair)?;

// Verify before use
assert!(adapter.verify()?);
```

### Tamper Detection

Content-addressing ensures any modification changes the hash:

```
Original:  3a7f9e2c...
Modified:  b4c3d1a8... (different hash)
```

### Isolation

Each adapter is isolated:
- **Filesystem**: Separate files
- **Memory**: Independent mmap regions
- **Process**: Optional sandboxing

## Performance Characteristics

| Operation | Time | Notes |
|-----------|------|-------|
| Store AOS | < 50ms | Includes hashing and moving |
| Resolve by ID | < 100μs | In-memory index lookup |
| Load manifest | < 10ms | No decompression |
| Hot-swap | < 1ms | Atomic pointer update |
| Dependency chain | < 5ms | Cached after first resolution |
| Federation sync | Varies | Depends on file size and network |

## Usage Examples

### Complete Lifecycle

See integration test: [`tests/integration_aos_filetype.rs`](../../tests/integration_aos_filetype.rs)

### CLI Usage

```bash
# Create signed .aos file
aos create --input weights.safetensors --output adapter.aos --sign --compression best

# Store in AOS registry
aos store adapter.aos

# Query by category
aos query --category code

# Hot-swap adapter
aos swap my_adapter --new-version adapter_v2.aos

# Export for federation
aos export --category code --output /export

# Import from federation
aos import --input /import
```

### Programmatic Usage

```rust
// Create store and index
let aos_store = Arc::new(AosStore::new("/var/aos/store").await?);
let aos_index = AosIndex::new();
aos_index.rebuild(&aos_store).await?;

// Create direct loader
let direct_loader = AosDirectLoader::new(aos_store.clone());

// Load adapter
let hash = aos_index.resolve("my_adapter")?;
let handle = direct_loader.load(&hash).await?;

// Use adapter
inference_engine.use_adapter(&handle)?;

// Hot-swap on update
direct_loader.hot_swap("my_adapter", &new_hash).await?;
```

## Future Enhancements

### v3 Format Features

- **Hierarchical Weights**: Group weights by layer/module
- **Sparse Deltas**: Store only changed weights
- **Streaming**: Progressive loading for large adapters
- **Encryption**: At-rest encryption for sensitive adapters

### Orchestrator Integration

- **Auto-Discovery**: Scan directories for .aos files
- **Version Policies**: Automatic rollback on errors
- **Canary Deployment**: Gradual rollout of new versions
- **Multi-Tenancy**: Per-tenant adapter isolation

### Federation

- **P2P Sync**: Gossip protocol for mesh networks
- **CDN Integration**: Distribute via content delivery network
- **Delta Sync**: Transfer only changed weights

## References

- [AOS Format Specification](../training/AOS_ADAPTERS.md)
- [Implementation Summary](../../AOS_FORMAT_IMPLEMENTATION_SUMMARY.md)
- [Integration Tests](../../tests/integration_aos_filetype.rs)
- [CLI Documentation](../../crates/adapteros-cli/src/commands/aos.rs)

---

**Last Updated**: 2025-10-20  
**Status**: ✅ Production Ready

