import Foundation

/// Simple in-memory cache for API responses
final class ResponseCache: NSObject, NSCacheDelegate {
    static let shared = ResponseCache()

    private let cache = NSCache<NSString, CacheEntry>()
    private let queue = DispatchQueue(label: "com.adapteros.menu.cache", attributes: .concurrent)
    private var entryCount: Int = 0  // Track actual entry count
    private var totalSizeBytes: Int = 0

    private override init() {
        super.init()
        // Configure cache limits
        cache.countLimit = 100 // Maximum 100 entries
        cache.totalCostLimit = 1024 * 1024 // 1MB total
        cache.delegate = self

        // Set up automatic cleanup - menu bar apps don't have UIApplication
        // Cache will be cleared manually or on app restart
    }
    
    // MARK: - NSCacheDelegate
    
    /// Called by NSCache when an entry is evicted automatically (e.g., memory pressure)
    /// This ensures entryCount stays accurate even when NSCache evicts entries due to memory pressure
    @objc func cache(_ cache: NSCache<AnyObject, AnyObject>, willEvictObject obj: Any) {
        // NSCache is evicting an object - decrement our count
        queue.async(flags: .barrier) {
            self.entryCount = max(0, self.entryCount - 1)
            if let entry = obj as? CacheEntry {
                self.totalSizeBytes = max(0, self.totalSizeBytes - entry.size)
            }
        }
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    /// Cache entry with expiration
    private class CacheEntry {
        let data: Data
        let expirationDate: Date
        let eTag: String?
        let size: Int

        init(data: Data, ttl: TimeInterval, eTag: String? = nil) {
            self.data = data
            self.expirationDate = Date().addingTimeInterval(ttl)
            self.eTag = eTag
            self.size = data.count
        }

        var isExpired: Bool {
            return Date() > expirationDate
        }
    }

    /// Store response in cache
    func store(_ data: Data, forKey key: String, ttl: TimeInterval = 30.0, eTag: String? = nil) {
        queue.async(flags: .barrier) {
            let existingEntry = self.cache.object(forKey: key as NSString)
            let entry = CacheEntry(data: data, ttl: ttl, eTag: eTag)
            self.cache.setObject(entry, forKey: key as NSString)
            
            // Update entry count
            if existingEntry == nil {
                self.entryCount += 1
            } else if let existingEntry = existingEntry {
                self.totalSizeBytes = max(0, self.totalSizeBytes - existingEntry.size)
            }
            self.totalSizeBytes += entry.size
        }
    }

    /// Retrieve response from cache
    func retrieve(forKey key: String) -> Data? {
        var result: Data?

        queue.sync {
            if let entry = cache.object(forKey: key as NSString), !entry.isExpired {
                result = entry.data
            } else if let entry = cache.object(forKey: key as NSString), entry.isExpired {
                // Remove expired entry
                cache.removeObject(forKey: key as NSString)
                entryCount = max(0, entryCount - 1)
                totalSizeBytes = max(0, totalSizeBytes - entry.size)
            }
        }

        return result
    }

    /// Check if key exists and is not expired
    func hasValidEntry(forKey key: String) -> Bool {
        var result = false

        queue.sync {
            if let entry = cache.object(forKey: key as NSString), !entry.isExpired {
                result = true
            }
        }

        return result
    }

    /// Get ETag for cached response
    func eTag(forKey key: String) -> String? {
        var result: String?

        queue.sync {
            if let entry = cache.object(forKey: key as NSString), !entry.isExpired {
                result = entry.eTag
            }
        }

        return result
    }

    /// Remove specific key from cache
    func remove(key: String) {
        queue.async(flags: .barrier) {
            if let existing = self.cache.object(forKey: key as NSString) {
            self.cache.removeObject(forKey: key as NSString)
                self.entryCount = max(0, self.entryCount - 1)
                self.totalSizeBytes = max(0, self.totalSizeBytes - existing.size)
            }
        }
    }

    /// Clear all cached responses
    @objc func clearCache() {
        queue.async(flags: .barrier) {
            self.cache.removeAllObjects()
            self.entryCount = 0
            self.totalSizeBytes = 0
        }
    }

    /// Get cache statistics
    var statistics: CacheStatistics {
        var count = 0
        var totalSize = 0

        queue.sync {
            count = entryCount
            totalSize = totalSizeBytes
        }

        return CacheStatistics(entryCount: count, totalSizeBytes: totalSize)
    }
}

/// Cache statistics
struct CacheStatistics {
    let entryCount: Int
    let totalSizeBytes: Int
}

/// Cache key generation utilities
extension ResponseCache {
    static func cacheKey(for endpoint: URL, method: String = "GET", body: Data? = nil) -> String {
        var components = [method, endpoint.absoluteString]

        if let body = body, let bodyString = String(data: body, encoding: .utf8) {
            components.append(bodyString)
        }

        return components.joined(separator: "|").sha256()
    }
}

/// SHA256 extension for cache keys
private extension String {
    func sha256() -> String {
        let data = Data(self.utf8)
        let hash = data.reduce(into: [UInt8]()) { result, byte in
            result.append(byte)
        }

        // Simple hash function for cache keys (not cryptographically secure)
        var hashValue: UInt32 = 5381
        for byte in hash {
            hashValue = ((hashValue << 5) &+ hashValue) &+ UInt32(byte)
        }

        return String(hashValue, radix: 16)
    }
}
