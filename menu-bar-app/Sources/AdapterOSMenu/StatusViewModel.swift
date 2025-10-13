import Foundation
import Combine
import SwiftUI

/// ViewModel managing status polling and UI state
@MainActor
class StatusViewModel: ObservableObject {
    
    // MARK: - Published State
    
    @Published var status: AdapterOSStatus?
    @Published var metrics: SystemMetrics?
    @Published var isOffline: Bool = true
    @Published var iconName: String = "bolt.slash"
    @Published var tooltip: String = "AdapterOS OFFLINE"
    
    // MARK: - Private State
    
    private let statusPaths = [
        "/var/run/adapteros_status.json",
        "var/adapteros_status.json"
    ]
    
    private let metricsCollector = SystemMetricsCollector()
    private var timer: Timer?
    
    // MARK: - Lifecycle
    
    init() {
        startPolling()
        // Do initial refresh immediately
        Task {
            await refresh()
        }
    }
    
    deinit {
        timer?.invalidate()
        timer = nil
    }
    
    // MARK: - Polling
    
    func startPolling() {
        // Poll every 5 seconds
        timer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.refresh()
            }
        }
        timer?.tolerance = 0.5  // Allow 0.5s tolerance for efficiency
    }
    
    func stopPolling() {
        timer?.invalidate()
        timer = nil
    }
    
    // MARK: - Refresh
    
    func refresh() async {
        // Collect system metrics
        metrics = metricsCollector.collect()
        
        // Read AdapterOS status
        status = readStatus()
        isOffline = (status == nil)
        
        // Update icon and tooltip
        updateIconAndTooltip()
    }
    
    // MARK: - Status Reading
    
    private func readStatus() -> AdapterOSStatus? {
        // Try each path in order
        for path in statusPaths {
            if let status = readStatusFromPath(path) {
                return status
            }
        }
        return nil
    }
    
    private func readStatusFromPath(_ path: String) -> AdapterOSStatus? {
        let fileURL = URL(fileURLWithPath: path)
        
        guard FileManager.default.fileExists(atPath: path) else {
            return nil
        }
        
        do {
            let data = try Data(contentsOf: fileURL)
            let decoder = JSONDecoder()
            return try decoder.decode(AdapterOSStatus.self, from: data)
        } catch {
            // Silent failure - file might be mid-write
            return nil
        }
    }
    
    // MARK: - Icon & Tooltip Logic
    
    private func updateIconAndTooltip() {
        guard let status = status, let metrics = metrics else {
            iconName = "bolt.slash"
            tooltip = "AdapterOS OFFLINE"
            return
        }
        
        // Determine icon based on state
        if metrics.cpuUsage > 70 {
            iconName = "flame"
        } else if !status.deterministic {
            iconName = "bolt.slash"
        } else {
            iconName = "bolt.circle"
        }
        
        // Build tooltip
        let statusText = status.status.uppercased()
        let cpu = String(format: "%.0f%%", metrics.cpuUsage)
        let gpu = String(format: "%.0f%%", metrics.gpuUsage)
        let ram = String(format: "%.0fGB", metrics.memoryUsedGB)
        
        tooltip = "AdapterOS \(statusText) · \(cpu) CPU · \(gpu) GPU · \(ram) RAM"
    }
    
    // MARK: - Actions
    
    func openLogs() {
        // Open Console.app filtered to AdapterOS logs
        let script = """
        tell application "Console"
            activate
        end tell
        """
        
        if let appleScript = NSAppleScript(source: script) {
            var error: NSDictionary?
            appleScript.executeAndReturnError(&error)
            if let error = error {
                print("Failed to open Console.app: \(error)")
            }
        }
    }
    
    func quit() {
        NSApplication.shared.terminate(nil)
    }
}




