import XCTest
@testable import adapterOSMenu

final class ResponseCacheTests: XCTestCase {
    var cache: ResponseCache!
    
    override func setUp() {
        cache = ResponseCache.shared
        cache.clearCache()
    }
    
    override func tearDown() {
        cache.clearCache()
    }
    
    func testEntryCountIncrements() {
        let stats1 = cache.statistics
        let initialCount = stats1.entryCount
        
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 60.0)
        
        let stats2 = cache.statistics
        XCTAssertEqual(stats2.entryCount, initialCount + 1, "Entry count should increment on store")
    }
    
    func testEntryCountDecrementsOnRemove() {
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 60.0)
        
        let stats1 = cache.statistics
        let count1 = stats1.entryCount
        
        cache.remove(key: "key1")
        
        let stats2 = cache.statistics
        XCTAssertEqual(stats2.entryCount, count1 - 1, "Entry count should decrement on remove")
    }
    
    func testEntryCountDecrementsOnExpiration() {
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 0.01) // Very short TTL
        
        let stats1 = cache.statistics
        let count1 = stats1.entryCount
        
        // Wait for expiration
        let expectation = XCTestExpectation(description: "Wait for expiration")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
            expectation.fulfill()
        }
        wait(for: [expectation], timeout: 1.0)
        
        // Try to retrieve expired entry - should remove it
        _ = cache.retrieve(forKey: "key1")
        
        let stats2 = cache.statistics
        XCTAssertEqual(stats2.entryCount, count1 - 1, "Entry count should decrement when expired entry removed")
    }
    
    func testStatisticsAccuracy() {
        cache.clearCache()
        
        XCTAssertEqual(cache.statistics.entryCount, 0, "Empty cache should have 0 entries")
        
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 60.0)
        cache.store(Data("test2".utf8), forKey: "key2", ttl: 60.0)
        cache.store(Data("test3".utf8), forKey: "key3", ttl: 60.0)
        
        let stats = cache.statistics
        XCTAssertEqual(stats.entryCount, 3, "Should have 3 entries")
        
        cache.remove(key: "key1")
        
        let stats2 = cache.statistics
        XCTAssertEqual(stats2.entryCount, 2, "Should have 2 entries after removal")
    }
    
    func testDuplicateStoreDoesNotIncrement() {
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 60.0)
        
        let stats1 = cache.statistics
        let count1 = stats1.entryCount
        
        // Store again with same key
        cache.store(Data("test2".utf8), forKey: "key1", ttl: 60.0)
        
        let stats2 = cache.statistics
        XCTAssertEqual(stats2.entryCount, count1, "Storing same key should not increment count")
    }
    
    func testClearCacheResetsCount() {
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 60.0)
        cache.store(Data("test2".utf8), forKey: "key2", ttl: 60.0)
        
        cache.clearCache()
        
        let stats = cache.statistics
        XCTAssertEqual(stats.entryCount, 0, "Clear should reset count to 0")
    }
    
    func testConcurrentCacheOperations() {
        let expectation = XCTestExpectation(description: "Concurrent operations")
        expectation.expectedFulfillmentCount = 100
        
        // Capture cache explicitly to avoid optional unwrapping issues
        let testCache = cache!
        
        for i in 0..<100 {
            DispatchQueue.global().async {
                testCache.store(Data("test\(i)".utf8), forKey: "key\(i)", ttl: 60.0)
                expectation.fulfill()
            }
        }
        
        wait(for: [expectation], timeout: 5.0)
        
        // Count should be accurate even with concurrent operations
        let stats = cache.statistics
        XCTAssertGreaterThanOrEqual(stats.entryCount, 0, "Count should be non-negative")
        XCTAssertLessThanOrEqual(stats.entryCount, 100, "Count should not exceed entries added")
    }
    
    func testNSCacheEvictionTracking() {
        cache.clearCache()
        
        // Store many entries - NSCache will evict some when memory pressure occurs
        // We can't directly control NSCache eviction, but we can verify the delegate
        // properly tracks evictions through the entry count
        
        for i in 0..<50 {
            cache.store(Data("test\(i)".utf8), forKey: "key\(i)", ttl: 60.0)
        }
        
        // Wait a bit for any evictions to occur
        let expectation = XCTestExpectation(description: "Wait for evictions")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            expectation.fulfill()
        }
        wait(for: [expectation], timeout: 1.0)
        
        let stats = cache.statistics
        // Entry count should be accurate even after NSCache evictions
        // The delegate should have decremented count for any evicted entries
        XCTAssertLessThanOrEqual(stats.entryCount, 50, "Count should not exceed entries added")
        XCTAssertGreaterThanOrEqual(stats.entryCount, 0, "Count should be non-negative")
    }
    
    func testCacheStatisticsAccuracy() {
        cache.clearCache()
        
        let data1 = Data("test1".utf8)
        let data2 = Data("test2".utf8)
        let data3 = Data("test3".utf8)
        
        cache.store(data1, forKey: "key1", ttl: 60.0)
        cache.store(data2, forKey: "key2", ttl: 60.0)
        cache.store(data3, forKey: "key3", ttl: 60.0)
        
        let stats = cache.statistics
        XCTAssertEqual(stats.entryCount, 3, "Should have 3 entries")
        
        let expectedSize = data1.count + data2.count + data3.count
        XCTAssertEqual(stats.totalSizeBytes, expectedSize, "Total size should match sum of entry sizes")
        
        cache.remove(key: "key1")
        
        let stats2 = cache.statistics
        XCTAssertEqual(stats2.entryCount, 2, "Should have 2 entries after removal")
        XCTAssertEqual(stats2.totalSizeBytes, data2.count + data3.count, "Size should decrease by removed entry size")
    }
    
    func testTTLExpiration() {
        cache.clearCache()
        
        cache.store(Data("test1".utf8), forKey: "key1", ttl: 0.01)
        
        XCTAssertNotNil(cache.retrieve(forKey: "key1"), "Entry should be available before expiration")
        
        let expectation = XCTestExpectation(description: "Wait for expiration")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.1) {
            expectation.fulfill()
        }
        wait(for: [expectation], timeout: 1.0)
        
        XCTAssertNil(cache.retrieve(forKey: "key1"), "Entry should be expired and removed")
        
        let stats = cache.statistics
        XCTAssertEqual(stats.entryCount, 0, "Expired entry should be removed from count")
    }
}
