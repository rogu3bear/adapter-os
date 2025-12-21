# Menu Bar App Testing Guide

## Overview

This document describes how to run tests, understand test coverage, and perform manual testing for the AdapterOS Menu Bar App.

## Running Tests

### All Tests

```bash
cd menu-bar-app
swift test
```

### Specific Test Suite

```bash
# StatusReader tests
swift test --filter StatusReaderTests

# StatusViewModel tests
swift test --filter StatusViewModelTests

# ResponseCache tests
swift test --filter ResponseCacheTests

# ServicePanelClient tests
swift test --filter ServicePanelClientTests

# Integration tests
swift test --filter IntegrationTests
```

### With Output

```bash
swift test --filter StatusReaderTests -- --verbose
```

## Test Coverage Overview

### Phase 1: Unit Tests

#### StatusReader Tests (10 tests)
- ✅ `testConcurrentReads` - Concurrent read operations don't conflict
- ✅ `testCachedStatusFallback` - Cached status used on decode failures
- ✅ `testCachedHashPreserved` - Hash preserved for cached fallback
- ✅ `testFileTimeout` - Timeout triggers correctly
- ✅ `testTimeoutRaceCondition` - Only one resume occurs with timeout
- ✅ `testFileLockedError` - Locked file detection
- ✅ `testConsecutiveErrors` - Error suppression after successful read
- ✅ `testPersistentErrorDetection` - Multiple consecutive failures show error
- ✅ `testDecodeErrorPreservation` - Decode error context logged
- ✅ `testMultiplePathDiscovery` - All fallback paths checked

#### StatusViewModel Tests (7 tests)
- ✅ `testWatcherSetupPreventsRaces` - Concurrent watcher setup prevented
- ✅ `testWatcherFileDescriptorCleanup` - File descriptors closed on errors
- ✅ `testConsecutiveErrorSuppressionReal` - Transient error logic
- ✅ `testInitCoordination` - Service refresh not duplicated
- ✅ `testWatcherRecreation` - Pending recreate tasks cancelled
- ✅ `testServiceOperationRacePrevention` - Concurrent operations blocked
- ✅ `testServiceOperationCancellation` - State reset on task cancellation

#### ResponseCache Tests (5 tests)
- ✅ `testEntryCountAccuracy` - Count increments/decrements correctly
- ✅ `testNSCacheEvictionTracking` - Delegate decrements count
- ✅ `testCacheStatistics` - Statistics return accurate count
- ✅ `testConcurrentCacheOperations` - Thread safety
- ✅ `testTTLExpiration` - Entries expire correctly

#### ServicePanelClient Tests (3 additional tests)
- ✅ `testConcurrentServiceOperations` - Retry/race handling
- ✅ `testCircuitBreakerBehavior` - Circuit breaker prevents cascades
- ✅ `testCacheIntegration` - Response caching works

### Phase 2: Integration Tests

#### End-to-End Scenarios (6 tests)
- ✅ `testFullLifecycleStatusReading` - Start → Read → Update → Delete → Error
- ✅ `testErrorRecoveryScenario` - Valid → Corrupt → Cache → Restore
- ✅ `testRapidFileUpdates` - Frequent updates handled gracefully
- ✅ `testCacheEvictionUnderPressure` - NSCache eviction → Statistics accurate
- ✅ `testMultipleServiceOperations` - Start/stop services concurrently
- ✅ `testTimeoutHandling` - Slow file read → Timeout → No crash

#### Stress Tests (3 tests)
- ✅ `testStressRapidFileUpdates` - Very rapid updates (100 iterations)
- ✅ `testStressConcurrentServiceOperations` - Many concurrent operations (100)
- ✅ `testLongRunningAppLifecycle` - Extended operation (50 operations)

## Manual Testing Procedures

### Startup Testing

1. **Launch App**
   ```bash
   cd menu-bar-app
   swift run
   ```

2. **Verify Initialization**
   - Menu bar icon appears within 5 seconds
   - Icon shows correct initial state (OFFLINE if no status file)
   - Tooltip displays "AdapterOS OFFLINE" or current status

3. **Check Status File Discovery**
   - Status file should be discovered across all paths
   - Watcher setup succeeds or falls back gracefully

### Normal Operation Testing

1. **Status Updates**
   - Create status file: `/var/run/adapteros_status.json`
   - Verify icon updates within 5 seconds
   - Verify tooltip shows current status
   - Update status file, verify changes reflected

2. **Metrics Collection**
   - CPU/RAM metrics update every 10 seconds
   - Icon changes to flame when CPU > 80%
   - Metrics match Activity Monitor

3. **Service Status**
   - Service status polls every 15 seconds (if implemented)
   - Failed services show error icon

### Error Scenarios

1. **Missing Status File**
   - Remove status file
   - Verify shows "OFFLINE" state
   - Verify tooltip indicates missing file

2. **Corrupted JSON**
   - Write invalid JSON to status file
   - Verify uses cached status (if available)
   - Verify shows error state after cache expires

3. **Permission Denied**
   - Change file permissions to 000
   - Verify shows permission error
   - Restore permissions, verify recovery

4. **File Timeout**
   - Create slow-responding file system scenario
   - Verify timeout triggers after 2 seconds
   - Verify fallback to cached status

### Edge Cases

1. **Sleep/Wake**
   - Put Mac to sleep
   - Wake Mac
   - Verify watcher recreates correctly
   - Verify status resumes reading

2. **File Path Changes**
   - Server switches to fallback path
   - Verify menu bar app discovers new path
   - Verify watcher recreated for new path

3. **Rapid Updates**
   - Update status file rapidly (10+ times/second)
   - Verify no crashes or race conditions
   - Verify eventual consistency

## Performance Benchmarks

### Memory Usage

- **Baseline**: ~5-10 MB (menu bar app)
- **With Cache**: ~10-15 MB (100 entries)
- **Under Pressure**: Cache evicts, memory stabilizes

### CPU Usage

- **Idle**: < 1% CPU
- **Polling**: ~0.5% CPU during 5s intervals
- **File Reading**: < 5% CPU during read operations

### File I/O

- **Read Latency**: < 10ms for typical status file
- **Timeout**: 2 seconds default
- **Throughput**: Handles 100+ reads/second without issues

## Debugging Failed Tests

### Common Issues

1. **File Permission Errors**
   - Ensure test files are writable
   - Check temporary directory permissions

2. **Timing Issues**
   - Increase timeout values for slow systems
   - Add delays where necessary for async operations

3. **Concurrent Test Failures**
   - Isolate tests that share state
   - Use unique file paths per test

### Running Single Test

```bash
swift test --filter StatusReaderTests.testConcurrentReads
```

### Debug Logging

Tests use structured logging. To see debug output:

```bash
swift test --filter StatusReaderTests -- --verbose 2>&1 | grep -i "debug\|error"
```

## Continuous Integration

### Test Command

```bash
swift test
```

### Coverage Report

```bash
# Requires Xcode
xcodebuild test -scheme AdapterOSMenu -destination 'platform=macOS'
```

## Test Maintenance

### Adding New Tests

1. Add test method to appropriate test class
2. Follow naming convention: `testFeatureName`
3. Use async/await for async operations
4. Clean up resources in `defer` or `tearDown`

### Test Organization

- **Unit Tests**: Test individual components in isolation
- **Integration Tests**: Test component interactions
- **Stress Tests**: Test under extreme conditions

### Test Data

- Use temporary files for file-based tests
- Clean up in `defer` blocks
- Use unique file names (UUID) to avoid conflicts

MLNavigator Inc 2025-01-15.

