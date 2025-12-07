@preconcurrency import Foundation
import CryptoKit

enum StatusReadError: Error, Equatable {
    case fileMissing
    case permissionDenied
    case decodeFailed(String)
    case validationFailed(String)
    case readError(String)
    case unknown
}

/// Reads AdapterOS status from the JSON snapshot file without blocking the main thread.
final class StatusReader {
    private let filePaths: [String]
    private let decoder: JSONDecoder
    private let readTimeout: TimeInterval
    private var lastValidStatus: AdapterOSStatus?  // Cache for fallback on corruption
    private var lastValidMetadata: (hash: Data, snippet: String)?
    private var validationErrorCount: Int = 0
    private var consecutiveFailures: Int = 0
    private var lastReadError: StatusReadError?
    
    /// Default paths to check in order: primary system path, then fallback paths
    /// First checks metadata file written by server to discover actual path
    static var defaultPaths: [String] {
        let homeDir = FileManager.default.homeDirectoryForCurrentUser.path
        let currentDir = FileManager.default.currentDirectoryPath
        let fileManager = FileManager.default
        
        var paths: [String] = []
        
        // First, try to read metadata file written by server indicating actual path
        let metadataPaths = [
            "/var/run/adapteros_status_path.txt",
            "\(homeDir)/Library/Application Support/AdapterOS/status_path.txt"
        ]
        
        for metadataPath in metadataPaths {
            if let content = try? String(contentsOfFile: metadataPath, encoding: .utf8),
               !content.trimmingCharacters(in: .whitespaces).isEmpty {
                let serverPath = content.trimmingCharacters(in: .whitespaces)
                if fileManager.fileExists(atPath: serverPath) {
                    paths.append(serverPath)
                    break  // Found server's actual path, use it first
                }
            }
        }
        
        // Always check primary system path
        if !paths.contains("/var/run/adapteros_status.json") {
            paths.append("/var/run/adapteros_status.json")
        }
        
        // Check relative to current directory (works if menu bar app runs from same dir as server)
        let currentPath = "\(currentDir)/var/adapteros_status.json"
        if !paths.contains(currentPath) {
            paths.append(currentPath)
        }
        
        // Check common server installation locations
        let commonServerDirs = [
            currentDir,  // Current working directory
            homeDir + "/.adapteros",  // User config directory
            "/opt/adapteros",  // System installation
            "/usr/local/adapteros",  // Local installation
        ]
        
        for dir in commonServerDirs {
            let path = "\(dir)/var/adapteros_status.json"
            if !paths.contains(path) {
                paths.append(path)
            }
        }
        
        // User-writable fallback location
        let userFallback = "\(homeDir)/Library/Application Support/AdapterOS/status.json"
        if !paths.contains(userFallback) {
            paths.append(userFallback)
        }
        
        return paths
    }

    private let artificialReadDelay: TimeInterval

    init(filePaths: [String] = StatusReader.defaultPaths, readTimeout: TimeInterval = 2.0, artificialReadDelay: TimeInterval = 0) {
        self.filePaths = filePaths
        self.decoder = JSONDecoder()
        self.readTimeout = readTimeout
        self.artificialReadDelay = artificialReadDelay
    }
    
    /// Find the first existing status file path
    private func findStatusFile() -> String? {
        let fileManager = FileManager.default
        for path in filePaths {
            if fileManager.fileExists(atPath: path) {
                return path
            }
        }
        return nil
    }

    /// Initialize with fallback to local test data
    convenience init() {
        // Check for local test data first, then system location
        let localPath = Bundle.main.bundlePath + "/../../../var/adapteros_status.json"
        if FileManager.default.fileExists(atPath: localPath) {
            self.init(filePaths: [localPath])
        } else {
            self.init(filePaths: ["/var/run/adapteros_status.json"])
        }
    }

    /// Read and decode status. Throws mapped StatusReadError.
    func readStatus() async throws -> AdapterOSStatus {
        let (status, _) = try await readInternal()
        return status
    }

    /// Read and decode status, capturing raw hash for de-jittering and a short snippet for copy.
    /// Returns .success(AdapterOSStatus) or .failure(StatusReadError).
<<<<<<< HEAD
    /// On validation failure, attempts to return last valid status if available.
    func readNow() async -> Result<(AdapterOSStatus, Data, String), StatusReadError> {
        do {
            let (status, meta) = try await readInternal()
            // Cache valid status for fallback
            updateLastValidStatus(status, metadata: meta)
            consecutiveFailures = 0
            lastReadError = nil
            return .success((status, meta.hash, meta.snippet))
        } catch let error as StatusReadError {
            // If validation failed and we have a cached status, return that instead
            if case .validationFailed = error, let cached = lastValidStatus {
                // Return cached status but log the validation failure
                // Note: We can't return the original hash/snippet, so we use empty data
                let metadata = lastValidMetadata ?? (Data(), "cached")
                validationErrorCount += 1
                lastReadError = error
                consecutiveFailures += 1
                return .success((cached, metadata.hash, metadata.snippet))
            }
            if case .decodeFailed(_) = error, let cached = lastValidStatus {
                let metadata = lastValidMetadata ?? (Data(), "cached")
                lastReadError = error
                consecutiveFailures += 1
                return .success((cached, metadata.hash, metadata.snippet))
            }
            lastReadError = error
            consecutiveFailures += 1
            return .failure(error)
        } catch {
            // On unknown error, try to return cached status
            if let cached = lastValidStatus {
                let metadata = lastValidMetadata ?? (Data(), "cached")
                lastReadError = .unknown
                consecutiveFailures += 1
                return .success((cached, metadata.hash, metadata.snippet))
            }
=======
    func readNow() async -> Result<(AdapterOSStatus, Data, String), StatusReadError> {
        do {
            let (status, meta) = try await readInternal()
            return .success((status, meta.hash, meta.snippet))
        } catch let error as StatusReadError {
            return .failure(error)
        } catch {
>>>>>>> integration-branch
            return .failure(.unknown)
        }
    }

    // MARK: - Internal read
    private func readInternal() async throws -> (AdapterOSStatus, (hash: Data, snippet: String)) {
<<<<<<< HEAD
        // Capture only the properties we need to avoid Sendable issues
        let decoder = self.decoder

        let timeout = readTimeout
        return try await withCheckedThrowingContinuation { continuation in
            let resumeLock = NSLock()
            var didResume = false

            let queue = DispatchQueue.global(qos: .utility)
            var readItem: DispatchWorkItem?

            func resumeSuccess(_ value: (AdapterOSStatus, (hash: Data, snippet: String))) {
                resumeLock.lock()
                defer { resumeLock.unlock() }
                guard !didResume else { return }
                didResume = true
                continuation.resume(returning: value)
            }

            func resumeFailure(_ error: StatusReadError) {
                resumeLock.lock()
                defer { resumeLock.unlock() }
                guard !didResume else { return }
                didResume = true
                continuation.resume(throwing: error)
            }

            readItem = DispatchWorkItem { [weak self] in
                guard let self = self else { return }
                if readItem?.isCancelled == true {
                    return
                }

                do {
                    // Find the first existing status file
                    guard let filePath = self.findStatusFile() else {
                        resumeFailure(.fileMissing)
                        return
                    }

                    if self.artificialReadDelay > 0 {
                        Thread.sleep(forTimeInterval: self.artificialReadDelay)
                        if readItem?.isCancelled == true {
                            return
                        }
                    }

                    let handle = try FileHandle(forReadingFrom: URL(fileURLWithPath: filePath))
                    defer { try? handle.close() }
                    let data = try handle.readToEnd() ?? Data()
                    if data.isEmpty {
                        let error = StatusReadError.decodeFailed("Status file '\(filePath)' is empty")
                        Logger.shared.error("Status file is empty", error: error, context: ["path": filePath])
                        resumeFailure(error)
                        return
                    }
=======
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                do {
                    guard FileManager.default.fileExists(atPath: self.filePath) else {
                        throw StatusReadError.fileMissing
                    }

                    let handle = try FileHandle(forReadingFrom: URL(fileURLWithPath: self.filePath))
                    defer { try? handle.close() }
                    let data = try handle.readToEnd() ?? Data()
                    if data.isEmpty { throw StatusReadError.decodeFailed }
>>>>>>> integration-branch

                    // Compute hash for de-jittering
                    let digest = SHA256.hash(data: data)
                    let hashData = Data(digest) // 32 bytes, bounded

                    // Decode JSON
                    do {
<<<<<<< HEAD
                        let status = try decoder.decode(AdapterOSStatus.self, from: data)
                        let snippet = Self.makeSnippet(from: data)
                        
                        // Validate decoded status
                        if let validationError = Self.validateStatus(status) {
                            resumeFailure(.validationFailed(validationError))
                            return
                        }
                        
                        // Status is valid - return it (caching happens in readNow wrapper)
                        resumeSuccess((status, (hashData, snippet)))
                    } catch {
                        let message = "Failed to decode AdapterOS status JSON: \(error.localizedDescription)"
                        let decodeError = StatusReadError.decodeFailed(message)
                        Logger.shared.error("Status JSON decode failed", error: error, context: ["path": filePath])
                        resumeFailure(decodeError)
                    }
                } catch let error as StatusReadError {
                    resumeFailure(error)
                } catch let error as NSError {
                    if error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoSuchFileError {
                        resumeFailure(.fileMissing)
                    } else if error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoPermissionError {
                        resumeFailure(.permissionDenied)
                    } else {
                        resumeFailure(.unknown)
                    }
                }
            }

            if let readItem {
                queue.async(execute: readItem)
            }

            queue.asyncAfter(deadline: .now() + timeout) {
                resumeLock.lock()
                let shouldTimeout = !didResume
                resumeLock.unlock()
                if shouldTimeout {
                    readItem?.cancel()
                    let timeoutError = StatusReadError.readError("Read timed out after \(String(format: "%.2f", timeout)) seconds")
                    Logger.shared.warning("Status read timed out", context: ["timeout_seconds": timeout])
                    resumeFailure(timeoutError)
                }
            }
=======
                        let status = try self.decoder.decode(AdapterOSStatus.self, from: data)
                        let snippet = Self.makeSnippet(from: data)
                        continuation.resume(returning: (status, (hashData, snippet)))
                    } catch {
                        continuation.resume(throwing: StatusReadError.decodeFailed)
                    }
                } catch let error as StatusReadError {
                    continuation.resume(throwing: error)
                } catch let error as NSError {
                    if error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoSuchFileError {
                        continuation.resume(throwing: .fileMissing)
                    } else if error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoPermissionError {
                        continuation.resume(throwing: .permissionDenied)
                    } else {
                        continuation.resume(throwing: .unknown)
                    }
                }
            }
>>>>>>> integration-branch
        }
    }

    private static func makeSnippet(from data: Data) -> String {
        // Keep at most 1KB, ensure valid UTF-8
        let maxLen = 1024
        let slice = data.prefix(maxLen)
        if let s = String(data: slice, encoding: .utf8) {
            return s
        }
        // Fallback to hex if not UTF-8
        return slice.map { String(format: "%02x", $0) }.joined()
    }
<<<<<<< HEAD
    
    /// Validate that decoded status has all required fields with valid values
    private static func validateStatus(_ status: AdapterOSStatus) -> String? {
        // Validate required non-optional fields
        if status.status.isEmpty {
            return "status field is empty"
        }
        
        // Validate status value is one of expected values
        let validStatuses = ["ok", "degraded", "error"]
        if !validStatuses.contains(status.status) {
            return "status field has invalid value: '\(status.status)'"
        }
        
        // Validate kernel_hash is not empty
        if status.kernel_hash.isEmpty {
            return "kernel_hash field is empty"
        }
        
        // Validate telemetry_mode is not empty
        if status.telemetry_mode.isEmpty {
            return "telemetry_mode field is empty"
        }
        
        // Validate schema_version if present (for forward compatibility)
        if let schemaVersion = status.schema_version, !schemaVersion.isEmpty {
            // Currently only support "1.0", but allow future versions
            if schemaVersion != "1.0" && !schemaVersion.starts(with: "1.") {
                return "unsupported schema_version: '\(schemaVersion)'"
            }
        }
        
        // Validate base_model_status if present
        if let baseModelStatus = status.base_model_status, !baseModelStatus.isEmpty {
            let validBaseModelStatuses = ["ready", "loading", "error", "unloaded", "unknown"]
            if !validBaseModelStatuses.contains(baseModelStatus) {
                return "base_model_status has invalid value: '\(baseModelStatus)'"
            }
        }
        
        return nil  // Validation passed
    }
    
    /// Get last valid status (for fallback on corruption)
    func getLastValidStatus() -> AdapterOSStatus? {
        return lastValidStatus
    }

    /// Get status read health metrics (for monitoring)
    func getReadHealthMetrics() -> StatusReadHealthMetrics {
        return StatusReadHealthMetrics(
            hasCachedStatus: lastValidStatus != nil,
            validationErrors: validationErrorCount,
            consecutiveFailures: consecutiveFailures,
            lastError: lastReadError
        )
    }

    // MARK: - Test Helpers

    #if DEBUG
    /// Test helper: Inject corrupted JSON for testing fallback behavior
    func injectCorruptedStatusForTesting() {
        lastValidStatus = nil
        lastValidMetadata = nil
    }

    /// Test helper: Inject valid status for testing
    func injectValidStatusForTesting(_ status: AdapterOSStatus, metadata: (hash: Data, snippet: String)? = nil) {
        lastValidStatus = status
        if let metadata {
            lastValidMetadata = metadata
        } else {
            lastValidMetadata = (Data(), "injected")
        }
    }
    #endif

    /// Health metrics for status read operations
    struct StatusReadHealthMetrics {
        let hasCachedStatus: Bool
        let validationErrors: Int
        let consecutiveFailures: Int
        let lastError: StatusReadError?
    }
    
    /// Update last valid status cache
    private func updateLastValidStatus(_ status: AdapterOSStatus, metadata: (hash: Data, snippet: String)) {
        lastValidStatus = status
        lastValidMetadata = metadata
    }
=======
>>>>>>> integration-branch
}


