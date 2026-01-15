# Metal Heap Observer - Documentation Index

Welcome to the Metal Heap Observer implementation documentation. This guide helps you navigate all available resources.

## Quick Links

### For Getting Started
Start here if you're new to the heap observer:
- **[INTEGRATION_HEAP_OBSERVER.md](INTEGRATION_HEAP_OBSERVER.md)** - Quick start guide and integration instructions
- **[EXAMPLES_HEAP_OBSERVER.md](EXAMPLES_HEAP_OBSERVER.md)** - 8 runnable code examples with explanations

### For Understanding the Implementation
Learn how it works under the hood:
- **[ARCHITECTURE_HEAP_OBSERVER.md](ARCHITECTURE_HEAP_OBSERVER.md)** - Detailed architecture and design
- **[src/heap_observer_impl.mm](src/heap_observer_impl.mm)** - Objective-C++ source code

### For API Reference
Find all available functions and structures:
- **[INTEGRATION_HEAP_OBSERVER.md#ffi-api-reference](INTEGRATION_HEAP_OBSERVER.md)** - Complete FFI API
- **[include/heap_observer.h](include/heap_observer.h)** - C header file
- **[src/heap_observer.rs](src/heap_observer.rs)** - Rust FFI bindings

### For Project Information
Understand the project scope and deliverables:
- **[SUMMARY_IMPLEMENTATION.md](SUMMARY_IMPLEMENTATION.md)** - Project summary and verification
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
├── ARCHITECTURE_HEAP_OBSERVER.md  [NEW] Implementation guide
├── INTEGRATION_HEAP_OBSERVER.md   [NEW] Integration guide
├── SUMMARY_IMPLEMENTATION.md      [NEW] Project summary
├── EXAMPLES_HEAP_OBSERVER.md      [NEW] Code examples
└── GUIDE_HEAP_OBSERVER.md         [NEW] This file
```

## Documentation Overview

### ARCHITECTURE_HEAP_OBSERVER.md
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

### INTEGRATION_HEAP_OBSERVER.md
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

### SUMMARY_IMPLEMENTATION.md
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

### EXAMPLES_HEAP_OBSERVER.md
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
| ARCHITECTURE_HEAP_OBSERVER.md | Implementation guide | 492 | ✓ New |
| INTEGRATION_HEAP_OBSERVER.md | Integration guide | 357 | ✓ New |
| SUMMARY_IMPLEMENTATION.md | Project summary | 405 | ✓ New |
| EXAMPLES_HEAP_OBSERVER.md | Code examples | 568 | ✓ New |

## Learning Path

### Path 1: Just Want to Use It (30 minutes)
1. Read: [INTEGRATION_HEAP_OBSERVER.md](INTEGRATION_HEAP_OBSERVER.md) (Quick Start section)
2. Read: [EXAMPLES_HEAP_OBSERVER.md](EXAMPLES_HEAP_OBSERVER.md) (Example 1: Basic Usage)
3. Copy example code and adapt to your needs

### Path 2: Want to Understand It (2 hours)
1. Read: [INTEGRATION_HEAP_OBSERVER.md](INTEGRATION_HEAP_OBSERVER.md) (all sections)
2. Read: [EXAMPLES_HEAP_OBSERVER.md](EXAMPLES_HEAP_OBSERVER.md) (all examples)
3. Skim: [ARCHITECTURE_HEAP_OBSERVER.md](ARCHITECTURE_HEAP_OBSERVER.md) (architecture sections)
4. Check: [src/heap_observer_impl.mm](src/heap_observer_impl.mm) (code structure)

### Path 3: Want to Modify It (4+ hours)
1. Read: [ARCHITECTURE_HEAP_OBSERVER.md](ARCHITECTURE_HEAP_OBSERVER.md) (complete)
2. Read: [src/heap_observer_impl.mm](src/heap_observer_impl.mm) (complete)
3. Read: [SUMMARY_IMPLEMENTATION.md](SUMMARY_IMPLEMENTATION.md) (verification section)
4. Refer to: [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](../../../docs/OBJECTIVE_CPP_FFI_PATTERNS.md)

## Common Tasks

### Task: Build the project
See: [INTEGRATION_HEAP_OBSERVER.md - Compilation](INTEGRATION_HEAP_OBSERVER.md#compilation)

### Task: Add Metal heap observer to my code
See: [INTEGRATION_HEAP_OBSERVER.md - Quick Start](INTEGRATION_HEAP_OBSERVER.md#quick-start)

### Task: Get fragmentation metrics
See: [EXAMPLES_HEAP_OBSERVER.md - Example 2](EXAMPLES_HEAP_OBSERVER.md#example-2-fragmentation-analysis)

### Task: Create safe wrapper
See: [EXAMPLES_HEAP_OBSERVER.md - Example 7](EXAMPLES_HEAP_OBSERVER.md#example-7-safe-ffi-wrapper)

### Task: Debug compilation issues
See: [INTEGRATION_HEAP_OBSERVER.md - Debugging](INTEGRATION_HEAP_OBSERVER.md#debugging)

### Task: Understand thread safety
See: [ARCHITECTURE_HEAP_OBSERVER.md - Thread Safety](ARCHITECTURE_HEAP_OBSERVER.md#thread-safety-model)

### Task: Monitor memory in real-time
See: [EXAMPLES_HEAP_OBSERVER.md - Example 5](EXAMPLES_HEAP_OBSERVER.md#example-5-continuous-monitoring-loop)

### Task: Handle errors properly
See: [EXAMPLES_HEAP_OBSERVER.md - Example 6](EXAMPLES_HEAP_OBSERVER.md#example-6-error-handling-with-error-messages)

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

See: [INTEGRATION_HEAP_OBSERVER.md#data-structures](INTEGRATION_HEAP_OBSERVER.md#data-structures)

## Performance Considerations

- **Recording:** O(1) async, non-blocking
- **Queries:** O(n+m) sync, brief lock hold
- **Memory:** O(n) where n = active allocations
- **Thread-safe:** Yes, fully concurrent

See: [ARCHITECTURE_HEAP_OBSERVER.md#performance-characteristics](ARCHITECTURE_HEAP_OBSERVER.md#performance-characteristics)

## Platform Support

- **macOS:** Full support (Metal API)
- **iOS/tvOS:** N/A (no Metal heap API)
- **Linux/Windows:** Compilation succeeds, no-op stubs

See: [INTEGRATION_HEAP_OBSERVER.md#platform-support](INTEGRATION_HEAP_OBSERVER.md#platform-support)

## Related Documentation

- [OBJECTIVE_CPP_FFI_PATTERNS.md](../../../docs/OBJECTIVE_CPP_FFI_PATTERNS.md) - FFI best practices
- [ARCHITECTURE.md#architecture-components](../../../docs/ARCHITECTURE.md#architecture-components) - System design
- [AGENTS.md](../../../AGENTS.md) - Project guidelines

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
- See: [ARCHITECTURE_HEAP_OBSERVER.md - Thread Safety](ARCHITECTURE_HEAP_OBSERVER.md#thread-safety-model)
- Key: Serial dispatch queue serializes all operations

### Performance Issues
- See: [INTEGRATION_HEAP_OBSERVER.md - Performance](INTEGRATION_HEAP_OBSERVER.md#performance-characteristics)
- Tip: Async recording doesn't block, use sync queries judiciously

## Contributing & Maintaining

### Modifying the Implementation
1. Read: [ARCHITECTURE_HEAP_OBSERVER.md](ARCHITECTURE_HEAP_OBSERVER.md) (complete)
2. Edit: [src/heap_observer_impl.mm](src/heap_observer_impl.mm)
3. Test: `cargo test -p adapteros-memory`
4. Document: Update relevant markdown files

### Adding New Features
1. Check: [ARCHITECTURE_HEAP_OBSERVER.md#future-enhancements](ARCHITECTURE_HEAP_OBSERVER.md#future-enhancements)
2. Follow: [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](../../../docs/OBJECTIVE_CPP_FFI_PATTERNS.md)
3. Add tests: See [EXAMPLES_HEAP_OBSERVER.md#example-8-unit-tests](EXAMPLES_HEAP_OBSERVER.md#example-8-unit-tests)

## Summary

The Metal Heap Observer provides thread-safe Metal heap monitoring for adapterOS:

- **420 lines** of production Objective-C++ code
- **1,822 lines** of comprehensive documentation
- **10 C FFI functions** for Rust integration
- **100% complete** and production-ready

Start with [INTEGRATION_HEAP_OBSERVER.md](INTEGRATION_HEAP_OBSERVER.md) for quick integration, or [EXAMPLES_HEAP_OBSERVER.md](EXAMPLES_HEAP_OBSERVER.md) for code examples.

---

**Questions?** Check the relevant documentation file or search for error messages in the docs.
