//
//  Models.swift
//  AdapterOSInstaller
//
//  Data models for installation state and configuration
//

import Foundation

enum InstallStep: String, Codable, CaseIterable {
    case precheck = "precheck"
    case createDirs = "create_dirs"
    case buildBinaries = "build_binaries"
    case initDatabase = "init_db"
    case buildMetal = "build_metal"
    case downloadModel = "download_model"
    case createTenant = "create_tenant"
    case complete = "complete"
    
    var displayName: String {
        switch self {
        case .precheck: return "Pre-flight Checks"
        case .createDirs: return "Creating Directories"
        case .buildBinaries: return "Building Binaries"
        case .initDatabase: return "Initializing Database"
        case .buildMetal: return "Compiling Metal Kernels"
        case .downloadModel: return "Downloading Model"
        case .createTenant: return "Creating Default Tenant"
        case .complete: return "Installation Complete"
        }
    }
    
    var progressValue: Double {
        switch self {
        case .precheck: return 0.0
        case .createDirs: return 0.1
        case .buildBinaries: return 0.4
        case .initDatabase: return 0.6
        case .buildMetal: return 0.7
        case .downloadModel: return 0.85
        case .createTenant: return 0.95
        case .complete: return 1.0
        }
    }
}

enum InstallMode: String {
    case full = "full"
    case minimal = "minimal"
    
    var displayName: String {
        switch self {
        case .full: return "Full Installation"
        case .minimal: return "Minimal Installation"
        }
    }
    
    var description: String {
        switch self {
        case .full:
            return "Builds binaries, initializes database, compiles Metal kernels, downloads model, and creates default tenant"
        case .minimal:
            return "Builds binaries, initializes database, and compiles Metal kernels only"
        }
    }
}

struct ProgressUpdate: Codable {
    let step: String
    let progress: Double
    let message: String
    let status: String
}

enum InstallationStatus {
    case notStarted
    case running
    case completed
    case failed(Error)
}

struct HardwareCheckResult {
    let name: String
    let passed: Bool
    let message: String
    let isRequired: Bool
}

