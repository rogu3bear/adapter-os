import Foundation
import IOKit
import Metal

/// Native system metrics collector using macOS APIs
class SystemMetricsCollector {
    
    private var previousCPUInfo: host_cpu_load_info?
    private var previousTimestamp: Date?
    
    /// Collect current system metrics
    func collect() -> SystemMetrics {
        let cpu = getCPUUsage()
        let (memUsed, memTotal) = getMemoryInfo()
        
        return SystemMetrics(
            cpuUsage: cpu,
            memoryUsedGB: memUsed,
            memoryTotalGB: memTotal
        )
    }
    
    // MARK: - CPU Usage
    
    private func getCPUUsage() -> Double {
        var size = mach_msg_type_number_t(MemoryLayout<host_cpu_load_info_data_t>.size / MemoryLayout<integer_t>.size)
        var cpuInfo = host_cpu_load_info_data_t()
        
        let result = withUnsafeMutablePointer(to: &cpuInfo) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(size)) {
                host_statistics(mach_host_self(), HOST_CPU_LOAD_INFO, $0, &size)
            }
        }
        
        guard result == KERN_SUCCESS else {
            return 0.0
        }
        
        // Calculate usage since last call
        if let previous = previousCPUInfo,
           let prevTime = previousTimestamp,
           Date().timeIntervalSince(prevTime) > 0.5 {  // At least 0.5s between samples
            
            let userDiff = Double(cpuInfo.cpu_ticks.0 - previous.cpu_ticks.0)
            let systemDiff = Double(cpuInfo.cpu_ticks.1 - previous.cpu_ticks.1)
            let idleDiff = Double(cpuInfo.cpu_ticks.2 - previous.cpu_ticks.2)
            let niceDiff = Double(cpuInfo.cpu_ticks.3 - previous.cpu_ticks.3)
            
            let totalDiff = userDiff + systemDiff + idleDiff + niceDiff
            
            previousCPUInfo = cpuInfo
            previousTimestamp = Date()
            
            guard totalDiff > 0 else { return 0.0 }
            
            let activeDiff = userDiff + systemDiff + niceDiff
            return (activeDiff / totalDiff) * 100.0
        } else {
            // First call or too soon - just store and return 0
            previousCPUInfo = cpuInfo
            previousTimestamp = Date()
            return 0.0
        }
    }
    
    // MARK: - Memory Info
    
    private func getMemoryInfo() -> (used: Double, total: Double) {
        let processInfo = ProcessInfo.processInfo
        let totalMemory = Double(processInfo.physicalMemory)
        
        var stats = vm_statistics64()
        var size = mach_msg_type_number_t(MemoryLayout<vm_statistics64>.size / MemoryLayout<integer_t>.size)
        
        let result = withUnsafeMutablePointer(to: &stats) {
            $0.withMemoryRebound(to: integer_t.self, capacity: Int(size)) {
                host_statistics64(mach_host_self(), HOST_VM_INFO64, $0, &size)
            }
        }
        
        guard result == KERN_SUCCESS else {
            return (0, totalMemory / (1024 * 1024 * 1024))
        }
        
        let pageSize = Double(vm_kernel_page_size)
        
        // Calculate used memory
        let active = Double(stats.active_count) * pageSize
        let wired = Double(stats.wire_count) * pageSize
        let compressed = Double(stats.compressor_page_count) * pageSize
        
        let usedMemory = active + wired + compressed
        
        // Convert to GB
        let usedGB = usedMemory / (1024 * 1024 * 1024)
        let totalGB = totalMemory / (1024 * 1024 * 1024)
        
        return (usedGB, totalGB)
    }
}




