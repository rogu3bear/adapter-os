//
//  CheckpointManager.swift
//  AdapterOSInstaller
//
//  Resume detection and checkpoint management
//

import Foundation

class CheckpointManager {
    private let checkpointPath = "/tmp/adapteros_install.state"
    
    func hasCheckpoint() -> Bool {
        return FileManager.default.fileExists(atPath: checkpointPath)
    }
    
    func getCheckpointInfo() -> (step: String, timestamp: String)? {
        guard let content = try? String(contentsOfFile: checkpointPath, encoding: .utf8) else {
            return nil
        }
        
        var step: String?
        var timestamp: String?
        
        for line in content.components(separatedBy: .newlines) {
            if line.hasPrefix("LAST_COMPLETED=") {
                step = line.replacingOccurrences(of: "LAST_COMPLETED=", with: "")
            } else if line.hasPrefix("LAST_TIMESTAMP=") {
                timestamp = line.replacingOccurrences(of: "LAST_TIMESTAMP=", with: "")
            }
        }
        
        if let step = step, let timestamp = timestamp {
            return (step, timestamp)
        }
        
        return nil
    }
    
    func clearCheckpoint() {
        try? FileManager.default.removeItem(atPath: checkpointPath)
    }
    
    func displayNameForStep(_ step: String) -> String {
        switch step {
        case "create_dirs": return "Creating Directories"
        case "build_binaries": return "Building Binaries"
        case "init_db": return "Initializing Database"
        case "build_metal": return "Compiling Metal Kernels"
        case "download_model": return "Downloading Model"
        case "create_tenant": return "Creating Tenant"
        default: return step
        }
    }
}

