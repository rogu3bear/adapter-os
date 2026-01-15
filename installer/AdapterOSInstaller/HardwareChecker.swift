//
//  HardwareChecker.swift
//  adapterOSInstaller
//
//  Hardware validation for Apple Silicon and system requirements
//

import Foundation

class HardwareChecker: ObservableObject {
    @Published var checks: [HardwareCheckResult] = []
    @Published var allRequiredPass: Bool = false
    
    func runChecks() {
        var results: [HardwareCheckResult] = []
        
        // Check 1: Apple Silicon (ARM64)
        results.append(checkAppleSilicon())
        
        // Check 2: Minimum RAM (16GB)
        results.append(checkRAM())
        
        // Check 3: Free disk space (10GB recommended)
        results.append(checkDiskSpace())
        
        // Check 4: macOS version
        results.append(checkMacOSVersion())
        
        self.checks = results
        self.allRequiredPass = results.filter { $0.isRequired }.allSatisfy { $0.passed }
    }
    
    private func checkAppleSilicon() -> HardwareCheckResult {
        let result = sysctlQuery("hw.optional.arm64")
        let isArm64 = result == "1"
        
        if isArm64 {
            // Get CPU brand to determine M-series
            let cpuBrand = sysctlQuery("machdep.cpu.brand_string")
            return HardwareCheckResult(
                name: "Apple Silicon",
                passed: true,
                message: "✓ \(cpuBrand)",
                isRequired: true
            )
        } else {
            return HardwareCheckResult(
                name: "Apple Silicon",
                passed: false,
                message: "✗ Intel CPU detected. adapterOS requires Apple Silicon (M1 or newer)",
                isRequired: true
            )
        }
    }
    
    private func checkRAM() -> HardwareCheckResult {
        let memsize = sysctlQueryInt("hw.memsize")
        let memoryGB = Double(memsize) / 1_073_741_824.0 // Convert bytes to GB
        let minRequiredGB = 16.0
        
        let passed = memoryGB >= minRequiredGB
        let message = passed
            ? String(format: "✓ %.1f GB (≥ %.0f GB required)", memoryGB, minRequiredGB)
            : String(format: "✗ %.1f GB detected (%.0f GB required)", memoryGB, minRequiredGB)
        
        return HardwareCheckResult(
            name: "RAM",
            passed: passed,
            message: message,
            isRequired: true
        )
    }
    
    private func checkDiskSpace() -> HardwareCheckResult {
        let homeURL = FileManager.default.homeDirectoryForCurrentUser
        
        do {
            let values = try homeURL.resourceValues(forKeys: [.volumeAvailableCapacityKey])
            if let capacity = values.volumeAvailableCapacity {
                let freeGB = Double(capacity) / 1_073_741_824.0
                let recommendedGB = 10.0
                
                let passed = freeGB >= recommendedGB
                let message = passed
                    ? String(format: "✓ %.1f GB free (≥ %.0f GB recommended)", freeGB, recommendedGB)
                    : String(format: "⚠ %.1f GB free (%.0f GB recommended)", freeGB, recommendedGB)
                
                return HardwareCheckResult(
                    name: "Disk Space",
                    passed: passed,
                    message: message,
                    isRequired: false // Recommended but not required
                )
            }
        } catch {
            return HardwareCheckResult(
                name: "Disk Space",
                passed: false,
                message: "⚠ Unable to check disk space",
                isRequired: false
            )
        }
        
        return HardwareCheckResult(
            name: "Disk Space",
            passed: false,
            message: "⚠ Unable to check disk space",
            isRequired: false
        )
    }
    
    private func checkMacOSVersion() -> HardwareCheckResult {
        let version = ProcessInfo.processInfo.operatingSystemVersion
        let versionString = "\(version.majorVersion).\(version.minorVersion).\(version.patchVersion)"
        
        // Require macOS 12.0+ (Monterey) for Metal 3 support
        let passed = version.majorVersion >= 12
        let message = passed
            ? "✓ macOS \(versionString)"
            : "✗ macOS \(versionString) (12.0+ required)"
        
        return HardwareCheckResult(
            name: "macOS Version",
            passed: passed,
            message: message,
            isRequired: true
        )
    }
    
    // MARK: - Helpers
    
    private func sysctlQuery(_ key: String) -> String {
        var size: Int = 0
        sysctlbyname(key, nil, &size, nil, 0)
        var value = [CChar](repeating: 0, count: size)
        sysctlbyname(key, &value, &size, nil, 0)
        return String(cString: value)
    }
    
    private func sysctlQueryInt(_ key: String) -> Int64 {
        var value: Int64 = 0
        var size = MemoryLayout<Int64>.size
        sysctlbyname(key, &value, &size, nil, 0)
        return value
    }
}

