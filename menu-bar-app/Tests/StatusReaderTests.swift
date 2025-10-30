import XCTest
@testable import AdapterOSMenu

final class StatusReaderTests: XCTestCase {
    func testDecodeValidJSON() async throws {
        let json = """
        {"status":"ok","uptime_secs":12,"adapters_loaded":1,"deterministic":true,"kernel_hash":"abcd1234abcd","telemetry_mode":"local","worker_count":1,"base_model_loaded":true,"base_model_status":"ready"}
        """.data(using: .utf8)!
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        FileManager.default.createFile(atPath: tmp.path, contents: json)
        let reader = StatusReader(filePath: tmp.path)
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
        let reader = StatusReader(filePath: tmp.path)
        let result = await reader.readNow()
        if case .success(let tuple) = result { XCTAssertEqual(tuple.0.status, "degraded") } else { XCTFail("Expected success") }
    }

    func testMissingFileMapsToError() async throws {
        let reader = StatusReader(filePath: "/path/does/not/exist.json")
        let result = await reader.readNow()
        if case .failure(let err) = result { XCTAssertEqual(err, .fileMissing) } else { XCTFail("Expected fileMissing") }
    }

    func testSchemaVersionSupport() async throws {
        let json = """
        {"schema_version":"1.0","status":"ok","uptime_secs":100,"adapters_loaded":2,"deterministic":true,"kernel_hash":"abcd1234","telemetry_mode":"local","worker_count":1,"base_model_loaded":true,"base_model_id":"qwen2.5-7b","base_model_name":"Qwen 2.5 7B","base_model_status":"ready","base_model_memory_mb":14336}
        """.data(using: .utf8)!
        let tmp = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString)
        FileManager.default.createFile(atPath: tmp.path, contents: json)
        let reader = StatusReader(filePath: tmp.path)
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
        let reader = StatusReader(filePath: tmp.path)
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
            let reader = StatusReader(filePath: tmp.path)
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
        let status = AdapterOSStatus(
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
            base_model_memory_mb: nil
        )

        XCTAssertEqual(status.uptimeFormatted, "2h 1m")
    }

    func testKernelHashShortening() throws {
        let status = AdapterOSStatus(
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
            base_model_memory_mb: nil
        )

        XCTAssertEqual(status.kernelHashShort, "abcdef12")
    }
}


