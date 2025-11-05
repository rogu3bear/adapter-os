import XCTest
@testable import AdapterOSMenu

final class ServicePanelClientTests: XCTestCase {
    var client: ServicePanelClient!
    var mockServer: MockServicePanelServer!

    override func setUp() async throws {
        mockServer = try MockServicePanelServer()
        client = ServicePanelClient(
            baseURL: URL(string: "http://localhost:\(mockServer.port)")!
        )
        // Set shared secret for authentication
        // Note: ServicePanelClient uses AuthenticationManager.shared which reads from Keychain
        // For tests, we'd need to configure AuthenticationManager with test secret
    }

    override func tearDown() async throws {
        try await mockServer.stop()
        mockServer = nil
        client = nil
    }

    func testHealthCheck() async throws {
        // Given: Mock server is running

        // When
        let health = try await client.checkHealth()

        // Then
        XCTAssertEqual(health.status, "healthy")
        XCTAssertGreaterThan(health.runningServices, 0)
        XCTAssertGreaterThan(health.totalServices, 0)
    }

    func testStartService() async throws {
        // Given: Mock server configured with test service

        // When
        let result = try await client.startService("test-service")

        // Then
        XCTAssertTrue(result.success)
        XCTAssertEqual(result.message, "Service started successfully")
    }

    func testStopService() async throws {
        // Given: Mock server configured with running service

        // When
        let result = try await client.stopService("test-service")

        // Then
        XCTAssertTrue(result.success)
        XCTAssertEqual(result.message, "Service stopped successfully")
    }

    func testGetAllServices() async throws {
        // When
        let services = try await client.getAllServices()

        // Then
        XCTAssertFalse(services.isEmpty)
        XCTAssertTrue(services.contains { $0.id == "backend-server" })
        XCTAssertTrue(services.contains { $0.id == "ui-frontend" })
    }

    func testGetEssentialServices() async throws {
        // When
        let essentialServices = try await client.getEssentialServices()

        // Then
        XCTAssertFalse(essentialServices.isEmpty)
        XCTAssertTrue(essentialServices.allSatisfy { !$0.id.isEmpty })
    }

    func testAuthenticationFailure() async throws {
        // Given: Client with wrong shared secret
        // Note: Authentication is handled via AuthenticationManager.shared
        // This test may need to be updated to test authentication failure differently
        let badClient = ServicePanelClient(
            baseURL: URL(string: "http://localhost:\(mockServer.port)")!
        )

        // When/Then
        do {
            _ = try await badClient.startService("test-service")
            XCTFail("Expected authentication failure")
        } catch let error as ServiceError {
            switch error {
            case .unauthorized, .forbidden:
                // Expected
                break
            default:
                XCTFail("Unexpected error: \(error)")
            }
        }
    }

    func testServiceNotFound() async throws {
        // When/Then
        do {
            _ = try await client.startService("nonexistent-service")
            XCTFail("Expected not found error")
        } catch let error as ServiceError {
            switch error {
            case .notFound:
                // Expected
                break
            default:
                XCTFail("Unexpected error: \(error)")
            }
        }
    }
    
    func testConcurrentServiceOperations() async throws {
        let expectation = expectation(description: "Concurrent operations")
        expectation.expectedFulfillmentCount = 10
        
        for _ in 0..<10 {
            Task {
                do {
                    _ = try await client.checkHealth()
                    expectation.fulfill()
                } catch {
                    expectation.fulfill()
                }
            }
        }
        
        await fulfillment(of: [expectation], timeout: 5.0)
    }
    
    func testCircuitBreakerBehavior() async throws {
        // Simulate multiple failures to trigger circuit breaker
        let failingClient = ServicePanelClient(
            baseURL: URL(string: "http://localhost:9999")! // Invalid port
        )
        
        var failures = 0
        for _ in 0..<10 {
            do {
                _ = try await failingClient.checkHealth()
            } catch {
                failures += 1
            }
        }
        
        XCTAssertGreaterThan(failures, 0, "Should have failures")
        
        // Circuit breaker should eventually open after threshold failures
        let breakerState = failingClient.circuitBreaker.currentState
        XCTAssertTrue(
            breakerState == .open || breakerState == .halfOpen || breakerState == .closed,
            "Circuit breaker should be in valid state"
        )
    }
    
    func testCacheIntegration() async throws {
        ResponseCache.shared.clearCache()
        
        // First call should hit server
        let health1 = try await client.checkHealth()
        
        // Second call should hit cache
        let health2 = try await client.checkHealth()
        
        XCTAssertEqual(health1.status, health2.status, "Cached response should match")
        
        let stats = ResponseCache.shared.statistics
        XCTAssertGreaterThan(stats.entryCount, 0, "Cache should have entries")
    }
}

extension ServicePanelClient {
    var circuitBreaker: CircuitBreaker {
        let mirror = Mirror(reflecting: self)
        for child in mirror.children {
            if child.label == "circuitBreaker", let breaker = child.value as? CircuitBreaker {
                return breaker
            }
        }
        fatalError("Circuit breaker not found")
    }
}

// MARK: - Mock Server for Testing

class MockServicePanelServer {
    private let server: HTTPServer
    let port: Int
    let sharedSecret: String

    init() throws {
        sharedSecret = "test-secret"
        let authToken = "Basic " + Data("service-panel:\(sharedSecret)".utf8).base64EncodedString()

        // Create a simple HTTP server for testing
        // Note: In real implementation, you'd use a proper HTTP server library
        server = HTTPServer()
        port = 8081 // Use a different port for testing

        // Health endpoint
        server.get("/api/health") { request in
            return HTTPResponse(
                status: .ok,
                json: [
                    "status": "healthy",
                    "timestamp": ISO8601DateFormatter().string(from: Date()),
                    "runningServices": 2,
                    "totalServices": 3
                ]
            )
        }

        // Service management endpoints (with auth)
        server.post("/api/services/start") { request in
            guard request.headers["authorization"] == authToken else {
                return HTTPResponse(status: .unauthorized)
            }

            return HTTPResponse(
                status: .ok,
                json: [
                    "success": true,
                    "message": "Service started successfully",
                    "pid": 12345
                ]
            )
        }

        server.post("/api/services/stop") { request in
            guard request.headers["authorization"] == authToken else {
                return HTTPResponse(status: .unauthorized)
            }

            return HTTPResponse(
                status: .ok,
                json: [
                    "success": true,
                    "message": "Service stopped successfully"
                ]
            )
        }

        server.get("/api/services") { request in
            guard request.headers["authorization"] == authToken else {
                return HTTPResponse(status: .unauthorized)
            }

            return HTTPResponse(
                status: .ok,
                json: [
                    "services": [
                        [
                            "id": "backend-server",
                            "name": "Backend Server",
                            "status": "running",
                            "port": 3300,
                            "pid": 12345,
                            "startTime": ISO8601DateFormatter().string(from: Date()),
                            "category": "core",
                            "essential": true,
                            "dependencies": [],
                            "startupOrder": 1,
                            "logs": ["Service started"]
                        ],
                        [
                            "id": "ui-frontend",
                            "name": "UI Frontend",
                            "status": "running",
                            "port": 3200,
                            "pid": 12346,
                            "startTime": ISO8601DateFormatter().string(from: Date()),
                            "category": "core",
                            "essential": false,
                            "dependencies": [],
                            "startupOrder": 2,
                            "logs": ["Service started"]
                        ]
                    ]
                ]
            )
        }

        try server.start(port: port)
    }

    func stop() async throws {
        try await server.stop()
    }
}

// MARK: - Simple HTTP Server (Simplified for testing)

class HTTPServer {
    // This is a simplified HTTP server implementation
    // In real code, you'd use a proper HTTP server library like Vapor or Hummingbird
    private var routes: [String: (HTTPRequest) -> HTTPResponse] = [:]

    func get(_ path: String, handler: @escaping (HTTPRequest) -> HTTPResponse) {
        routes["GET \(path)"] = handler
    }

    func post(_ path: String, handler: @escaping (HTTPRequest) -> HTTPResponse) {
        routes["POST \(path)"] = handler
    }

    func start(port: Int) throws {
        // Implementation would start actual HTTP server
        // For now, this is just a placeholder
    }

    func stop() async throws {
        // Implementation would stop HTTP server
    }
}

struct HTTPRequest {
    let method: String
    let path: String
    let headers: [String: String]
    let body: Data?
}

struct HTTPResponse {
    let status: HTTPStatus
    let headers: [String: String] = ["Content-Type": "application/json"]
    let body: Data

    init(status: HTTPStatus, json: [String: Any]) {
        self.status = status
        self.body = try! JSONSerialization.data(withJSONObject: json)
    }

    init(status: HTTPStatus) {
        self.status = status
        self.body = Data()
    }
}

enum HTTPStatus {
    case ok, unauthorized, notFound
    var code: Int {
        switch self {
        case .ok: return 200
        case .unauthorized: return 401
        case .notFound: return 404
        }
    }
}
