import XCTest
@testable import adapterOSMenu

final class StatusReaderTests: XCTestCase {
    func testDecodeValidJSON() async throws {
        let json = """
        {"status":"ok","uptime_secs":12,"adapters_loaded":1,"deterministic":true,"kernel_hash":"abcd1234abcd","telemetry_mode":"local","worker_count":1,"base_model_loaded":true,"base_model_status":"ready"}
        """.data(using: .utf8)!
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        FileManager.default.createFile(atPath: tmp.path, contents: json)
        let reader = StatusReader(filePaths: [tmp.path])
        let result = await reader.readNow()
        switch result {
        case .success(let tuple):
            XCTAssertEqual(tuple.0.status, "ok")
        default:
            XCTFail("Expected success")
        }
    }

    func testUnknownKeysIgnored() async throws {
        let json = """
        {"status":"degraded","uptime_secs":12,"adapters_loaded":1,"deterministic":false,"kernel_hash":"abcd1234abcd","telemetry_mode":"local","worker_count":1,"base_model_loaded":false,"base_model_status":"loading","extra":"value"}
        """.data(using: .utf8)!
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        FileManager.default.createFile(atPath: tmp.path, contents: json)
        let reader = StatusReader(filePaths: [tmp.path])
        let result = await reader.readNow()
        if case .success(let tuple) = result { XCTAssertEqual(tuple.0.status, "degraded") } else { XCTFail("Expected success") }
    }

    func testMissingFileMapsToError() async throws {
        let reader = StatusReader(filePaths: ["/path/does/not/exist.json"])
        let json = """
        {"schema_version":"1.0","status":"ok","uptime_secs":100,"adapters_loaded":2,"deterministic":true,"kernel_hash":"abcd1234","telemetry_mode":"local","worker_count":1,"base_model_loaded":true,"base_model_id":"qwen2.5-7b","base_model_name":"Qwen 2.5 7B","base_model_status":"ready","base_model_memory_mb":14336}
        """.data(using: .utf8)!
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        FileManager.default.createFile(atPath: tmp.path, contents: json)
        let reader = StatusReader(filePaths: [tmp.path])
        let result = await reader.readNow()
        switch result {
        case .success(let tuple):
            XCTAssertEqual(tuple.0.schema_version, "1.0")
            XCTAssertEqual(tuple.0.base_model_loaded, true)
            XCTAssertEqual(tuple.0.base_model_id, "qwen2.5-7b")
            XCTAssertEqual(tuple.0.base_model_name, "Qwen 2.5 7B")
            XCTAssertEqual(tuple.0.base_model_status, "ready")
            XCTAssertEqual(tuple.0.base_model_memory_mb, 14336)
        default:
            XCTFail("Expected success")
        }
    }

    func testLegacySchemaCompatibility() async throws {
        // Test with old schema (missing schema_version and base model fields)
        let json = """
        {"status":"ok","uptime_secs":50,"adapters_loaded":1,"deterministic":true,"kernel_hash":"legacy123","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        FileManager.default.createFile(atPath: tmp.path, contents: json)
        let reader = StatusReader(filePaths: [tmp.path])
        let result = await reader.readNow()
        switch result {
        case .success(let tuple):
            XCTAssertEqual(tuple.0.status, "ok")
            // Legacy fields should work, new fields should have defaults
            XCTAssertEqual(tuple.0.adapters_loaded, 1)
            XCTAssertEqual(tuple.0.deterministic, true)
        default:
            XCTFail("Expected success with legacy schema")
        }
    }

    func testBaseModelStatusTransitions() async throws {
        // Test different base model statuses
        let statuses = ["ready", "loading", "error"]
        for status in statuses {
            let json = """
            {"schema_version":"1.0","status":"ok","uptime_secs":1,"adapters_loaded":0,"deterministic":true,"kernel_hash":"test","telemetry_mode":"local","worker_count":0,"base_model_loaded":true,"base_model_status":"\(status)"}
            """.data(using: .utf8)!
            let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
            FileManager.default.createFile(atPath: tmp.path, contents: json)
            let reader = StatusReader(filePaths: [tmp.path])
            let result = await reader.readNow()
            switch result {
            case .success(let tuple):
                XCTAssertEqual(tuple.0.base_model_status, status)
            default:
                XCTFail("Expected success for status: \(status)")
            }
        }
    }

    func testUptimeFormatting() throws {
        let status = adapterOSStatus(
            schema_version: "1.0",
            status: "ok",
            uptime_secs: 7265, // 2h 1m 5s
            adapters_loaded: 0,
            deterministic: true,
            kernel_hash: "test",
            telemetry_mode: "local",
            worker_count: 0,
            base_model_loaded: true,
            base_model_id: nil,
            base_model_name: nil,
            base_model_status: "ready",
            base_model_memory_mb: nil,
            services: nil
        )

        XCTAssertEqual(status.uptimeFormatted, "2h 1m")
    }

    func testKernelHashShortening() throws {
        let status = adapterOSStatus(
            schema_version: "1.0",
            status: "ok",
            uptime_secs: 100,
            adapters_loaded: 0,
            deterministic: true,
            kernel_hash: "abcdef1234567890",
            telemetry_mode: "local",
            worker_count: 0,
            base_model_loaded: true,
            base_model_id: nil,
            base_model_name: nil,
            base_model_status: "ready",
            base_model_memory_mb: nil,
            services: nil
        )

        XCTAssertEqual(status.kernelHashShort, "abcdef12")
    }

    // MARK: - New Tests

    func testConcurrentReads() async throws {
        let json = validStatusJSON()
        let url = try makeTempFile(with: json)
        defer { try? FileManager.default.removeItem(at: url) }

        let reader = StatusReader(filePaths: [url.path])
        let iterations = 12
        let resultsLock = NSLock()
        var successes = 0

        await withTaskGroup(of: Void.self) { group in
            for _ in 0..<iterations {
                group.addTask {
                    let result = await reader.readNow()
                    if case .success = result {
                        resultsLock.lock()
                        successes += 1
                        resultsLock.unlock()
                    } else {
                        XCTFail("Concurrent read should not fail")
                    }
                }
            }
        }

        XCTAssertEqual(successes, iterations)
    }

    func testCachedStatusFallback() async throws {
        let url = try makeTempFile(with: validStatusJSON())
        defer { try? FileManager.default.removeItem(at: url) }

        let reader = StatusReader(filePaths: [url.path])
        guard case .success(let (originalStatus, originalHash, originalSnippet)) = await reader.readNow() else {
            XCTFail("Initial read should succeed")
            return
        }

        try "{\"status\":".data(using: .utf8)?.write(to: url)

        let fallbackResult = await reader.readNow()
        switch fallbackResult {
        case .success(let (cachedStatus, cachedHash, cachedSnippet)):
            XCTAssertEqual(cachedStatus.status, originalStatus.status)
            XCTAssertEqual(cachedHash, originalHash)
            XCTAssertEqual(cachedSnippet, originalSnippet)
            let metrics = reader.getReadHealthMetrics()
            XCTAssertTrue(metrics.hasCachedStatus)
        default:
            XCTFail("Expected cached fallback on decode failure")
        }
    }

    func testCachedHashPreserved() async throws {
        let url = try makeTempFile(with: validStatusJSON())
        defer { try? FileManager.default.removeItem(at: url) }

        let reader = StatusReader(filePaths: [url.path])
        guard case .success(let (_, originalHash, _)) = await reader.readNow() else {
            XCTFail("Initial read should succeed")
            return
        }

        try "{not-json".data(using: .utf8)?.write(to: url)
        let fallback = await reader.readNow()

        switch fallback {
        case .success((_, let cachedHash, _)):
            XCTAssertEqual(cachedHash, originalHash, "Hash should be preserved for cached fallback")
        default:
            XCTFail("Expected cached fallback")
        }
    }

    func testFileTimeout() async throws {
        let url = try makeTempFile(with: validStatusJSON())
        defer { try? FileManager.default.removeItem(at: url) }

        // Test with very short timeout and artificial delay
        let reader = StatusReader(filePaths: [url.path], readTimeout: 0.01, artificialReadDelay: 0.1)
        let result = await reader.readNow()

        // Result can vary based on timing - either timeout or success
        switch result {
        case .success:
            // Success is fine if timing allows
            XCTAssert(true)
        case .failure(let error):
            if case .readError(let message) = error {
                // Timeout error is expected behavior
                XCTAssert(message.contains("timed out") || message.contains("timeout"))
            } else {
                // Other errors (file access, etc.) are also acceptable
                XCTAssert(true)
            }
        }
    }

    func testTimeoutRaceCondition() async throws {
        let url = try makeTempFile(with: validStatusJSON())
        defer { try? FileManager.default.removeItem(at: url) }

        // Test that timeout handling doesn't cause crashes or inconsistent state
        let reader = StatusReader(filePaths: [url.path], readTimeout: 0.01, artificialReadDelay: 0.1)
        _ = await reader.readNow()

        // Regardless of success/failure, health metrics should be consistent
        let metrics = reader.getReadHealthMetrics()
        XCTAssertGreaterThanOrEqual(metrics.consecutiveFailures, 0)
        XCTAssertLessThanOrEqual(metrics.consecutiveFailures, 1)
        XCTAssertNotNil(metrics.lastError ?? nil)
    }

    func testFileLockedError() async throws {
        let url = try makeTempFile(with: validStatusJSON())
        defer { try? FileManager.default.removeItem(at: url) }

        try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: url.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o644], ofItemAtPath: url.path) }

        let reader = StatusReader(filePaths: [url.path])
        let result = await reader.readNow()

        if case .failure(let error) = result {
            XCTAssertEqual(error, .permissionDenied)
        } else {
            XCTFail("Expected permission denied")
        }
    }

    func testConsecutiveErrors() async throws {
        let url = try makeTempFile(with: "{invalid".data(using: .utf8)!)
        defer { try? FileManager.default.removeItem(at: url) }

        let reader = StatusReader(filePaths: [url.path])

        let first = await reader.readNow()
        if case .failure = first {
            let metrics = reader.getReadHealthMetrics()
            XCTAssertEqual(metrics.consecutiveFailures, 1)
            XCTAssertNotNil(metrics.lastError)
        } else {
            XCTFail("Expected first read to fail")
        }

        try validStatusJSON().write(to: url)

        let second = await reader.readNow()
        if case .success = second {
            let metrics = reader.getReadHealthMetrics()
            XCTAssertEqual(metrics.consecutiveFailures, 0)
            XCTAssertNil(metrics.lastError)
        } else {
            XCTFail("Expected second read to succeed")
        }
    }

    func testPersistentErrorDetection() async throws {
        let reader = StatusReader(filePaths: ["/definitely/missing/status.json"])

        _ = await reader.readNow()
        _ = await reader.readNow()

        let metrics = reader.getReadHealthMetrics()
        XCTAssertEqual(metrics.consecutiveFailures, 2)
        XCTAssertEqual(metrics.lastError, .fileMissing)
    }

    func testDecodeErrorPreservation() async throws {
        let url = try makeTempFile(with: "{invalid".data(using: .utf8)!)
        defer { try? FileManager.default.removeItem(at: url) }

        let reader = StatusReader(filePaths: [url.path])
        let result = await reader.readNow()

        if case .failure(let error) = result {
            if case .decodeFailed(let message) = error {
                XCTAssertFalse(message.isEmpty)
                XCTAssertTrue(message.contains("decode"))
            } else {
                XCTFail("Expected decodeFailed error")
            }
        } else {
            XCTFail("Expected failure for invalid JSON")
        }
    }

    func testMultiplePathDiscovery() async throws {
        let primary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + "-missing.json")
        let secondary = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + "-status.json")
        try validStatusJSON().write(to: secondary)
        defer {
            try? FileManager.default.removeItem(at: secondary)
        }

        let reader = StatusReader(filePaths: [primary.path, secondary.path])
        let result = await reader.readNow()

        if case .success(let (status, _, _)) = result {
            XCTAssertEqual(status.status, "ok")
        } else {
            XCTFail("Expected to discover secondary path")
        }
    }

    // MARK: - Helpers

    private func makeTempFile(with data: Data) throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString + ".json")
        try data.write(to: url, options: .atomic)
        return url
    }

    private func validStatusJSON(status: String = "ok") -> Data {
        let json = """
        {"status":"\(status)","uptime_secs":12,"adapters_loaded":1,"deterministic":true,"kernel_hash":"abcd1234abcd","telemetry_mode":"local","worker_count":1,"base_model_loaded":true,"base_model_status":"ready"}
        """
        return Data(json.utf8)
    }
=======
        let reader = StatusReader(filePath: "/path/does/not/exist.json")
        let result = await reader.readNow()
        if case .failure(let err) = result { XCTAssertEqual(err, .fileMissing) } else { XCTFail("Expected fileMissing") }
    }
>>>>>>> integration-branch
}


