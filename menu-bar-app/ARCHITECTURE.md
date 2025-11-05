# Menu Bar App Architecture

## Overview

The AdapterOS Menu Bar App is a lightweight macOS menu bar application that displays AdapterOS status by reading JSON written by the control plane. It uses zero network calls, native system APIs for metrics, and provides a clean, distraction-free interface.

## Concurrency Model

### Main Actor Isolation

The `StatusViewModel` is marked with `@MainActor` to ensure all UI updates happen on the main thread. This prevents data races and ensures thread-safe access to published properties.

### Async/Await Pattern

- **StatusReader**: Uses `async/await` with `withCheckedThrowingContinuation` for non-blocking file I/O
- **StatusViewModel**: All refresh operations are async and properly awaited
- **ServicePanelClient**: Uses async/await for HTTP requests with proper error handling

### Serialization Guards

- **Watcher Setup**: Protected by `isSettingUpWatcher` flag to prevent concurrent watcher creation
- **Cache Operations**: Uses concurrent queue with barrier flags for write operations
- **Service Operations**: Circuit breaker pattern prevents cascading failures

## Resource Cleanup Guarantees

### File Descriptors

- **VNODE Watcher**: File descriptors are closed in `setCancelHandler` when source is cancelled
- **StatusReader**: File handles use `defer` to ensure cleanup even on errors
- **Lifecycle**: `deinit` methods properly cancel sources and close descriptors

### Memory Management

- **NSCache**: Automatically evicts entries under memory pressure
- **Delegate Tracking**: `ResponseCache` delegate decrements entry count when NSCache evicts entries
- **Weak References**: All closures use `[weak self]` to prevent retain cycles

### Timer Cleanup

- **Poll Timer**: Cancelled in `stopPolling()` and `deinit`
- **Metrics Timer**: Cancelled in `deinit`
- **Sleep/Wake Observers**: Removed in `deinit`

## Error Handling Strategy

### Error Types

- **StatusReadError**: Enum with specific error cases (fileMissing, decodeFailed, etc.)
- **ServiceError**: Comprehensive HTTP error handling with retry logic
- **Validation Errors**: Structured validation failures with context

### Error Propagation

- **StatusReader**: Returns `Result<T, StatusReadError>` for non-throwing operations
- **Fallback Strategy**: Uses cached status on decode/validation failures
- **Logging**: All errors logged with context using structured logging

### Retry Logic

- **ServicePanelClient**: Exponential backoff with configurable retries
- **Circuit Breaker**: Prevents cascading failures by opening after threshold
- **Watcher Retry**: Exponential backoff for watcher setup failures (max 3 attempts)

## Known Limitations

### File Timeout

- **Timeout**: Default 2 seconds for file reads
- **Rationale**: Prevents blocking UI thread on slow filesystem
- **Behavior**: Returns timeout error, falls back to cached status if available

### Path Discovery

- **Order**: Checks paths in priority order (metadata file → system paths → fallback)
- **Limitation**: Doesn't watch for new paths being created dynamically
- **Workaround**: Polling checks for path changes every 5 seconds

### NSCache Eviction

- **Non-Deterministic**: NSCache eviction timing is not predictable
- **Tracking**: Delegate tracks evictions to maintain accurate count
- **Impact**: Statistics may be approximate under heavy memory pressure

## Performance Characteristics

### Memory Usage

- **Cache Limit**: 100 entries, 1MB total size
- **Entry Tracking**: O(1) operations for add/remove/lookup
- **Statistics**: O(1) calculation (maintains running totals)

### CPU Usage

- **Polling**: 5-second intervals, minimal CPU impact
- **Metrics Sampling**: 10-second intervals
- **File Watching**: Event-driven, zero CPU when idle

### File I/O

- **Non-Blocking**: All file operations use async/await
- **Atomic Reads**: Uses FileHandle for safe concurrent access
- **Hash-Based De-jittering**: Prevents unnecessary UI updates for identical content

## Thread Safety

### Shared State

- **ResponseCache**: Thread-safe using concurrent queue with barriers
- **StatusReader**: Stateless (caching is internal, protected by actor)
- **StatusViewModel**: @MainActor ensures all access is serialized

### Race Condition Prevention

- **Watcher Setup**: Serialization guard prevents concurrent setup
- **Cache Updates**: Barrier operations ensure atomic updates
- **Status Updates**: Hash comparison prevents redundant updates

## Testing Strategy

### Unit Tests

- **StatusReader**: Tests file reading, error handling, caching
- **StatusViewModel**: Tests state management, error suppression
- **ResponseCache**: Tests entry tracking, eviction, statistics
- **ServicePanelClient**: Tests retry logic, circuit breaker, caching

### Integration Tests

- **End-to-End**: Full lifecycle from file creation to deletion
- **Error Recovery**: Corrupt file → cache fallback → restore
- **Concurrency**: Multiple concurrent operations
- **Stress Tests**: Rapid updates, long-running scenarios

### Manual Testing

- **Startup**: App launches, icon appears, status loads
- **Normal Operation**: Status updates, metrics refresh
- **Error Scenarios**: Missing file, corrupted JSON, permission denied
- **Edge Cases**: Sleep/wake, file path changes, timeout

MLNavigator Inc 2025-01-15.

