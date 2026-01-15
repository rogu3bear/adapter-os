import Foundation

<<<<<<< HEAD
/// Status of a managed service
struct ServiceStatus: Codable {
    let id: String                    // Service identifier
    let name: String                  // Human-readable service name
    let state: String                 // "stopped" | "starting" | "running" | "stopping" | "failed" | "restarting"
    let pid: UInt32?                  // Process ID if running
    let port: UInt16?                 // Port number if applicable
    let health_status: String         // "unknown" | "healthy" | "unhealthy" | "checking"
    let restart_count: UInt32         // Number of restart attempts
    let last_error: String?           // Last error message if any
}

/// adapterOS status model decoded from /var/run/adapteros_status.json
/// Unknown keys are ignored by default Decodable behavior.
/// Optional fields allow for backward compatibility with older JSON formats.
struct adapterOSStatus: Codable {
    let schema_version: String?   // Schema version for compatibility (optional for legacy)
=======
/// adapterOS status model decoded from /var/run/adapteros_status.json
/// Unknown keys are ignored by default Decodable behavior.
struct adapterOSStatus: Decodable {
>>>>>>> integration-branch
    let status: String            // "ok" | "degraded" | "error"
    let uptime_secs: UInt64
    let adapters_loaded: Int
    let deterministic: Bool
    let kernel_hash: String
    let telemetry_mode: String
    let worker_count: Int
<<<<<<< HEAD
    let base_model_loaded: Bool?
    let base_model_id: String?
    let base_model_name: String?
    let base_model_status: String?
    let base_model_memory_mb: Int?
    let services: [ServiceStatus]? // Service status information (optional for backward compatibility)
=======
    let base_model_loaded: Bool
    let base_model_id: String?
    let base_model_name: String?
    let base_model_status: String
    let base_model_memory_mb: Int?
>>>>>>> integration-branch

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
<<<<<<< HEAD

    /// Services with failures
    var failedServices: [ServiceStatus] {
        return services?.filter { $0.state == "failed" } ?? []
    }

    /// Services that are not running (stopped or failed)
    var nonRunningServices: [ServiceStatus] {
        return services?.filter { $0.state == "stopped" || $0.state == "failed" } ?? []
    }

    /// Whether any services have failed
    var hasServiceFailures: Bool {
        return !failedServices.isEmpty
    }
=======
>>>>>>> integration-branch
}


