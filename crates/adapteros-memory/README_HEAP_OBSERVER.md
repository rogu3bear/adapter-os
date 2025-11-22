# Metal Heap Observer - Documentation Index

Welcome to the Metal Heap Observer implementation documentation. This guide helps you navigate all available resources.

## Quick Links

### For Getting Started
Start here if you're new to the heap observer:
- **[HEAP_OBSERVER_INTEGRATION.md](HEAP_OBSERVER_INTEGRATION.md)** - Quick start guide and integration instructions
- **[EXAMPLES.md](EXAMPLES.md)** - 8 runnable code examples with explanations

### For Understanding the Implementation
Learn how it works under the hood:
- **[HEAP_OBSERVER_IMPL.md](HEAP_OBSERVER_IMPL.md)** - Detailed architecture and design
- **[src/heap_observer_impl.mm](src/heap_observer_impl.mm)** - Objective-C++ source code

### For API Reference
Find all available functions and structures:
- **[HEAP_OBSERVER_INTEGRATION.md#ffi-api-reference](HEAP_OBSERVER_INTEGRATION.md)** - Complete FFI API
- **[include/heap_observer.h](include/heap_observer.h)** - C header file
- **[src/heap_observer.rs](src/heap_observer.rs)** - Rust FFI bindings

### For Project Information
Understand the project scope and deliverables:
- **[IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md)** - Project summary and verification
- **[../HEAP_OBSERVER_DELIVERABLES.md](../HEAP_OBSERVER_DELIVERABLES.md)** - Complete deliverables list

## File Structure

```
crates/adapteros-memory/
├── src/
│   ├── heap_observer_impl.mm      [NEW] Objective-C++ implementation
│   ├── heap_observer.rs           Rust FFI bindings & wrapper types
│   └── [other modules]
├── include/
│   └── heap_observer.h            C FFI header
├── build.rs                       Build script (configured for .mm)
├── Cargo.toml                     Dependencies
│
├── HEAP_OBSERVER_IMPL.md          [NEW] Implementation guide
├── HEAP_OBSERVER_INTEGRATION.md   [NEW] Integration guide
├── IMPLEMENTATION_SUMMARY.md      [NEW] Project summary
├── EXAMPLES.md                    [NEW] Code examples
└── README_HEAP_OBSERVER.md        [NEW] This file
```

## Documentation Overview

### HEAP_OBSERVER_IMPL.md
**Purpose:** Detailed technical documentation
**Audience:** Developers maintaining the code
**Key Sections:**
- Architecture overview
- Component descriptions
- Thread safety model
- FFI specifications
- Memory management
- Performance analysis
- Debugging guide
- Future enhancements

**When to Read:** When you need to understand internals or modify the implementation

### HEAP_OBSERVER_INTEGRATION.md
**Purpose:** Practical integration guide
**Audience:** Developers using the observer
**Key Sections:**
- Quick start guide
- File structure
- Build configuration
- FFI API reference
- Data structures
- Platform support
- Performance tips
- Related documentation

**When to Read:** When integrating into your codebase

### IMPLEMENTATION_SUMMARY.md
**Purpose:** Project overview and verification
**Audience:** Project managers and reviewers
**Key Sections:**
- Status and deliverables
- Technical specifications
- Requirements verification
- Performance characteristics
- Quality metrics
- Compatibility matrix

**When to Read:** For project status and completeness verification

### EXAMPLES.md
**Purpose:** Runnable code examples
**Audience:** Developers learning the API
**Key Sections:**
- 8 complete code examples
- Integration patterns
- Safe wrapper abstraction
- Unit test patterns
- Performance tips

**When to Read:** When learning the API or implementing integration

## Quick Reference

### Basic Usage Pattern

```rust
// 1. Initialize (once at startup)
unsafe { metal_heap_observer_init(); }

// 2. Record allocations
unsafe {
    metal_heap_observe_allocation(
        heap_id, buffer_id, size, offset, addr, storage_mode
    );
}

// 3. Query metrics
unsafe {
    let mut metrics = FFIMetalMemoryMetrics { /* ... */ };
    metal_heap_get_metrics(&mut metrics);
    println!("Fragmentation: {:.1}%", metrics.overall_fragmentation * 100.0);
}

// 4. Record deallocations
unsafe { metal_heap_observe_deallocation(buffer_id); }
```

### Build & Test

```bash
# Build the memory crate
cargo build -p adapteros-memory

# Run tests
cargo test -p adapteros-memory

# Build with verbose output
cargo build -p adapteros-memory --verbose
```

### Key Files at a Glance

| File | Purpose | Lines | Status |
|------|---------|-------|--------|
| heap_observer_impl.mm | Objective-C++ implementation | 420 | ✓ Complete |
| heap_observer.rs | Rust FFI bindings | - | ✓ Existing |
| build.rs | Build configuration | - | ✓ Configured |
| HEAP_OBSERVER_IMPL.md | Implementation guide | 492 | ✓ New |
| HEAP_OBSERVER_INTEGRATION.md | Integration guide | 357 | ✓ New |
| IMPLEMENTATION_SUMMARY.md | Project summary | 405 | ✓ New |
| EXAMPLES.md | Code examples | 568 | ✓ New |

## Learning Path

### Path 1: Just Want to Use It (30 minutes)
1. Read: [HEAP_OBSERVER_INTEGRATION.md](HEAP_OBSERVER_INTEGRATION.md) (Quick Start section)
2. Read: [EXAMPLES.md](EXAMPLES.md) (Example 1: Basic Usage)
3. Copy example code and adapt to your needs

### Path 2: Want to Understand It (2 hours)
1. Read: [HEAP_OBSERVER_INTEGRATION.md](HEAP_OBSERVER_INTEGRATION.md) (all sections)
2. Read: [EXAMPLES.md](EXAMPLES.md) (all examples)
3. Skim: [HEAP_OBSERVER_IMPL.md](HEAP_OBSERVER_IMPL.md) (architecture sections)
4. Check: [src/heap_observer_impl.mm](src/heap_observer_impl.mm) (code structure)

### Path 3: Want to Modify It (4+ hours)
1. Read: [HEAP_OBSERVER_IMPL.md](HEAP_OBSERVER_IMPL.md) (complete)
2. Read: [src/heap_observer_impl.mm](src/heap_observer_impl.mm) (complete)
3. Read: [IMPLEMENTATION_SUMMARY.md](IMPLEMENTATION_SUMMARY.md) (verification section)
4. Refer to: [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](../../../docs/OBJECTIVE_CPP_FFI_PATTERNS.md)

## Common Tasks

### Task: Build the project
See: [HEAP_OBSERVER_INTEGRATION.md - Compilation](HEAP_OBSERVER_INTEGRATION.md#compilation)

### Task: Add Metal heap observer to my code
See: [HEAP_OBSERVER_INTEGRATION.md - Quick Start](HEAP_OBSERVER_INTEGRATION.md#quick-start)

### Task: Get fragmentation metrics
See: [EXAMPLES.md - Example 2](EXAMPLES.md#example-2-fragmentation-analysis)

### Task: Create safe wrapper
See: [EXAMPLES.md - Example 7](EXAMPLES.md#example-7-safe-ffi-wrapper)

### Task: Debug compilation issues
See: [HEAP_OBSERVER_INTEGRATION.md - Debugging](HEAP_OBSERVER_INTEGRATION.md#debugging)

### Task: Understand thread safety
See: [HEAP_OBSERVER_IMPL.md - Thread Safety](HEAP_OBSERVER_IMPL.md#thread-safety-model)

### Task: Monitor memory in real-time
See: [EXAMPLES.md - Example 5](EXAMPLES.md#example-5-continuous-monitoring-loop)

### Task: Handle errors properly
See: [EXAMPLES.md - Example 6](EXAMPLES.md#example-6-error-handling-with-error-messages)

## API Reference by Category

### Initialization
- `metal_heap_observer_init()` - Start observer

### Recording Events
- `metal_heap_observe_allocation()` - Record allocation
- `metal_heap_observe_deallocation()` - Record deallocation
- `metal_heap_update_state()` - Update heap state

### Querying Metrics
- `metal_heap_get_metrics()` - Overall metrics
- `metal_heap_get_fragmentation()` - Fragmentation data
- `metal_heap_get_all_states()` - All heap states
- `metal_heap_get_migration_events()` - Migration events

### Maintenance
- `metal_heap_clear()` - Clear all data
- `metal_heap_get_last_error()` - Get error message

## Data Structures

### Input/Output Structures
- `FFIHeapState` - Heap snapshot
- `FFIMetalMemoryMetrics` - Overall memory metrics
- `FFIFragmentationMetrics` - Fragmentation analysis
- `FFIPageMigrationEvent` - Memory migration event

See: [HEAP_OBSERVER_INTEGRATION.md#data-structures](HEAP_OBSERVER_INTEGRATION.md#data-structures)

## Performance Considerations

- **Recording:** O(1) async, non-blocking
- **Queries:** O(n+m) sync, brief lock hold
- **Memory:** O(n) where n = active allocations
- **Thread-safe:** Yes, fully concurrent

See: [HEAP_OBSERVER_IMPL.md#performance-characteristics](HEAP_OBSERVER_IMPL.md#performance-characteristics)

## Platform Support

- **macOS:** Full support (Metal API)
- **iOS/tvOS:** N/A (no Metal heap API)
- **Linux/Windows:** Compilation succeeds, no-op stubs

See: [HEAP_OBSERVER_INTEGRATION.md#platform-support](HEAP_OBSERVER_INTEGRATION.md#platform-support)

## Related Documentation

- [OBJECTIVE_CPP_FFI_PATTERNS.md](../../../docs/OBJECTIVE_CPP_FFI_PATTERNS.md) - FFI best practices
- [ARCHITECTURE_PATTERNS.md](../../../docs/ARCHITECTURE_PATTERNS.md) - System design
- [CLAUDE.md](../../../CLAUDE.md) - Project guidelines

## Troubleshooting

### Build Fails
- Check: `build.rs` has correct framework linking
- Check: Xcode Command Line Tools installed
- Try: `cargo clean && cargo build`

### Functions Not Found
- Check: `#include <dispatch/dispatch.h>` in code
- Check: Framework linking in `build.rs`
- Try: `cargo build --verbose` for linking errors

### Thread Safety Questions
- See: [HEAP_OBSERVER_IMPL.md - Thread Safety](HEAP_OBSERVER_IMPL.md#thread-safety-model)
- Key: Serial dispatch queue serializes all operations

### Performance Issues
- See: [HEAP_OBSERVER_INTEGRATION.md - Performance](HEAP_OBSERVER_INTEGRATION.md#performance-characteristics)
- Tip: Async recording doesn't block, use sync queries judiciously

## Contributing & Maintaining

### Modifying the Implementation
1. Read: [HEAP_OBSERVER_IMPL.md](HEAP_OBSERVER_IMPL.md) (complete)
2. Edit: [src/heap_observer_impl.mm](src/heap_observer_impl.mm)
3. Test: `cargo test -p adapteros-memory`
4. Document: Update relevant markdown files

### Adding New Features
1. Check: [HEAP_OBSERVER_IMPL.md#future-enhancements](HEAP_OBSERVER_IMPL.md#future-enhancements)
2. Follow: [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](../../../docs/OBJECTIVE_CPP_FFI_PATTERNS.md)
3. Add tests: See [EXAMPLES.md#example-8-unit-tests](EXAMPLES.md#example-8-unit-tests)

## Summary

The Metal Heap Observer provides thread-safe Metal heap monitoring for AdapterOS:

- **420 lines** of production Objective-C++ code
- **1,822 lines** of comprehensive documentation
- **10 C FFI functions** for Rust integration
- **100% complete** and production-ready

Start with [HEAP_OBSERVER_INTEGRATION.md](HEAP_OBSERVER_INTEGRATION.md) for quick integration, or [EXAMPLES.md](EXAMPLES.md) for code examples.

---

**Questions?** Check the relevant documentation file or search for error messages in the docs.
