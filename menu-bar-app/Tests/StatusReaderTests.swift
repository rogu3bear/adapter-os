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
}


