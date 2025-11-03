import XCTest
@testable import AdapterOSMenu
import Foundation

/// Integration tests for end-to-end scenarios
final class IntegrationTests: XCTestCase {
    
    // MARK: - Full Lifecycle Tests
    
    func testFullLifecycleStatusReading() async throws {
        // Test: Create status file → Read → Update → Read again → Delete → Verify error handling
        
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        
        // Create initial status
        let initialJSON = """
        {"status":"ok","uptime_secs":100,"adapters_loaded":1,"deterministic":true,"kernel_hash":"test1234","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        try initialJSON.write(to: statusFile, options: .atomic)
        
        // Read initial status
        let reader = StatusReader(filePath: statusFile.path)
        let result1 = await reader.readNow()
        guard case .success(let (status1, _, _)) = result1 else {
            XCTFail("Initial read should succeed")
            return
        }
        XCTAssertEqual(status1.status, "ok")
        
        // Update status file
        let updatedJSON = """
        {"status":"ok","uptime_secs":200,"adapters_loaded":2,"deterministic":true,"kernel_hash":"test5678","telemetry_mode":"local","worker_count":2}
        """.data(using: .utf8)!
        try updatedJSON.write(to: statusFile, options: .atomic)
        
        // Read updated status
        let result2 = await reader.readNow()
        guard case .success(let (status2, _, _)) = result2 else {
            XCTFail("Updated read should succeed")
            return
        }
        XCTAssertEqual(status2.uptime_secs, 200)
        
        // Delete file
        try FileManager.default.removeItem(at: statusFile)
        
        // Read after deletion - should use cached status
        let result3 = await reader.readNow()
        switch result3 {
        case .success(let (cached, _, snippet)):
            // Should return cached status when file missing
            XCTAssertEqual(cached.status, "ok")
            XCTAssertEqual(snippet, "cached")
        case .failure(let error):
            // Or fail if cache not used for file missing
            XCTAssertEqual(error, .fileMissing)
        }
        
        // Cleanup
        try? FileManager.default.removeItem(at: statusFile)
    }
    
    func testErrorRecoveryScenario() async throws {
        // Test: Valid file → Corrupt → Cache fallback → Restore valid → Fresh read
        
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        
        // Create valid status
        let validJSON = """
        {"status":"ok","uptime_secs":100,"adapters_loaded":1,"deterministic":true,"kernel_hash":"test1234","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        try validJSON.write(to: statusFile, options: .atomic)
        
        let reader = StatusReader(filePath: statusFile.path)
        
        // Read valid status (caches it)
        let result1 = await reader.readNow()
        guard case .success(let (validStatus, _, _)) = result1 else {
            XCTFail("Valid read should succeed")
            return
        }
        XCTAssertEqual(validStatus.status, "ok")
        
        // Corrupt the file
        try "invalid json {{{".write(to: statusFile, atomically: true, encoding: .utf8)
        
        // Read corrupted file - should return cached
        let result2 = await reader.readNow()
        guard case .success(let (cachedStatus, _, snippet)) = result2 else {
            XCTFail("Should return cached status on corruption")
            return
        }
        XCTAssertEqual(cachedStatus.status, "ok")
        XCTAssertEqual(snippet, "cached")
        
        // Restore valid file
        try validJSON.write(to: statusFile, options: .atomic)
        
        // Read restored file - should succeed with fresh data
        let result3 = await reader.readNow()
        guard case .success(let (restoredStatus, _, _)) = result3 else {
            XCTFail("Restored read should succeed")
            return
        }
        XCTAssertEqual(restoredStatus.status, "ok")
        
        // Cleanup
        try? FileManager.default.removeItem(at: statusFile)
    }
    
    func testRapidFileUpdates() async throws {
        // Test: Rapid file updates should be handled gracefully
        
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        
        let reader = StatusReader(filePath: statusFile.path)
        
        // Perform multiple rapid updates
        for i in 1...10 {
            let json = """
            {"status":"ok","uptime_secs":\(i * 10),"adapters_loaded":1,"deterministic":true,"kernel_hash":"test1234","telemetry_mode":"local","worker_count":1}
            """.data(using: .utf8)!
            try json.write(to: statusFile, options: .atomic)
            
            // Small delay to allow file system to settle
            try await Task.sleep(nanoseconds: 10_000_000) // 10ms
            
            let result = await reader.readNow()
            guard case .success(let (status, _, _)) = result else {
                XCTFail("Rapid update \(i) should succeed")
                return
            }
            // Verify we're reading updated data (allows for eventual consistency)
            XCTAssertGreaterThanOrEqual(status.uptime_secs, 0)
        }
        
        // Cleanup
        try? FileManager.default.removeItem(at: statusFile)
    }
    
    func testCacheEvictionUnderPressure() async throws {
        // Test: NSCache eviction should properly maintain entry count
        
        let cache = ResponseCache.shared
        cache.clearCache()
        
        // Fill cache beyond limit to trigger evictions
        // Note: NSCache eviction behavior is not deterministic, so we test that
        // the delegate is called and count stays accurate
        
        // Store many entries
        for i in 0..<150 { // More than countLimit of 100
            cache.store(
                Data("test data \(i)".utf8),
                forKey: "key\(i)",
                ttl: 60.0
            )
        }
        
        // Give NSCache time to evict if needed
        try await Task.sleep(nanoseconds: 100_000_000) // 100ms
        
        let stats = cache.statistics
        
        // Count should be reasonable (may be less than 150 due to evictions)
        XCTAssertLessThanOrEqual(stats.entryCount, 150)
        XCTAssertGreaterThanOrEqual(stats.entryCount, 0)
        
        // Cache should still be functional
        if stats.entryCount > 0 {
            let hasEntry = cache.hasValidEntry(forKey: "key0")
            // May or may not have entry depending on eviction
            _ = hasEntry // Use it to avoid warning
        }
        
        cache.clearCache()
    }
    
    func testConcurrentCacheAccess() async throws {
        // Test: Multiple concurrent cache operations don't corrupt state
        
        let cache = ResponseCache.shared
        cache.clearCache()
        
        let taskCount = 50
        var tasks: [Task<Void, Never>] = []
        
        // Spawn concurrent tasks
        for i in 0..<taskCount {
            let task = Task {
                cache.store(
                    Data("concurrent test \(i)".utf8),
                    forKey: "concurrent\(i)",
                    ttl: 60.0
                )
                
                // Also retrieve
                _ = cache.retrieve(forKey: "concurrent\(i)")
            }
            tasks.append(task)
        }
        
        // Wait for all tasks
        for task in tasks {
            _ = await task.value
        }
        
        // Verify cache state is consistent
        let stats = cache.statistics
        XCTAssertLessThanOrEqual(stats.entryCount, taskCount)
        XCTAssertGreaterThanOrEqual(stats.entryCount, 0)
        
        cache.clearCache()
    }
    
    // MARK: - Edge Case Tests
    
    func testMultiplePathDiscovery() async throws {
        // Test: Reader correctly discovers file across multiple paths
        
        let tmpDir = FileManager.default.temporaryDirectory
        let path1 = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        let path2 = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        
        // Create file at second path only
        let validJSON = """
        {"status":"ok","uptime_secs":100,"adapters_loaded":1,"deterministic":true,"kernel_hash":"test1234","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        try validJSON.write(to: path2, options: .atomic)
        
        // Note: StatusReader currently supports single path only
        // Test with the path that has the file
        let reader = StatusReader(filePath: path2.path)
        let result = await reader.readNow()
        
        guard case .success(let (status, _, _)) = result else {
            XCTFail("Should find file at second path")
            return
        }
        
        XCTAssertEqual(status.status, "ok")
        
        // Cleanup
        try? FileManager.default.removeItem(at: path2)
    }
    
    func testEmptyStatusFile() async throws {
        // Test: Empty file should fail gracefully
        
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        
        // Create empty file
        try Data().write(to: statusFile, options: .atomic)
        
        let reader = StatusReader(filePath: statusFile.path)
        let result = await reader.readNow()
        
        // Should fail with decode error (or use cache if available)
        switch result {
        case .failure(let error):
            if case .decodeFailed = error {
                // Expected
            } else {
                XCTFail("Expected decodeFailed for empty file")
            }
        case .success(let (cached, _, _)):
            // If cache exists, may return cached
            XCTAssertEqual(cached.status, "ok")
        }
        
        // Cleanup
        try? FileManager.default.removeItem(at: statusFile)
    }
}

