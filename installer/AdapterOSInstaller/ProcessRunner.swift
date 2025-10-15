//
//  ProcessRunner.swift
//  AdapterOSInstaller
//
//  Async process execution with streaming output and JSON parsing
//

import Foundation
import Combine

class ProcessRunner: ObservableObject {
    @Published var currentStep: InstallStep = .precheck
    @Published var progress: Double = 0.0
    @Published var logs: [String] = []
    @Published var status: InstallationStatus = .notStarted
    
    private var process: Process?
    private let workspaceRoot: String
    
    init(workspaceRoot: String = "/Users/star/Dev/adapter-os") {
        self.workspaceRoot = workspaceRoot
    }
    
    func runBootstrap(mode: InstallMode, airGapped: Bool) async throws {
        status = .running
        logs = []
        progress = 0.0
        
        let scriptPath = "\(workspaceRoot)/scripts/bootstrap_with_checkpoints.sh"
        
        // Verify script exists
        guard FileManager.default.fileExists(atPath: scriptPath) else {
            throw NSError(domain: "ProcessRunner", code: 1, 
                         userInfo: [NSLocalizedDescriptionKey: "Bootstrap script not found at \(scriptPath)"])
        }
        
        let checkpointFile = "/tmp/adapteros_install.state"
        let modeArg = mode.rawValue
        let airGappedArg = airGapped ? "true" : "false"
        let jsonArg = "true"
        
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/bash")
        process.arguments = [scriptPath, checkpointFile, modeArg, airGappedArg, jsonArg]
        process.currentDirectoryURL = URL(fileURLWithPath: workspaceRoot)
        
        let outputPipe = Pipe()
        let errorPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = errorPipe
        
        self.process = process
        
        // Handle stdout
        outputPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if data.count > 0 {
                if let line = String(data: data, encoding: .utf8) {
                    self?.processOutputLine(line)
                }
            }
        }
        
        // Handle stderr
        errorPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            let data = handle.availableData
            if data.count > 0 {
                if let line = String(data: data, encoding: .utf8) {
                    DispatchQueue.main.async {
                        self?.logs.append("[ERROR] \(line)")
                    }
                }
            }
        }
        
        do {
            try process.run()
            process.waitUntilExit()
            
            // Close handlers
            outputPipe.fileHandleForReading.readabilityHandler = nil
            errorPipe.fileHandleForReading.readabilityHandler = nil
            
            if process.terminationStatus == 0 {
                DispatchQueue.main.async {
                    self.status = .completed
                    self.currentStep = .complete
                    self.progress = 1.0
                }
            } else {
                let error = NSError(domain: "ProcessRunner", code: Int(process.terminationStatus),
                                  userInfo: [NSLocalizedDescriptionKey: "Installation failed with exit code \(process.terminationStatus)"])
                DispatchQueue.main.async {
                    self.status = .failed(error)
                }
                throw error
            }
        } catch {
            DispatchQueue.main.async {
                self.status = .failed(error)
            }
            throw error
        }
    }
    
    func cancel() {
        process?.terminate()
        status = .notStarted
    }
    
    private func processOutputLine(_ line: String) {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        
        // Try to parse as JSON progress update
        if trimmed.starts(with: "{"), let data = trimmed.data(using: .utf8) {
            do {
                let update = try JSONDecoder().decode(ProgressUpdate.self, from: data)
                DispatchQueue.main.async {
                    self.progress = update.progress
                    if let step = InstallStep(rawValue: update.step) {
                        self.currentStep = step
                    }
                    self.logs.append(update.message)
                }
                return
            } catch {
                // Not valid JSON, treat as regular log line
            }
        }
        
        // Regular log line
        if !trimmed.isEmpty {
            DispatchQueue.main.async {
                self.logs.append(trimmed)
                
                // Try to extract progress from text patterns
                self.extractProgressFromText(trimmed)
            }
        }
    }
    
    private func extractProgressFromText(_ text: String) {
        // Pattern matching for common progress indicators
        if text.contains("Creating") || text.contains("create_dirs") {
            currentStep = .createDirs
            progress = max(progress, 0.1)
        } else if text.contains("Building") || text.contains("Compiling") || text.contains("build_binaries") {
            currentStep = .buildBinaries
            progress = max(progress, 0.2)
        } else if text.contains("Database") || text.contains("init_db") {
            currentStep = .initDatabase
            progress = max(progress, 0.6)
        } else if text.contains("Metal") || text.contains("build_metal") {
            currentStep = .buildMetal
            progress = max(progress, 0.7)
        } else if text.contains("Downloading") || text.contains("download_model") {
            currentStep = .downloadModel
            progress = max(progress, 0.85)
        } else if text.contains("tenant") || text.contains("create_tenant") {
            currentStep = .createTenant
            progress = max(progress, 0.95)
        } else if text.contains("smoke") || text.contains("test") || text.contains("smoke_test") {
            currentStep = .smokeTest
            progress = max(progress, 0.98)
        }
    }
}

