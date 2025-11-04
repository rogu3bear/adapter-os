import Foundation

/// Simple in-memory cache for API responses
final class ResponseCache: NSObject, NSCacheDelegate {
    static let shared = ResponseCache()

    private let cache = NSCache<NSString, CacheEntry>()
    private let queue = DispatchQueue(label: "com.adapteros.menu.cache", attributes: .concurrent)
    private var entryCount: Int = 0  // Track actual entry count

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

        init(data: Data, ttl: TimeInterval, eTag: String? = nil) {
            self.data = data
            self.expirationDate = Date().addingTimeInterval(ttl)
            self.eTag = eTag
        }

        var isExpired: Bool {
            return Date() > expirationDate
        }
    }

    /// Store response in cache
    func store(_ data: Data, forKey key: String, ttl: TimeInterval = 30.0, eTag: String? = nil) {
        queue.async(flags: .barrier) {
            let wasNewEntry = self.cache.object(forKey: key as NSString) == nil
            let entry = CacheEntry(data: data, ttl: ttl, eTag: eTag)
            self.cache.setObject(entry, forKey: key as NSString)
            
            // Update entry count
            if wasNewEntry {
                self.entryCount += 1
            }
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
            let existed = self.cache.object(forKey: key as NSString) != nil
            self.cache.removeObject(forKey: key as NSString)
            if existed {
                self.entryCount = max(0, self.entryCount - 1)
            }
        }
    }

    /// Clear all cached responses
    @objc func clearCache() {
        queue.async(flags: .barrier) {
            self.cache.removeAllObjects()
            self.entryCount = 0
        }
    }

    /// Get cache statistics
    var statistics: CacheStatistics {
        var count = 0
        var totalSize = 0

        queue.sync {
            count = entryCount
            
            // Calculate approximate total size by summing cached entry sizes
            // Note: NSCache doesn't expose iteration, so we estimate based on count
            // In practice, this is approximate since we can't iterate NSCache entries
            totalSize = count * 1024  // Estimate ~1KB per entry average
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
