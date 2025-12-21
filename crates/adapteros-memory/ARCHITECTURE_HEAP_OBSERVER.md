# Metal Heap Observer Objective-C++ Implementation

**Status:** Complete and integrated
**File:** `/crates/adapteros-memory/src/heap_observer_impl.mm`
**Build Configuration:** `build.rs` (already configured)
**Header:** `/crates/adapteros-memory/include/heap_observer.h`

## Overview

This document describes the Objective-C++ implementation of the Metal heap observer system. The implementation provides thread-safe Metal heap monitoring, allocation tracking, fragmentation detection, and page migration event recording through C FFI bindings.

## Architecture

### Component Hierarchy

```
C FFI Layer (Rust-callable functions)
         ↓
Objective-C++ Implementation (MetalHeapObserverImpl)
         ↓
Objective-C Helper Classes (HeapAllocationRecord, HeapStateRecord, PageMigrationEventRecord)
         ↓
Apple Frameworks (Metal, Foundation, IOKit, CoreFoundation)
```

### Thread Safety Model

- **Dispatch Queue:** Serial queue (`DISPATCH_QUEUE_SERIAL`) ensures thread-safe access to shared state
- **Synchronous Operations:** `dispatch_sync()` for queries (get_metrics, get_fragmentation, get_all_states)
- **Asynchronous Operations:** `dispatch_async()` for mutations (allocations, deallocations, state updates)
- **Memory Safety:** Automatic Reference Counting (ARC) enabled via `-fobjc-arc` flag

## Core Components

### 1. Private Objective-C Classes

#### HeapAllocationRecord
Represents a single Metal buffer allocation.

**Properties:**
- `heapId` (uint64_t): Identifier of parent heap
- `bufferId` (uint64_t): Unique buffer identifier
- `sizeBytes` (uint64_t): Allocation size in bytes
- `offsetBytes` (uint64_t): Offset within heap
- `memoryAddr` (uint64_t): Virtual memory address
- `timestamp` (uint64_t): Allocation timestamp (mach_absolute_time)
- `storageMode` (uint32_t): MTL storage mode flags

#### HeapStateRecord
Snapshot of heap metrics at a point in time.

**Properties:**
- `heapId` (uint64_t): Heap identifier
- `totalSize` (uint64_t): Total heap size in bytes
- `usedSize` (uint64_t): Currently used size in bytes
- `allocationCount` (uint32_t): Number of active allocations
- `fragmentationRatio` (float): Calculated fragmentation (0.0-1.0)
- `largestFreeBlock` (uint64_t): Size of largest contiguous free region

#### PageMigrationEventRecord
Records a page migration or memory relocation event.

**Properties:**
- `eventIdHigh`, `eventIdLow` (uint64_t): UUID split into two parts
- `migrationType` (uint32_t): Type of migration (1=PageOut, 2=PageIn, 3=BufferRelocate, etc.)
- `sourceAddr`, `destAddr` (uint64_t): Source/destination addresses
- `sizeBytes` (uint64_t): Migrated memory size
- `timestamp` (uint64_t): Event timestamp

### 2. MetalHeapObserverImpl Class

Main observer class managing heap tracking and metrics calculation.

**Instance Variables:**
```objc
id<MTLDevice> metalDevice;                                    // Metal device reference
NSMutableDictionary<NSNumber*, HeapAllocationRecord*>* allocations;  // Buffer ID → allocation
NSMutableDictionary<NSNumber*, HeapStateRecord*>* heapStates;        // Heap ID → state
NSMutableArray<PageMigrationEventRecord*>* migrationEvents;          // Event log
dispatch_queue_t threadSafeQueue;                             // Serial queue
uint64_t nextBufferId;                                        // Buffer ID counter
NSMutableString* lastErrorMessage;                            // Error log
float samplingRate;                                           // Sampling probability
```

**Key Methods:**

##### Initialization
```objc
- (instancetype)initWithDevice:(id<MTLDevice>)device
                   samplingRate:(float)rate
```
- Sets up Metal device reference
- Initializes containers (NSMutableDictionary, NSMutableArray)
- Creates dispatch queue with label "com.adapteros.metal.heap.observer"
- Clamps sampling rate to [0.0, 1.0]

##### Allocation Tracking
```objc
- (void)recordAllocation:(uint64_t)heapId
               bufferId:(uint64_t)bufferId
                   size:(uint64_t)size
                 offset:(uint64_t)offset
                  addr:(uint64_t)addr
           storageMode:(uint32_t)storageMode
```
- Submits allocation record asynchronously to queue
- Skips recording if sampling check fails
- Creates HeapAllocationRecord with current timestamp

##### Deallocation Tracking
```objc
- (void)recordDeallocation:(uint64_t)bufferId
```
- Removes allocation from tracking dictionary
- Triggers page migration check for large allocations (>1MB)
- Detects very large allocations (>10MB) as potential migration events

##### State Updates
```objc
- (void)updateHeapState:(uint64_t)heapId
              totalSize:(uint64_t)totalSize
               usedSize:(uint64_t)usedSize
```
- Creates HeapStateRecord with calculated fragmentation
- Counts allocations belonging to heap
- Calculates fragmentation ratio: `1.0 - (usedSize / totalSize)`

##### Metrics Calculation
```objc
- (int)getFragmentationMetrics:(FFIFragmentationMetrics*)outMetrics
- (int)getAllHeapStates:(FFIHeapState*)outHeaps maxHeaps:(uint32_t)maxHeaps
- (int)getMetrics:(FFIMetalMemoryMetrics*)outMetrics
- (int)getMigrationEvents:(FFIPageMigrationEvent*)outEvents maxEvents:(uint32_t)maxEvents
```
- All queries use `dispatch_sync()` for consistency
- Calculate metrics synchronously to ensure valid snapshots
- Handle buffer overflow gracefully (count vs. max_heaps)

## FFI C Entry Points

All functions are `extern "C"` and callable from Rust:

### Lifecycle

```c
int metal_heap_observer_init()
```
- Uses `dispatch_once()` to ensure singleton initialization
- Creates system default Metal device
- Returns 1 on success, 0 on failure

### Observation

```c
int metal_heap_observe_allocation(heap_id, buffer_id, size, offset, addr, storage_mode)
int metal_heap_observe_deallocation(buffer_id)
int metal_heap_update_state(heap_id, total_size, used_size)
```
- Delegates to MetalHeapObserverImpl methods
- Returns 1 on success, 0 on failure

### Metrics Queries

```c
int metal_heap_get_fragmentation(heap_id, out_metrics)
int metal_heap_get_all_states(out_heaps, max_heaps)
int metal_heap_get_metrics(out_metrics)
int metal_heap_get_migration_events(out_events, max_events)
```
- Returns 0 on success, negative on error
- Safely handles null pointers (-1)

### Utility

```c
int metal_heap_clear()
size_t metal_heap_get_last_error(buffer, buffer_len)
```

## Build Configuration

### Compilation Flags

**Objective-C/C++ Options:**
- `-std=c++17`: C++17 standard
- `-fobjc-arc`: Automatic Reference Counting
- `-fno-objc-arc-exceptions`: Disable ARC exception handling
- `-fvisibility=hidden`: Hide symbols by default

**Framework Linking:**
- `-framework Metal`: Metal API
- `-framework Foundation`: Foundation classes (NSString, NSDictionary, etc.)
- `-framework IOKit`: Memory pressure callbacks (future)
- `-framework CoreFoundation`: Core Foundation utilities

**Optimization & Warnings:**
- `-O3`: Full optimization
- `-Wall -Wextra -Werror`: Strict warnings
- `-Wno-deprecated-declarations`: Allow deprecated Metal APIs

### Build Script (build.rs)

```rust
#[cfg(target_os = "macos")]
fn main() {
    println!("cargo:rerun-if-changed=src/heap_observer_impl.mm");

    cc::Build::new()
        .file("src/heap_observer_impl.mm")
        .flag("-std=c++17")
        .flag("-fobjc-arc")
        // ... (flags as above)
        .compile("heap_observer");

    println!("cargo:rustc-link-lib=framework=Metal");
    // ... (framework links)
}
```

## Memory Management Strategy

### Objective-C Lifetime

1. **Allocation Recording:**
   - `HeapAllocationRecord` created in async block
   - Stored in NSMutableDictionary (retained)
   - Removed on deallocation (released)

2. **Observer Instance:**
   - Singleton created in `dispatch_once()`
   - Retained globally
   - Never explicitly released (intentional leak, singleton pattern)

3. **Device Reference:**
   - Metal device released immediately after assignment to ivar
   - Device lifetime managed by Metal runtime

4. **Error Messages:**
   - NSMutableString stored as ivar
   - Retained for observer lifetime
   - Updated asynchronously

### ARC Behavior

- Automatic Reference Counting (ARC) enabled via `-fobjc-arc`
- Release pools created automatically for async blocks
- Object cleanup happens after block execution
- No manual retain/release needed for most objects

## Fragmentation Detection

### Calculation Process

1. **Aggregate Allocations:**
   - Sum all buffer sizes = `totalAllocated`
   - Sum heap sizes = `totalHeapSize`

2. **Free Space Detection:**
   - Calculate free per heap: `heapSize - heapUsedSize`
   - Count free blocks: number of heaps with free space

3. **Fragmentation Metrics:**

```
externalFragmentation = totalFreeBytes / totalHeapSize
internalFragmentation = 0.05 (estimated 5% allocation overhead)
fragmentationRatio = (external + internal) / 2.0
```

4. **Compaction Efficiency:**

```
maxRecoverable = totalFreeBytes - largestFreeBlock
compactionEfficiency = 1.0 - (maxRecoverable / totalFreeBytes)
```

## Page Migration Detection

### Detection Strategy

Currently uses heuristic-based detection:

1. **Deallocation Triggers Check:**
   - When buffer deallocated, check size

2. **Size-Based Heuristics:**
   - >1MB: Candidate for migration
   - >10MB: Likely migration event

### Future Enhancement

Could integrate with:
- IOKit memory pressure callbacks
- Metal memory pressure events
- System memory statistics
- Xcode Instruments integration

## Error Handling

### Return Values

- **Success:** Return value > 0 or == 0 (function-specific)
- **Null Pointer Error:** Return -1
- **Invalid Parameter:** Return -1 or -2 (function-specific)
- **Calculation Error:** Return -2

### Error Messages

- Logged via `lastErrorMessage` ivar
- Retrievable via `metal_heap_get_last_error(buffer, len)`
- Max 256 characters per message

## Performance Characteristics

### Time Complexity

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| recordAllocation | O(1) | Dictionary insert, async |
| recordDeallocation | O(1) | Dictionary lookup/remove, async |
| getFragmentationMetrics | O(n) | n = number of heaps, sync |
| getAllHeapStates | O(m) | m = number of heaps, sync |
| getMetrics | O(n+m) | n = allocations, m = heaps, sync |
| getMigrationEvents | O(k) | k = events recorded, sync |

### Space Complexity

- Allocations: O(n) where n = active buffers
- Heap states: O(m) where m = active heaps
- Migration events: O(k) unbounded (circular buffer future improvement)

### Dispatch Queue Overhead

- Serial queue ensures thread safety but serializes operations
- Async mutations have minimal blocking
- Sync queries block briefly for snapshot consistency

## Testing Considerations

### Unit Testing

Would need to mock Metal objects since testing on non-macOS is impractical.

**Test Vectors:**

1. **Basic Allocation/Deallocation:**
   - Record multiple allocations
   - Verify counts in metrics
   - Clear state

2. **Fragmentation Scenarios:**
   - Contiguous allocations (low fragmentation)
   - Interleaved allocations (high fragmentation)
   - Large gaps (external fragmentation)

3. **Error Handling:**
   - Null pointer parameters
   - Invalid heap IDs
   - Buffer overflow conditions

4. **Thread Safety:**
   - Concurrent record/query operations
   - Verify no data races with Thread Sanitizer

### Integration Testing

```rust
#[cfg(target_os = "macos")]
#[test]
fn test_ffi_metal_heap_observer() {
    unsafe {
        metal_heap_observer_init();
        metal_heap_observe_allocation(1, 100, 1024, 0, 0x1000, 1);

        let mut metrics = FFIMetalMemoryMetrics {
            total_allocated: 0,
            // ... initialize fields
        };

        let result = metal_heap_get_metrics(&mut metrics);
        assert_eq!(result, 0);
        assert_eq!(metrics.allocation_count, 1);
        assert_eq!(metrics.total_allocated, 1024);
    }
}
```

## Platform-Specific Considerations

### macOS (Target Support)

- Full Metal API available
- Foundation framework available
- Dispatch library integrated with system
- Mach time for timestamps

### Non-macOS (Stub Implementation)

The Rust heap_observer.rs provides stub implementations that:
- Return success codes without actual tracking
- Maintain API compatibility
- Allow cross-platform compilation

## Future Enhancements

### 1. Circular Event Buffer

```objc
@property (nonatomic) NSMutableArray<PageMigrationEventRecord*>* migrationEvents;
// Could be replaced with:
@property (nonatomic) NSMutableArray<PageMigrationEventRecord*>* migrationEvents;
@property (nonatomic) NSUInteger maxEventsCapacity;
```

### 2. Memory Pressure Integration

```objc
// In initWithDevice:samplingRate:
dispatch_source_t source = dispatch_source_create(
    DISPATCH_SOURCE_TYPE_MEMORYPRESSURE,
    0, 0, threadSafeQueue
);
dispatch_source_set_event_handler(source, ^{
    // Handle memory pressure
});
dispatch_activate(source);
```

### 3. Xcode Metrics Integration

```c
// Export metrics in Instruments format
void metal_heap_export_instruments_data(char* output_buffer, size_t max_size);
```

### 4. Timestamp Improvements

Replace `mach_absolute_time()` with:
```objc
uint64_t getMicrosecondsSinceEpoch() {
    return (uint64_t)([[NSDate date] timeIntervalSince1970] * 1_000_000);
}
```

## Debugging

### GDB Breakpoints

```gdb
(lldb) br set -n "metal_heap_observe_allocation"
(lldb) br set -n "recordAllocation:"
```

### LLDB Inspecting State

```
(lldb) po [[[g_observer allocations] allKeys] count]
(lldb) po [[g_observer heapStates] objectForKey:@(1)]
```

### Memory Debugging

```bash
# Address Sanitizer
ASAN_OPTIONS=detect_leaks=1 cargo test

# Thread Sanitizer
TSAN_OPTIONS=sanitize_stack=1 cargo test
```

## References

- **FFI Header:** `/crates/adapteros-memory/include/heap_observer.h`
- **Rust Interface:** `/crates/adapteros-memory/src/heap_observer.rs` (FFI declarations)
- **Build Script:** `/crates/adapteros-memory/build.rs`
- **Metal Documentation:** [Apple Metal Programming Guide](https://developer.apple.com/documentation/metal)
- **Objective-C FFI Guide:** [docs/OBJECTIVE_CPP_FFI_PATTERNS.md](docs/OBJECTIVE_CPP_FFI_PATTERNS.md)

## Summary

The Objective-C++ Metal heap observer implementation provides:

1. **Thread-Safe Heap Monitoring:** Serial dispatch queue ensures data consistency
2. **Allocation Tracking:** Maintains dictionary of active buffers per heap
3. **Fragmentation Detection:** Calculates external/internal fragmentation ratios
4. **Page Migration Tracking:** Detects large allocations as migration candidates
5. **FFI Integration:** Clean C interface callable from Rust
6. **Memory Safety:** ARC-managed lifetime, no manual memory management needed
7. **Performance:** O(1) recording, O(n) queries with proper async/sync separation

All FFI functions are implemented according to specifications in `heap_observer.rs` and match the C header signatures in `include/heap_observer.h`.
