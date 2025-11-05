import Foundation
import os.log

/// Structured logging utility for the AdapterOS menu bar
final class Logger {
    static let shared = Logger()

    private let subsystem = "com.adapteros.menu"
    private let logger: OSLog

    enum LogLevel: String {
        case debug = "DEBUG"
        case info = "INFO"
        case warning = "WARNING"
        case error = "ERROR"
    }

    private init() {
        logger = OSLog(subsystem: subsystem, category: "MenuBar")
    }

    // MARK: - Logging Methods

    func debug(_ message: String, file: String = #file, function: String = #function, line: Int = #line, context: [String: Any]? = nil) {
        log(level: .debug, message: message, file: file, function: function, line: line, context: context)
    }

    func info(_ message: String, file: String = #file, function: String = #function, line: Int = #line, context: [String: Any]? = nil) {
        log(level: .info, message: message, file: file, function: function, line: line, context: context)
    }

    func warning(_ message: String, file: String = #file, function: String = #function, line: Int = #line, context: [String: Any]? = nil) {
        log(level: .warning, message: message, file: file, function: function, line: line, context: context)
    }

    func error(_ message: String, error: Error? = nil, file: String = #file, function: String = #function, line: Int = #line, context: [String: Any]? = nil) {
        var fullContext = context ?? [:]
        if let error = error {
            fullContext["error"] = error.localizedDescription
            fullContext["error_type"] = String(describing: type(of: error))
        }
        log(level: .error, message: message, file: file, function: function, line: line, context: fullContext)
    }

    // MARK: - Performance Logging

    func performance(_ operation: String, duration: TimeInterval, file: String = #file, function: String = #function, line: Int = #line, context: [String: Any]? = nil) {
        var fullContext = context ?? [:]
        fullContext["operation"] = operation
        fullContext["duration_ms"] = String(format: "%.2f", duration * 1000)
        info("Performance: \(operation) completed in \(String(format: "%.2f", duration * 1000))ms", file: file, function: function, line: line, context: fullContext)
    }

    // MARK: - Service Operation Logging

    func serviceOperation(_ operation: String, serviceId: String, success: Bool, duration: TimeInterval? = nil, error: Error? = nil, file: String = #file, function: String = #function, line: Int = #line) {
        var context: [String: Any] = ["service_id": serviceId, "operation": operation]

        if let duration = duration {
            context["duration_ms"] = String(format: "%.2f", duration * 1000)
        }

        let message = "Service operation: \(operation) \(success ? "succeeded" : "failed") for service '\(serviceId)'"

        if success {
            info(message, file: file, function: function, line: line, context: context)
        } else {
            self.error(message, error: error, file: file, function: function, line: line, context: context)
        }
    }

    // MARK: - Circuit Breaker Logging

    func circuitBreakerState(_ state: CircuitBreakerState, failureCount: Int, file: String = #file, function: String = #function, line: Int = #line) {
        let context: [String: Any] = [
            "state": String(describing: state),
            "failure_count": failureCount
        ]

        let level: LogLevel
        switch state {
        case .closed:
            level = .debug
        case .halfOpen:
            level = .info
        case .open:
            level = .warning
        }

        log(level: level, message: "Circuit breaker state changed to \(state)", file: file, function: function, line: line, context: context)
    }

    // MARK: - Private Methods

    private func log(level: LogLevel, message: String, file: String, function: String, line: Int, context: [String: Any]?) {
        let timestamp = ISO8601DateFormatter().string(from: Date())
        let filename = URL(fileURLWithPath: file).lastPathComponent
        let location = "\(filename):\(line) \(function)"

        var logMessage = "[\(timestamp)] [\(level.rawValue)] [\(location)] \(message)"

        if let context = context, !context.isEmpty {
            let contextString = context.map { "\($0.key)=\($0.value)" }.joined(separator: " ")
            logMessage += " | \(contextString)"
        }

        // Use OSLog for system integration
        let osLogType: OSLogType
        switch level {
        case .debug:
            osLogType = .debug
        case .info:
            osLogType = .info
        case .warning:
            osLogType = .default
        case .error:
            osLogType = .error
        }

        os_log("%{public}@", log: logger, type: osLogType, logMessage)

        // In debug builds, also print to console for easier debugging
        #if DEBUG
        print(logMessage)
        #endif
    }
}

// MARK: - Convenience Extensions

extension Logger {
    func time<T>(_ operation: String, file: String = #file, function: String = #function, line: Int = #line, _ block: () async throws -> T) async throws -> T {
        let startTime = Date()
        do {
            let result = try await block()
            let duration = Date().timeIntervalSince(startTime)
            performance(operation, duration: duration, file: file, function: function, line: line)
            return result
        } catch {
            let duration = Date().timeIntervalSince(startTime)
            self.error("Operation '\(operation)' failed after \(String(format: "%.2f", duration * 1000))ms", error: error, file: file, function: function, line: line)
            throw error
        }
    }

    func time<T>(_ operation: String, file: String = #file, function: String = #function, line: Int = #line, _ block: () throws -> T) throws -> T {
        let startTime = Date()
        do {
            let result = try block()
            let duration = Date().timeIntervalSince(startTime)
            performance(operation, duration: duration, file: file, function: function, line: line)
            return result
        } catch {
            let duration = Date().timeIntervalSince(startTime)
            self.error("Operation '\(operation)' failed after \(String(format: "%.2f", duration * 1000))ms", error: error, file: file, function: function, line: line)
            throw error
        }
    }
}
