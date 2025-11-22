# Metal Build System Integration for adapteros-memory

## Overview

This document describes the build system integration for compiling Objective-C++ Metal heap observer code in the adapteros-memory crate.

## Architecture

### File Structure

```
crates/adapteros-memory/
├── build.rs                          # Build script (Rust)
├── Cargo.toml                        # Crate manifest with build-dependencies
├── include/
│   └── heap_observer.h              # C/C++ header with FFI declarations
└── src/
    ├── heap_observer_impl.mm        # Objective-C++ Metal implementation
    └── heap_observer.rs             # Rust FFI bindings and wrappers
```

## Build Process

### 1. Build Script (`build.rs`)

The build script handles compilation of the Objective-C++ Metal heap observer:

**macOS Targets:**
- Detects and includes `src/heap_observer_impl.mm`
- Compiles with C++17 standard and Objective-C ARC support
- Links against Metal, Foundation, IOKit, and CoreFoundation frameworks
- Optimizes with `-O3` flag
- Enables compiler warnings

**Non-macOS Targets:**
- Provides stub implementation (no compilation)
- FFI functions are conditionally compiled in Rust code

**Key Compilation Flags:**
```rust
cc::Build::new()
    .file("src/heap_observer_impl.mm")
    .flag("-std=c++17")
    .flag("-fobjc-arc")                    // Automatic Reference Counting
    .flag("-fno-objc-arc-exceptions")      // ARC without exceptions
    .flag("-fvisibility=hidden")           // Hide internal symbols
    .include("include")                    // Include header path
    .flag("-O3")                           // Optimization level
    .compile("heap_observer")              // Output library name
```

**Framework Linking:**
```
-framework Metal
-framework Foundation
-framework IOKit
-framework CoreFoundation
```

### 2. Header File (`include/heap_observer.h`)

Provides the C FFI interface for the Metal heap observer. Contains:

- **FFI-safe structures** (all with `#[repr(C)]` equivalent):
  - `FFIHeapAllocation`: Tracks individual buffer allocations
  - `FFIHeapState`: Snapshot of heap state (size, usage, fragmentation)
  - `FFIFragmentationMetrics`: Detailed fragmentation analysis
  - `FFIMetalMemoryMetrics`: Aggregate memory metrics across all heaps
  - `FFIPageMigrationEvent`: Records page migration events

- **FFI Functions** (C extern):
  - `metal_heap_observer_init()`: Initialize Metal device observation
  - `metal_heap_observe_allocation()`: Record buffer allocation
  - `metal_heap_observe_deallocation()`: Record buffer deallocation
  - `metal_heap_update_state()`: Update heap state after changes
  - `metal_heap_get_fragmentation()`: Calculate fragmentation metrics
  - `metal_heap_get_all_states()`: Retrieve all tracked heap states
  - `metal_heap_get_metrics()`: Get aggregate Metal memory metrics
  - `metal_heap_get_migration_events()`: Query page migration events
  - `metal_heap_clear()`: Clear all recorded observation data
  - `metal_heap_get_last_error()`: Retrieve error messages

### 3. Implementation (`src/heap_observer_impl.mm`)

Objective-C++ implementation that:

1. **Initialization**
   - Checks for Metal-capable device availability
   - Sets up global observation state

2. **Allocation Tracking**
   - Records buffer allocations with timestamps
   - Maintains thread-safe allocation map
   - Tracks heap IDs, sizes, offsets, and storage modes

3. **State Management**
   - Updates heap states after allocation/deallocation
   - Calculates fragmentation metrics
   - Maintains per-heap and aggregate statistics

4. **Error Handling**
   - Thread-local error message buffer
   - All functions return status codes (0 = failure, 1 = success)
   - Variadic error formatting

5. **Thread Safety**
   - `std::mutex` for allocation map protection
   - `std::mutex` for heap state protection
   - `std::atomic<bool>` for initialization flag
   - `thread_local` storage for error messages

### 4. Rust Bindings (`src/heap_observer.rs`)

Provides Rust FFI bindings and safe wrappers:

```rust
#[cfg(target_os = "macos")]
extern "C" {
    pub fn metal_heap_observer_init() -> i32;
    pub fn metal_heap_observe_allocation(...) -> i32;
    // ... other extern declarations
}
```

Includes safe wrapper functions and tests for FFI correctness.

## Cargo.toml Configuration

```toml
[build-dependencies]
cc = "1.0"                           # C/C++ build tool

[target.'cfg(target_os = "macos")'.dependencies]
metal = "0.30"                       # Metal framework bindings
core-foundation = "0.9"              # CoreFoundation bindings
objc = "0.2"                         # Objective-C runtime bindings
libc = "0.2"                         # libc types
```

## Compilation Flow

1. **Cargo invokes build script** (`build.rs`)
2. **build.rs checks platform**:
   - macOS: Compile ObjC++ with cc crate
   - Other: No compilation (stubs only)
3. **Compilation generates**:
   - Static library: `libheap_observer.a`
   - Placed in: `target/<profile>/build/adapteros-memory-*/out/`
4. **Linker combines**:
   - Rust code + static library + framework symbols
5. **Final output**: Binary with Metal heap observation support

## Building

### Full Build
```bash
cargo build -p adapteros-memory
```

### Release Build (with optimizations)
```bash
cargo build --release -p adapteros-memory
```

### Build Script Only
```bash
cargo build --build-plan -p adapteros-memory 2>&1 | grep "heap_observer"
```

## Conditional Compilation

The implementation uses platform detection:

**macOS Only:**
```rust
#[cfg(target_os = "macos")]
extern "C" { /* FFI declarations */ }

#[cfg(target_os = "macos")]
pub fn observe_allocation(...) { /* implementation */ }
```

**Non-macOS Fallback:**
```rust
#[cfg(not(target_os = "macos"))]
pub fn observe_allocation(...) {
    // Return error or no-op
}
```

## Framework Availability

### Required Frameworks
- **Metal**: GPU acceleration and device management
- **Foundation**: Objective-C runtime and utilities
- **IOKit**: Hardware monitoring and page fault detection
- **CoreFoundation**: Low-level system APIs

### Availability Checks
The implementation checks Metal device availability at runtime:
```objc
id<MTLDevice> device = MTLCreateSystemDefaultDevice();
if (!device) {
    // Metal not available
}
```

## Error Handling

All FFI functions return status codes:

| Return Value | Meaning |
|---|---|
| `1` or `>0` | Success |
| `0` | General failure |
| `-1` | Invalid parameters |
| `-2` | Calculation error |

Error messages are stored in thread-local buffer accessible via:
```rust
metal_heap_get_last_error(buffer, buffer_len)
```

## Testing

### Unit Tests in Rust
Located in `src/heap_observer.rs`:
```rust
#[test]
fn test_heap_observer_creation() { }

#[test]
fn test_ffi_fragmentation_metrics() { }

#[test]
fn test_ffi_null_pointer_handling() { }
```

### Compilation Verification
```bash
cargo check -p adapteros-memory
```

### Full Test Suite
```bash
cargo test -p adapteros-memory --lib heap_observer
```

## Performance Considerations

### Compilation Time
- First build: ~5-10 seconds (ObjC++ compilation)
- Incremental: < 1 second (if `.mm` unchanged)

### Runtime Performance
- Metal device detection: < 1ms
- Allocation recording: O(1) with map-based storage
- Fragmentation calculation: O(n allocations)
- Memory overhead: ~100 bytes per allocation + state maps

### Optimization Flags
- `-O3`: Full optimization in release builds
- `-fvisibility=hidden`: Smaller binary, faster linking
- `-fobjc-arc`: Automatic memory management

## Troubleshooting

### Issue: "No Metal-capable device found"
**Cause**: macOS version < 10.11 or no Metal GPU
**Solution**: Update macOS or use CPU fallback

### Issue: Compilation errors
**Check:**
1. Xcode Command Line Tools installed: `xcode-select --install`
2. Metal headers available: `/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX.sdk/System/Library/Frameworks/Metal.framework`
3. C++17 support: Check clang version

### Issue: Linker errors
**Solution:** Ensure frameworks are linked:
```bash
# Check framework paths
xcrun --show-sdk-path
```

## Maintenance

### When to Rebuild
- Changes to `src/heap_observer_impl.mm`
- Changes to `include/heap_observer.h`
- Changes to `build.rs`
- Metal framework API updates

### Continuous Integration
For CI/CD pipelines:
```bash
# macOS agents only
cargo build -p adapteros-memory
cargo test -p adapteros-memory --lib
```

## References

- [Objective-C++ Interop](../docs/OBJECTIVE_CPP_FFI_PATTERNS.md)
- [Metal Framework](https://developer.apple.com/metal/)
- [Rust FFI Best Practices](https://doc.rust-lang.org/nomicon/ffi.html)
- [cc crate Documentation](https://docs.rs/cc/)

## Future Enhancements

1. **IOKit Integration**
   - Detect page migration events via IOKit notifications
   - Track memory pressure changes

2. **Performance Monitoring**
   - Measure compilation time impact
   - Profile Metal device query overhead

3. **Cross-Platform**
   - Android Metal equivalent (Vulkan)
   - Linux Metal support (Vulkan)

4. **Advanced Features**
   - Heap compaction detection
   - GPU memory swapping
   - Determinism attestation

