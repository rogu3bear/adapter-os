import Foundation

/// System metrics collected from native macOS APIs
struct SystemMetrics {
    let cpuUsage: Double        // 0.0 - 100.0
    let memoryUsedGB: Double    // GB
    let memoryTotalGB: Double   // GB
    
    var memoryPercent: Double {
        guard memoryTotalGB > 0 else { return 0 }
        return (memoryUsedGB / memoryTotalGB) * 100.0
    }
}




