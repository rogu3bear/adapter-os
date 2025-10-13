import Foundation

/// Status structure matching the JSON written by mplora-server
struct AdapterOSStatus: Codable {
    let status: String              // "ok" | "degraded" | "error"
    let uptime_secs: UInt64
    let adapters_loaded: Int
    let deterministic: Bool
    let kernel_hash: String
    let telemetry_mode: String
    let worker_count: Int
    
    /// Check if status indicates system is healthy
    var isHealthy: Bool {
        status == "ok" || status == "degraded"
    }
    
    /// Format uptime as human-readable string
    var uptimeFormatted: String {
        let hours = uptime_secs / 3600
        let minutes = (uptime_secs % 3600) / 60
        
        if hours > 0 {
            return "\(hours)h \(minutes)m"
        } else if minutes > 0 {
            return "\(minutes)m"
        } else {
            return "\(uptime_secs)s"
        }
    }
}

/// System metrics collected from native macOS APIs
struct SystemMetrics {
    let cpuUsage: Double        // 0.0 - 100.0
    let gpuUsage: Double        // 0.0 - 100.0
    let memoryUsedGB: Double    // GB
    let memoryTotalGB: Double   // GB
    
    var memoryPercent: Double {
        guard memoryTotalGB > 0 else { return 0 }
        return (memoryUsedGB / memoryTotalGB) * 100.0
    }
}




