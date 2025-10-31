import Foundation
import CryptoKit

enum StatusReadError: Error, Equatable {
    case fileMissing
    case permissionDenied
    case decodeFailed
    case unknown
}

/// Reads AdapterOS status from the JSON snapshot file without blocking the main thread.
final class StatusReader {
    private let filePath: String
    private let decoder: JSONDecoder

    init(filePath: String = "/var/run/adapteros_status.json") {
        self.filePath = filePath
        self.decoder = JSONDecoder()
    }

    /// Read and decode status. Throws mapped StatusReadError.
    func readStatus() async throws -> AdapterOSStatus {
        let (status, _) = try await readInternal()
        return status
    }

    /// Read and decode status, capturing raw hash for de-jittering and a short snippet for copy.
    /// Returns .success(AdapterOSStatus) or .failure(StatusReadError).
    func readNow() async -> Result<(AdapterOSStatus, Data, String), StatusReadError> {
        do {
            let (status, meta) = try await readInternal()
            return .success((status, meta.hash, meta.snippet))
        } catch let error as StatusReadError {
            return .failure(error)
        } catch {
            return .failure(.unknown)
        }
    }

    // MARK: - Internal read
    private func readInternal() async throws -> (AdapterOSStatus, (hash: Data, snippet: String)) {
        // Capture only the properties we need to avoid Sendable issues
        let filePath = self.filePath
        let decoder = self.decoder

        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .utility).async {
                do {
                    guard FileManager.default.fileExists(atPath: filePath) else {
                        throw StatusReadError.fileMissing
                    }

                    let handle = try FileHandle(forReadingFrom: URL(fileURLWithPath: filePath))
                    defer { try? handle.close() }
                    let data = try handle.readToEnd() ?? Data()
                    if data.isEmpty { throw StatusReadError.decodeFailed }

                    // Compute hash for de-jittering
                    let digest = SHA256.hash(data: data)
                    let hashData = Data(digest) // 32 bytes, bounded

                    // Decode JSON
                    do {
                        let status = try decoder.decode(AdapterOSStatus.self, from: data)
                        let snippet = Self.makeSnippet(from: data)
                        continuation.resume(returning: (status, (hashData, snippet)))
                    } catch {
                        continuation.resume(throwing: StatusReadError.decodeFailed)
                    }
                } catch let error as StatusReadError {
                    continuation.resume(throwing: error)
                } catch let error as NSError {
                    if error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoSuchFileError {
                        continuation.resume(throwing: StatusReadError.fileMissing)
                    } else if error.domain == NSCocoaErrorDomain && error.code == NSFileReadNoPermissionError {
                        continuation.resume(throwing: StatusReadError.permissionDenied)
                    } else {
                        continuation.resume(throwing: StatusReadError.unknown)
                    }
                }
            }
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
}


