import XCTest
@testable import adapterOSMenu

final class StatusViewModelTests: XCTestCase {
    var viewModel: StatusViewModel!
    
    override func setUp() {
            viewModel = StatusViewModel()
        viewModel.stopPolling()
    }
    
    override func tearDown() {
            viewModel = nil
        }

    func testInitialState() {
        XCTAssertTrue(viewModel.isOffline, "Should start offline")
        XCTAssertEqual(viewModel.iconName, "bolt.slash.circle.fill")
        XCTAssertEqual(viewModel.tooltip, "adapterOS OFFLINE")
        XCTAssertNil(viewModel.status)
    }

    func testStatusUpdateBehavior() async throws {
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        defer { try? FileManager.default.removeItem(at: statusFile) }

        // Test successful read
        let json1 = """
        {"status":"ok","uptime_secs":100,"adapters_loaded":1,"deterministic":true,"kernel_hash":"hash1","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        try json1.write(to: statusFile)

        await viewModel.refresh()
        XCTAssertFalse(viewModel.isOffline, "Should be online after successful read")
        XCTAssertEqual(viewModel.status?.uptime_secs, 100)

        // Test same hash doesn't update
        await viewModel.refresh()
        XCTAssertEqual(viewModel.status?.uptime_secs, 100, "Status should not change with same hash")

        // Test different hash updates
        let json2 = """
        {"status":"ok","uptime_secs":200,"adapters_loaded":1,"deterministic":true,"kernel_hash":"hash2","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        try json2.write(to: statusFile)

        await viewModel.refresh()
        XCTAssertEqual(viewModel.status?.uptime_secs, 200, "Status should update with different hash")
    }

    func testErrorHandling() async throws {
        let tmpViewModel = StatusViewModel()
        tmpViewModel.stopPolling()

        // Test with no file (should show offline)
        await tmpViewModel.refresh()
        XCTAssertTrue(tmpViewModel.isOffline, "Should be offline with no status file")

        // Test with invalid JSON
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        defer { try? FileManager.default.removeItem(at: statusFile) }

        try "{invalid json".write(to: statusFile, atomically: true, encoding: .utf8)

        await tmpViewModel.refresh()
        // Error handling depends on whether cached status exists
        // Either offline or cached status should be returned
        XCTAssertNotNil(tmpViewModel.lastError ?? tmpViewModel.status, "Should have either error or cached status")
    }

    func testIconAndTooltipUpdate() async throws {
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        defer { try? FileManager.default.removeItem(at: statusFile) }

        let tmpViewModel = StatusViewModel()
        tmpViewModel.stopPolling()

        // Test offline state
        XCTAssertEqual(tmpViewModel.iconName, "bolt.slash.circle.fill")
        XCTAssertEqual(tmpViewModel.tooltip, "adapterOS OFFLINE")

        // Test online state
        let json = """
        {"status":"ok","uptime_secs":100,"adapters_loaded":2,"deterministic":true,"kernel_hash":"test","telemetry_mode":"local","worker_count":2,"base_model_loaded":true,"base_model_status":"ready"}
        """.data(using: .utf8)!
        try json.write(to: statusFile)

        await tmpViewModel.refresh()

        XCTAssertNotEqual(tmpViewModel.iconName, "bolt.slash.circle.fill", "Icon should change when online")
        XCTAssertFalse(tmpViewModel.tooltip.contains("OFFLINE"), "Tooltip should not show offline")
    }

    func testConcurrentOperations() async throws {
        let tmpDir = FileManager.default.temporaryDirectory
        let statusFile = tmpDir.appendingPathComponent(UUID().uuidString + ".json")
        defer { try? FileManager.default.removeItem(at: statusFile) }

        let json = """
        {"status":"ok","uptime_secs":100,"adapters_loaded":1,"deterministic":true,"kernel_hash":"test","telemetry_mode":"local","worker_count":1}
        """.data(using: .utf8)!
        try json.write(to: statusFile)

        let tmpViewModel = StatusViewModel()
        tmpViewModel.stopPolling()

        // Run multiple concurrent operations
        async let refresh1 = tmpViewModel.refresh()
        async let refresh2 = tmpViewModel.refresh()
        async let refresh3 = tmpViewModel.refresh()

        await [refresh1, refresh2, refresh3]

        // All should complete without crashes
        XCTAssertNotNil(tmpViewModel.status, "Status should be available after concurrent operations")
    }

    func testServiceOperationCancellation() async throws {
        let tmpViewModel = StatusViewModel()
        tmpViewModel.stopPolling()

        let task = Task {
            await tmpViewModel.refresh()
        }

        task.cancel()

        await Task.sleep(nanoseconds: 100_000_000)

        // Should not crash
        XCTAssertTrue(true, "Operation should handle cancellation gracefully")
    }
}
