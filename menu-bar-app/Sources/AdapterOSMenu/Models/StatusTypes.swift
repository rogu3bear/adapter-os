import Foundation

/// AdapterOS status model decoded from /var/run/adapteros_status.json
/// Unknown keys are ignored by default Decodable behavior.
struct AdapterOSStatus: Decodable {
    let schema_version: String    // Schema version for compatibility
    let status: String            // "ok" | "degraded" | "error"
    let uptime_secs: UInt64
    let adapters_loaded: Int
    let deterministic: Bool
    let kernel_hash: String
    let telemetry_mode: String
    let worker_count: Int
    let base_model_loaded: Bool
    let base_model_id: String?
    let base_model_name: String?
    let base_model_status: String
    let base_model_memory_mb: Int?

    /// Human-readable uptime string like "3h 12m"
    var uptimeFormatted: String {
        let hours = uptime_secs / 3600
        let minutes = (uptime_secs % 3600) / 60
        if hours > 0 { return "\(hours)h \(minutes)m" }
        if minutes > 0 { return "\(minutes)m" }
        return "\(uptime_secs)s"
    }

    /// Short kernel hash (first 8 chars) for display
    var kernelHashShort: String {
        return String(kernel_hash.prefix(8))
    }
}


