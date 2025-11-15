import Foundation
import Combine

/// Circuit breaker states
enum CircuitBreakerState {
    case closed      // Normal operation
    case open        // Failing, requests rejected
    case halfOpen    // Testing if service recovered
}

/// Circuit breaker for preventing cascade failures
class CircuitBreaker {
    private let failureThreshold: Int
    private let recoveryTimeout: TimeInterval
    private let expectedException: (Error) -> Bool

    private var state: CircuitBreakerState = .closed
    private var failureCount: Int = 0
    private var lastFailureTime: Date?

    init(failureThreshold: Int = 5,
         recoveryTimeout: TimeInterval = 60.0,
         expectedException: @escaping (Error) -> Bool = { _ in true }) {
        self.failureThreshold = failureThreshold
        self.recoveryTimeout = recoveryTimeout
        self.expectedException = expectedException
    }

    func execute<T>(_ operation: () async throws -> T) async throws -> T {
        switch state {
        case .closed:
            do {
                let result = try await operation()
                reset()
                return result
            } catch {
                recordFailure(error)
                throw error
            }

        case .open:
            if shouldAttemptReset() {
                state = .halfOpen
                do {
                    let result = try await operation()
                    reset()
                    return result
                } catch {
                    recordFailure(error)
                    throw error
                }
            } else {
                throw CircuitBreakerError.circuitOpen
            }

        case .halfOpen:
            do {
                let result = try await operation()
                reset()
                return result
            } catch {
                recordFailure(error)
                throw error
            }
        }
    }

    private func recordFailure(_ error: Error) {
        if expectedException(error) {
            failureCount += 1
            lastFailureTime = Date()

            if failureCount >= failureThreshold {
                state = .open
            }
        }
    }

    private func reset() {
        failureCount = 0
        lastFailureTime = nil
        state = .closed
    }

    private func shouldAttemptReset() -> Bool {
        guard let lastFailure = lastFailureTime else { return false }
        return Date().timeIntervalSince(lastFailure) >= recoveryTimeout
    }

    var currentState: CircuitBreakerState { state }
    var currentFailureCount: Int { failureCount }
}

enum CircuitBreakerError: Error, LocalizedError {
    case circuitOpen

    var errorDescription: String? {
        switch self {
        case .circuitOpen:
            return "Circuit breaker is open - service temporarily unavailable"
        }
    }
}

/// Client for communicating with the AdapterOS Service Panel API
final class ServicePanelClient {
    private let baseURL: URL
    private let session: URLSession
    private let decoder: JSONDecoder
    private let encoder: JSONEncoder
    private let circuitBreaker: CircuitBreaker
    private let authManager: AuthenticationManager

    // Configuration
    private let config: ServiceClientConfig

    /// Configuration for different operation types
    struct ServiceClientConfig {
        let defaultTimeout: TimeInterval
        let maxRetries: Int
        let retryDelay: TimeInterval
        let operationConfigs: [String: OperationConfig]

        struct OperationConfig {
            let timeout: TimeInterval
            let maxRetries: Int
            let retryDelay: TimeInterval
            let cacheable: Bool
            let cacheTTL: TimeInterval
        }

        static let `default` = ServiceClientConfig(
            defaultTimeout: 30.0,
            maxRetries: 3,
            retryDelay: 1.0,
            operationConfigs: [
                "health": OperationConfig(timeout: 5.0, maxRetries: 2, retryDelay: 0.5, cacheable: true, cacheTTL: 5.0),
                "services": OperationConfig(timeout: 10.0, maxRetries: 3, retryDelay: 1.0, cacheable: true, cacheTTL: 15.0),
                "start_service": OperationConfig(timeout: 60.0, maxRetries: 2, retryDelay: 2.0, cacheable: false, cacheTTL: 0.0),
                "stop_service": OperationConfig(timeout: 30.0, maxRetries: 2, retryDelay: 1.0, cacheable: false, cacheTTL: 0.0),
                "unload_model": OperationConfig(timeout: 120.0, maxRetries: 2, retryDelay: 5.0, cacheable: false, cacheTTL: 0.0)
            ]
        )

        func config(for operation: String) -> OperationConfig {
            return operationConfigs[operation] ?? OperationConfig(
                timeout: defaultTimeout,
                maxRetries: maxRetries,
                retryDelay: retryDelay,
                cacheable: false,
                cacheTTL: 0.0
            )
        }
    }

    // Authentication cache
    private var cachedAuthToken: String?
    private var tokenExpiration: Date?

    init(baseURL: URL = URL(string: "http://localhost:3301")!, config: ServiceClientConfig = .default) {
        self.baseURL = baseURL
        self.config = config
        self.authManager = AuthenticationManager.shared

        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = config.defaultTimeout
        configuration.timeoutIntervalForResource = config.defaultTimeout * 2

        self.session = URLSession(configuration: configuration)
        self.decoder = JSONDecoder()
        self.encoder = JSONEncoder()
        self.circuitBreaker = CircuitBreaker(
            failureThreshold: 5,
            recoveryTimeout: 60.0,
            expectedException: { error in
                // Only count network and server errors as circuit breaker failures
                if let serviceError = error as? ServiceError {
                    return serviceError.shouldRetry
                }
                return error is URLError
            }
        )

        // Configure date decoding
        decoder.dateDecodingStrategy = .iso8601
        encoder.dateEncodingStrategy = .iso8601
    }

    // MARK: - Authentication

    private func getAuthToken() throws -> String {
        // Check if we have a valid cached token
        if let token = cachedAuthToken,
           let expiration = tokenExpiration,
           expiration > Date() {
            return token
        }

        // Get new token from auth manager
        let token = try authManager.getOrCreateToken()

        // Cache it
        cachedAuthToken = token
        tokenExpiration = Date().addingTimeInterval(3500) // 1 hour minus 100 seconds buffer

        return token
    }

    // MARK: - Service Operations

    func startService(_ serviceId: String) async throws -> ServiceOperationResult {
        try validateServiceId(serviceId)
        let endpoint = baseURL.appendingPathComponent("api/services/start")
        let request = ServiceRequest(serviceId: serviceId)

        return try await performRequest(endpoint: endpoint, method: "POST", body: request, operation: "start_service")
    }

    func stopService(_ serviceId: String) async throws -> ServiceOperationResult {
        try validateServiceId(serviceId)
        let endpoint = baseURL.appendingPathComponent("api/services/stop")
        let request = ServiceRequest(serviceId: serviceId)

        return try await performRequest(endpoint: endpoint, method: "POST", body: request, operation: "stop_service")
    }

    func getServiceStatus(_ serviceId: String) async throws -> ServiceInfo {
        try validateServiceId(serviceId)
        let endpoint = baseURL.appendingPathComponent("api/services/status")
        let request = ServiceRequest(serviceId: serviceId)

        let result: ServiceStatusResult = try await performRequest(
            endpoint: endpoint,
            method: "POST",
            body: request,
            operation: "services"
        )

        guard let service = result.service else {
            throw ServiceError.invalidResponse("No service information returned")
        }

        return service
    }

    func getAllServices() async throws -> [ServiceInfo] {
        let endpoint = baseURL.appendingPathComponent("api/services")
        let result: ServicesListResult = try await performRequest(endpoint: endpoint, method: "GET", operation: "services")

        return result.services
    }

    func startAllEssentialServices() async throws -> EssentialServicesResult {
        let endpoint = baseURL.appendingPathComponent("api/services/essential/start")

        return try await performRequest(endpoint: endpoint, method: "POST", operation: "start_service")
    }

    func stopAllEssentialServices() async throws -> EssentialServicesResult {
        let endpoint = baseURL.appendingPathComponent("api/services/essential/stop")

        return try await performRequest(endpoint: endpoint, method: "POST", operation: "stop_service")
    }

    func getEssentialServices() async throws -> [EssentialServiceInfo] {
        let endpoint = baseURL.appendingPathComponent("api/services/essential")
        let result: EssentialServicesListResult = try await performRequest(endpoint: endpoint, method: "GET", operation: "services")

        return result.essentialServices
    }

    // MARK: - Model Management

    func unloadModel(_ modelId: String) async throws -> ModelOperationResult {
        let endpoint = baseURL.appendingPathComponent("v1/models/\(modelId)/unload")

        return try await performRequest(endpoint: endpoint, method: "POST", operation: "unload_model")
    }

    // MARK: - Health Check

    func checkHealth() async throws -> HealthStatus {
        let endpoint = baseURL.appendingPathComponent("api/health")

        return try await performRequest(endpoint: endpoint, method: "GET", operation: "health")
    }

    // MARK: - Private Methods

    private func performRequest<T: Codable>(
        endpoint: URL,
        method: String,
        body: Encodable? = nil,
        operation: String = "default"
    ) async throws -> T {
        let operationConfig = config.config(for: operation)
        let bodyData = body.flatMap { try? encoder.encode($0) }

        // Check cache for GET requests (idempotent operations)
        if method == "GET" && operationConfig.cacheable,
           let cachedData = checkCache(for: endpoint, method: method, body: bodyData) {
            Logger.shared.debug("Cache hit for \(endpoint.path)", context: ["method": method, "operation": operation])
            return try decoder.decode(T.self, from: cachedData)
        }

        return try await circuitBreaker.execute {
            var attempt = 0
            var lastError: Error?

            while attempt < operationConfig.maxRetries {
                do {
                    let result: T = try await performSingleRequest(endpoint: endpoint, method: method, body: body, timeout: operationConfig.timeout)

                    // Cache successful GET responses
                    if method == "GET" && operationConfig.cacheable,
                       let resultData = try? encoder.encode(result) {
                        cacheResponse(resultData, for: endpoint, method: method, body: bodyData, ttl: operationConfig.cacheTTL)
                    }

                    return result
                } catch {
                    lastError = error
                    attempt += 1

                    // Check if this error type should be retried
                    if let serviceError = error as? ServiceError, !serviceError.shouldRetry {
                        throw error
                    }

                    // Don't retry on the last attempt
                    if attempt < operationConfig.maxRetries {
                        // Use error-specific retry delay if available, otherwise exponential backoff
                        let baseDelay = (error as? ServiceError)?.retryDelay ?? operationConfig.retryDelay
                        let delay = baseDelay * pow(2.0, Double(attempt - 1))
                        try await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
                    }
                }
            }

            throw lastError ?? ServiceError.unknown
        }
    }

    // MARK: - Caching

    private func checkCache(for endpoint: URL, method: String, body: Data?) -> Data? {
        let cacheKey = ResponseCache.cacheKey(for: endpoint, method: method, body: body)
        return ResponseCache.shared.retrieve(forKey: cacheKey)
    }

    private func cacheResponse(_ data: Data, for endpoint: URL, method: String, body: Data?, ttl: TimeInterval) {
        let cacheKey = ResponseCache.cacheKey(for: endpoint, method: method, body: body)
        ResponseCache.shared.store(data, forKey: cacheKey, ttl: ttl)
    }

    private func performSingleRequest<T: Decodable>(
        endpoint: URL,
        method: String,
        body: Encodable? = nil,
        timeout: TimeInterval? = nil
    ) async throws -> T {
        var request = URLRequest(url: endpoint)
        request.httpMethod = method
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("AdapterOSMenu/1.0", forHTTPHeaderField: "User-Agent")

        // Use operation-specific timeout if provided
        if let timeout = timeout {
            request.timeoutInterval = timeout
        }

        // Add authentication for service management and model endpoints
        if endpoint.path.contains("/api/services") || endpoint.path.contains("/v1/models") {
            let token = try getAuthToken()
            request.setValue(token, forHTTPHeaderField: "Authorization")
        }

        if let body = body {
            request.httpBody = try encoder.encode(body)
        }

        let (data, response) = try await session.data(for: request)

        guard let httpResponse = response as? HTTPURLResponse else {
            throw ServiceError.invalidResponse("Not an HTTP response")
        }

        switch httpResponse.statusCode {
        case 200...299:
            // Success
            break
        case 300...399:
            // Redirects - shouldn't happen for our API, but handle gracefully
            if let location = httpResponse.value(forHTTPHeaderField: "Location") {
                throw ServiceError.redirect("Unexpected redirect to: \(location)")
            } else {
                throw ServiceError.redirect("Unexpected redirect (no location header)")
            }
        case 400:
            throw ServiceError.badRequest("Invalid request parameters")
        case 401:
            throw ServiceError.unauthorized("Authentication required")
        case 403:
            throw ServiceError.forbidden("Access denied")
        case 404:
            throw ServiceError.notFound("Service or endpoint not found")
        case 405:
            throw ServiceError.methodNotAllowed("HTTP method not allowed")
        case 408:
            throw ServiceError.timeout("Request timeout")
        case 409:
            throw ServiceError.conflict("Operation conflicts with current state")
        case 410:
            throw ServiceError.gone("Resource no longer available")
        case 422:
            throw ServiceError.unprocessableEntity("Request validation failed")
        case 429:
            throw ServiceError.tooManyRequests("Rate limit exceeded")
        case 500:
            throw ServiceError.internalServerError("Internal server error")
        case 501:
            throw ServiceError.notImplemented("Feature not implemented")
        case 502:
            throw ServiceError.badGateway("Bad gateway")
        case 503:
            throw ServiceError.serviceUnavailable("Service temporarily unavailable")
        case 504:
            throw ServiceError.gatewayTimeout("Gateway timeout")
        case 505...599:
            throw ServiceError.serverError("Server error: \(httpResponse.statusCode)")
        default:
            throw ServiceError.unknownStatusCode(httpResponse.statusCode)
        }

        do {
            return try decoder.decode(T.self, from: data)
        } catch {
            throw ServiceError.decodingFailed("Failed to decode response: \(error.localizedDescription)")
        }
    }
}

// MARK: - Data Models

struct ServiceRequest: Codable {
    let serviceId: String
}

struct ServiceOperationResult: Codable {
    let success: Bool
    let message: String
    let pid: Int?

    enum CodingKeys: String, CodingKey {
        case success, message, pid
    }
}

struct ServiceStatusResult: Codable {
    let service: ServiceInfo?
    let running: Bool?
    let pid: Int?
    let logs: [String]?
}

struct ServicesListResult: Codable {
    let services: [ServiceInfo]
}

struct EssentialServicesResult: Codable {
    let success: Bool
    let message: String
    let results: [EssentialServiceOperationResult]
}

struct EssentialServiceOperationResult: Codable {
    let serviceId: String
    let status: String
    let pid: Int?
    let error: String?
}

struct EssentialServicesListResult: Codable {
    let essentialServices: [EssentialServiceInfo]
}

struct ServiceInfo: Codable {
    let id: String
    let name: String
    let status: String
    let port: Int
    let pid: Int?
    let dependencies: [String]
    let startupOrder: Int
}

struct EssentialServiceInfo: Codable {
    let id: String
    let name: String
    let status: String
    let port: Int
    let pid: Int?
    let dependencies: [String]
    let startupOrder: Int
}

struct ModelOperationResult: Codable {
    // The unload model endpoint returns just a status code, so we'll use an empty struct
    // If the API returns more data in the future, we can extend this
}

struct HealthStatus: Codable {
    let status: String
    let timestamp: Date
    let runningServices: Int
    let totalServices: Int
}

enum ServiceError: Error, LocalizedError {
    case badRequest(String)
    case unauthorized(String)
    case forbidden(String)
    case notFound(String)
    case methodNotAllowed(String)
    case timeout(String)
    case conflict(String)
    case gone(String)
    case unprocessableEntity(String)
    case tooManyRequests(String)
    case internalServerError(String)
    case notImplemented(String)
    case badGateway(String)
    case serviceUnavailable(String)
    case gatewayTimeout(String)
    case redirect(String)
    case serverError(String)
    case unknownStatusCode(Int)
    case invalidResponse(String)
    case decodingFailed(String)
    case networkError(URLError)
    case unknown

    var errorDescription: String? {
        switch self {
        case .badRequest(let message):
            return "Bad Request: \(message)"
        case .unauthorized(let message):
            return "Unauthorized: \(message)"
        case .forbidden(let message):
            return "Forbidden: \(message)"
        case .notFound(let message):
            return "Not Found: \(message)"
        case .methodNotAllowed(let message):
            return "Method Not Allowed: \(message)"
        case .timeout(let message):
            return "Timeout: \(message)"
        case .conflict(let message):
            return "Conflict: \(message)"
        case .gone(let message):
            return "Gone: \(message)"
        case .unprocessableEntity(let message):
            return "Unprocessable Entity: \(message)"
        case .tooManyRequests(let message):
            return "Too Many Requests: \(message)"
        case .internalServerError(let message):
            return "Internal Server Error: \(message)"
        case .notImplemented(let message):
            return "Not Implemented: \(message)"
        case .badGateway(let message):
            return "Bad Gateway: \(message)"
        case .serviceUnavailable(let message):
            return "Service Unavailable: \(message)"
        case .gatewayTimeout(let message):
            return "Gateway Timeout: \(message)"
        case .redirect(let message):
            return "Redirect: \(message)"
        case .serverError(let message):
            return "Server Error: \(message)"
        case .unknownStatusCode(let code):
            return "Unknown Status Code: \(code)"
        case .invalidResponse(let message):
            return "Invalid Response: \(message)"
        case .decodingFailed(let message):
            return "Decoding Failed: \(message)"
        case .networkError(let error):
            return "Network Error: \(error.localizedDescription)"
        case .unknown:
            return "Unknown Error"
        }
    }

    /// Determines if this error should trigger a retry
    var shouldRetry: Bool {
        switch self {
        case .networkError, .timeout, .serviceUnavailable, .gatewayTimeout, .tooManyRequests:
            return true
        case .serverError:
            // Some server errors might be retryable
            return true
        case .badRequest, .unauthorized, .forbidden, .notFound, .methodNotAllowed,
             .conflict, .gone, .unprocessableEntity, .internalServerError, .notImplemented,
             .badGateway, .redirect, .unknownStatusCode, .invalidResponse, .decodingFailed, .unknown:
            return false
        }
    }

    /// Suggested retry delay for this error type
    var retryDelay: TimeInterval {
        switch self {
        case .tooManyRequests:
            return 60.0 // Respect rate limiting
        case .serviceUnavailable, .gatewayTimeout:
            return 5.0 // Temporary service issues
        case .networkError, .timeout:
            return 2.0 // Network issues
        default:
            return 1.0 // Default delay
        }
    }
}

// MARK: - Input Validation

extension ServicePanelClient {
    private func validateServiceId(_ serviceId: String) throws {
        // Check for empty/whitespace-only strings
        let trimmed = serviceId.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            throw ValidationError.emptyServiceId
        }

        // Check length limits (reasonable bounds)
        guard trimmed.count <= 100 else {
            throw ValidationError.serviceIdTooLong
        }

        // Check for valid characters (alphanumeric, hyphens, underscores)
        let allowedCharacterSet = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_"))
        guard CharacterSet(charactersIn: trimmed).isSubset(of: allowedCharacterSet) else {
            throw ValidationError.invalidServiceIdCharacters
        }

        // Check for path traversal attempts
        guard !trimmed.contains("..") && !trimmed.contains("/") && !trimmed.contains("\\") else {
            throw ValidationError.pathTraversalAttempt
        }

        // Check for reserved service IDs
        let reservedIds = ["admin", "root", "system", "kernel", "init"]
        guard !reservedIds.contains(trimmed.lowercased()) else {
            throw ValidationError.reservedServiceId
        }
    }

    private func validateURL(_ url: URL) throws {
        // Ensure URL is valid and uses HTTP/HTTPS
        guard url.scheme == "http" || url.scheme == "https" else {
            throw ValidationError.invalidURLScheme
        }

        // Ensure host is localhost or valid IP
        guard let host = url.host,
              (host == "localhost" || host == "127.0.0.1" || host == "::1") else {
            throw ValidationError.invalidHost
        }

        // Ensure port is reasonable (if specified)
        if let port = url.port {
            guard port > 0 && port <= 65535 else {
                throw ValidationError.invalidPort
            }
        }
    }
}

enum ValidationError: Error, LocalizedError {
    case emptyServiceId
    case serviceIdTooLong
    case invalidServiceIdCharacters
    case pathTraversalAttempt
    case reservedServiceId
    case invalidURLScheme
    case invalidHost
    case invalidPort

    var errorDescription: String? {
        switch self {
        case .emptyServiceId:
            return "Service ID cannot be empty"
        case .serviceIdTooLong:
            return "Service ID is too long (maximum 100 characters)"
        case .invalidServiceIdCharacters:
            return "Service ID contains invalid characters (only alphanumeric, hyphens, and underscores allowed)"
        case .pathTraversalAttempt:
            return "Service ID contains path traversal characters"
        case .reservedServiceId:
            return "Service ID uses a reserved name"
        case .invalidURLScheme:
            return "URL must use HTTP or HTTPS scheme"
        case .invalidHost:
            return "URL must point to localhost"
        case .invalidPort:
            return "URL port must be between 1 and 65535"
        }
    }
}
