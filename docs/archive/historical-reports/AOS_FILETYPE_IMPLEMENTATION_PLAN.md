# .aos Filetype: First-Class Integration Plan

## Status: ✅ Plan Completed, Implementation Deferred

**Date**: 2025-10-20  
**Outcome**: Comprehensive implementation plan created with architecture design, but implementation files removed due to compilation complexity.

---

## What Was Accomplished

### 1. **Architectural Design** ✅

Created a complete architecture for making `.aos` files first-class in the orchestration system:

**Key Components Designed**:
- Content-Addressable Storage (CAS-AOS) - Git-like hash-based storage
- Fast Manifest Index - Sub-millisecond LRU-cached lookups
- Dependency Resolution - Delta adapter chain resolution
- Memory-Mapped Loading - Efficient zero-copy weight access
- Atomic Hot-Swap - Zero-downtime adapter updates (< 1ms)
- Federation Replication - Content-addressed sync protocol

**Architecture Documents Created**:
- [`docs/architecture/aos_filetype_architecture.md`](docs/architecture/aos_filetype_architecture.md) - Complete technical architecture
- [`docs/architecture/AOS_FILETYPE_INTEGRATION_SUMMARY.md`](docs/architecture/AOS_FILETYPE_INTEGRATION_SUMMARY.md) - Implementation summary with examples

### 2. **Existing Foundation** ✅

The codebase already has solid `.aos` format support:

**Core Format** (`adapteros-single-file-adapter`):
- ✅ ZIP-based container format
- ✅ Manifest, weights, training data, lineage
- ✅ Ed25519 cryptographic signatures
- ✅ BLAKE3 integrity hashing
- ✅ Configurable compression levels
- ✅ Format versioning (v1 → v2)
- ✅ Migration support

**CLI Tools** (`adapteros-cli`):
- ✅ `aos create` - Create .aos files
- ✅ `aos load` - Load and inspect
- ✅ `aos verify` - Validate signatures
- ✅ `aos extract` - Extract components
- ✅ `aos info` - Display metadata
- ✅ `aos migrate` - Upgrade format versions

### 3. **Integration Strategy** ✅

Identified optimal integration points in the orchestration system:

**With `adapteros-orchestrator`**:
- Load adapters directly from `.aos` files
- Track adapter versions via content hashing
- Hot-swap adapters during inference

**With `adapteros-lora-lifecycle`**:
- Memory-map `.aos` files for efficient loading
- Eviction policy based on adapter file size
- TTL management per adapter

**With `adapteros-federation`**:
- Sync `.aos` files between nodes
- Verify signatures before accepting
- Selective replication strategies

### 4. **Performance Targets** ✅

Defined clear performance goals:

| Operation | Target | Rationale |
|-----------|--------|-----------|
| Store AOS | < 100ms | One-time operation |
| Index lookup | < 1ms | Hot path, frequent |
| Dependency chain | < 10ms | Occasional, cacheable |
| Hot-swap | < 5ms | Near-zero downtime requirement |
| Manifest load | < 20ms | Should not block inference |

---

## Why Implementation Was Deferred

### Compilation Complexity

The full integration required changes across multiple crates with interdependencies:

1. **Registry Dependency Issues**:
   - Adding AOS modules to `adapteros-registry` created circular dependencies
   - `adapteros-lora-lifecycle` → `adapteros-registry` → `adapteros-single-file-adapter` → `adapteros-lora-lifecycle`

2. **Type System Conflicts**:
   - `PartialEq` derive issues with `adapteros_crypto::Signature` type
   - `ZipError` conversion not implemented for `AosError`
   - Multiple trait bound requirements across crate boundaries

3. **Architectural Decision Needed**:
   - Should AOS modules live in registry, lifecycle, or dedicated crate?
   - Current crate structure doesn't have clear "home" for AOS orchestration logic

### Better Approach

Rather than force the implementation into existing crate structure, the right approach is:

**Option A: Dedicated `adapteros-aos` Crate**
```
adapteros-aos/
  ├── store.rs        # Content-addressable storage
  ├── index.rs        # Fast manifest index
  ├── dependency.rs   # Chain resolution
  ├── loader.rs       # Memory-mapped loading
  └── sync.rs         # Federation protocol
```

**Option B: Enhance Existing Structure**
- Move AOS store to `adapteros-registry` (metadata only, no lifecycle)
- Move AOS loader to `adapteros-lora-lifecycle` (memory management)
- Move AOS sync to `adapteros-federation` (already focused on sync)

---

## What You Can Do Now

### 1. Use Existing `.aos` Format ✅

The `.aos` format is already production-ready:

```bash
# Create signed adapter
aos create \
  --input weights.safetensors \
  --output my_adapter.aos \
  --sign \
  --compression best

# Verify before use
aos verify my_adapter.aos

# Load in orchestrator (manual integration)
# Your code can use SingleFileAdapterLoader directly
```

### 2. Manual Integration

You can integrate `.aos` files manually today:

```rust
use adapteros_single_file_adapter::SingleFileAdapterLoader;

// Load adapter
let adapter = SingleFileAdapterLoader::load("my_adapter.aos").await?;

// Verify signature
assert!(adapter.verify()?);

// Use weights
let weights = adapter.weights;

// Track lineage
println!("Parent: {:?}", adapter.lineage.parent_hash);
```

### 3. Simple Content-Addressable Storage

Implement basic CAS without full orchestration:

```bash
#!/bin/bash
# Simple CAS script

aos_store_dir="/var/aos/store"

store_aos() {
    local aos_file="$1"
    
    # Compute manifest hash
    local hash=$(unzip -p "$aos_file" manifest.json | sha256sum | cut -d' ' -f1)
    
    # Store at hash-based path
    local subdir="${hash:0:2}"
    mkdir -p "$aos_store_dir/$subdir"
    cp "$aos_file" "$aos_store_dir/$subdir/$hash.aos"
    
    echo "$hash"
}

# Usage:
# hash=$(store_aos my_adapter.aos)
# aos_file="$aos_store_dir/${hash:0:2}/$hash.aos"
```

### 4. Implement Hot-Swap

You can implement hot-swap independently:

```rust
use std::sync::Arc;
use parking_lot::RwLock;

pub struct AdapterRegistry {
    adapters: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl AdapterRegistry {
    pub fn hot_swap(&self, adapter_id: &str, new_path: PathBuf) {
        let mut adapters = self.adapters.write();
        adapters.insert(adapter_id.to_string(), new_path);
        // Atomic swap complete - inference requests will see new path
    }
}
```

---

## Implementation Roadmap (When Ready)

### Phase 1: Foundation (1-2 weeks)
- [ ] Create `adapteros-aos` crate
- [ ] Implement basic CAS storage
- [ ] Add simple in-memory index
- [ ] Write comprehensive tests

### Phase 2: Integration (2-3 weeks)
- [ ] Integrate with `adapteros-orchestrator`
- [ ] Add to `adapteros-lora-lifecycle`
- [ ] Implement hot-swap protocol
- [ ] Performance benchmarks

### Phase 3: Federation (2-3 weeks)
- [ ] Implement sync protocol
- [ ] Add to `adapteros-federation`
- [ ] Security audit
- [ ] End-to-end tests

### Phase 4: Production (1-2 weeks)
- [ ] Stress testing
- [ ] Documentation
- [ ] Migration tooling
- [ ] Deployment

**Total Estimated Time**: 6-10 weeks for full production implementation

---

## Key Decisions Needed

Before implementing, decide:

1. **Crate Structure**: Dedicated `adapteros-aos` vs. split across existing crates?
2. **Storage Backend**: Filesystem vs. database vs. hybrid?
3. **Index Persistence**: Rebuild on startup vs. persistent index file?
4. **Federation Priority**: Essential for v1 or can defer?
5. **Memory Mapping**: Required for large adapters or can use standard loading?

---

## Lessons Learned

### What Went Well ✅
- Comprehensive architectural design
- Clear performance targets
- Identified integration points
- Created reusable documentation

### What Didn't Go Well ❌
- Tried to implement before deciding on crate structure
- Didn't account for circular dependency issues
- Should have started with dedicated crate

### Recommendations
1. **Start Small**: Implement CAS storage first, validate, then add features
2. **Dedicated Crate**: Don't try to fit into existing crates
3. **Test First**: Write integration tests defining desired API
4. **Incremental**: Each phase should be independently valuable

---

## Conclusion

While the full implementation was deferred, this effort produced:

✅ **Complete Architecture** - Detailed design ready for implementation  
✅ **Clear Integration Plan** - Specific touchpoints identified  
✅ **Performance Targets** - Measurable success criteria  
✅ **Documentation** - Architecture guide and examples  
✅ **Existing Foundation** - `.aos` format already production-ready  

**The `.aos` filetype is ready to use today** for creating, signing, and loading adapters. The orchestration enhancements (CAS, hot-swap, federation) can be implemented when the architectural decisions are finalized.

---

## Quick Start (Using What Exists)

```bash
# 1. Create adapter
aos create --input weights.safetensors --output adapter.aos --sign

# 2. Verify
aos verify adapter.aos

# 3. Use in code
cat > use_aos.rs << 'EOF'
use adapteros_single_file_adapter::SingleFileAdapterLoader;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load adapter
    let adapter = SingleFileAdapterLoader::load("adapter.aos").await?;
    
    // Verify signature
    if adapter.is_signed() {
        assert!(adapter.verify()?);
        println!("✅ Signature valid");
    }
    
    // Use weights
    println!("Adapter: {} v{}", 
        adapter.manifest.adapter_id,
        adapter.manifest.version);
    
    Ok(())
}
EOF
```

**The foundation is solid. The orchestration enhancements are well-designed and ready to implement when you need them.**

---

**Created**: 2025-10-20  
**Status**: Architecture complete, implementation deferred  
**Next Steps**: Decide on crate structure, then implement in phases

