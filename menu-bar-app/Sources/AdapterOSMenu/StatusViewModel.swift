import Foundation
import Combine
import SwiftUI
import Dispatch
import AppKit
import Darwin

/// ViewModel managing status polling and UI state
@MainActor
class StatusViewModel: ObservableObject {
    
    // MARK: - Published State
    
    @Published var status: AdapterOSStatus?
    @Published var metrics: SystemMetrics?
    @Published var isOffline: Bool = true
    @Published var iconName: String = "bolt.slash.circle.fill"
    @Published var tooltip: String = "AdapterOS OFFLINE"
    @Published var lastError: StatusReadError?
    @Published var lastUpdate: Date?
    
    // MARK: - Private State
    
    private let reader = StatusReader()
    private let metricsCollector = SystemMetricsCollector()
    private var pollTimerCancellable: AnyCancellable?
    private var metricsTimerCancellable: AnyCancellable?
    private var vnodeSource: DispatchSourceFileSystemObject?
    private var lastHash: Data?
    private var transientErrorSuppressed = false
    private var sleepWakeObservers: [NSObjectProtocol] = []
    private var currentStatusPath: String?
    private var watcherFailureCount: Int = 0
    private let maxWatcherFailures = 3
    private var lastWatcherRetryTime: Date?
    
    /// Find the first existing status file path
    private func findStatusFile() -> String? {
        let fileManager = FileManager.default
        for path in StatusReader.defaultPaths {
            if fileManager.fileExists(atPath: path) {
                return path
            }
        }
        return nil
    }
    
    // MARK: - Lifecycle
    
    init() {
        setupWatcher()
        setupSleepWake()
        startPolling()
        startMetricsSampling()
        Task { await refresh() }
    }
    
    deinit {
        vnodeSource?.cancel()
        vnodeSource = nil
        pollTimerCancellable?.cancel()
        metricsTimerCancellable?.cancel()
        let nc = NSWorkspace.shared.notificationCenter
        for obs in sleepWakeObservers { nc.removeObserver(obs) }
        sleepWakeObservers.removeAll()
    }
    
    // MARK: - Polling
    
    func startPolling() {
        pollTimerCancellable = Timer.publish(every: 5, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                guard let self else { return }
                // Check if status file path changed (e.g., server started using fallback)
                let foundPath = self.findStatusFile()
                if foundPath != self.currentStatusPath {
                    // Path changed, recreate watcher
                    self.setupWatcher()
                } else if self.vnodeSource == nil {
                    // No watcher but path hasn't changed, try to set up watcher
                    // Only retry if we haven't exceeded max failures
                    if self.watcherFailureCount < self.maxWatcherFailures {
                        self.setupWatcher()
                    }
                }
                // Always refresh via polling (works even if watcher fails)
                Task { @MainActor in await self.refresh() }
            }
    }

    func stopPolling() {
        pollTimerCancellable?.cancel()
        pollTimerCancellable = nil
    }
    
    // MARK: - Refresh
    
    func refresh() async {
        await readStatusAndUpdate()
        updateIconAndTooltip()
    }
    
    // MARK: - Status Reading
    
    private func readStatusAndUpdate() async {
        switch await reader.readNow() {
        case .success(let (newStatus, hash, _)):
            lastError = nil
            isOffline = false
            if lastHash != hash {
                lastHash = hash
                status = newStatus
                lastUpdate = Date()
            }
            transientErrorSuppressed = false
        case .failure(let error):
            // Suppress transient errors for one cycle
            if transientErrorSuppressed {
                lastError = error
                isOffline = true
                status = nil
            } else {
                transientErrorSuppressed = true
            }
        }
    }
    
    // MARK: - Icon & Tooltip Logic
    
    private func updateIconAndTooltip() {
        guard let status = status else {
            iconName = "bolt.slash.circle.fill"
            tooltip = "AdapterOS OFFLINE"
            return
        }

        // Determine icon based on state
        if let metrics = metrics, metrics.cpuUsage > 80 {
            iconName = "flame.fill"
        } else if status.status == "error" {
            iconName = "bolt.slash.circle.fill"
        } else if status.status == "degraded" {
            iconName = "bolt.badge.exclamationmark"
        } else {
            iconName = "bolt.circle.fill"
        }

        // Build tooltip (CPU/mem only)
        if let metrics = metrics {
            let statusText = status.status.uppercased()
            let cpu = String(format: "%.0f%%", metrics.cpuUsage)
            let mem = String(format: "%.0fGB", metrics.memoryUsedGB)
            tooltip = "AdapterOS \(statusText) · \(cpu) CPU · \(mem) RAM"
        } else {
            tooltip = "AdapterOS \(status.status.uppercased())"
        }
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

    // MARK: - VNODE watcher
    private func setupWatcher() {
        // Cancel any existing watcher first
        vnodeSource?.cancel()
        vnodeSource = nil

        // Exponential backoff: if we've failed recently, wait before retrying
        if let lastRetry = lastWatcherRetryTime, watcherFailureCount > 0 {
            let timeSinceRetry = Date().timeIntervalSince(lastRetry)
            let backoffSeconds = min(pow(2.0, Double(watcherFailureCount - 1)), 30.0) // Cap at 30s
            if timeSinceRetry < backoffSeconds {
                // Too soon to retry, skip this attempt
                return
            }
        }

        // Find the current status file path
        guard let statusPath = findStatusFile() else {
            // File doesn't exist yet, will retry with exponential backoff
            watcherFailureCount += 1
            lastWatcherRetryTime = Date()
            if watcherFailureCount <= maxWatcherFailures {
                let backoffSeconds = min(pow(2.0, Double(watcherFailureCount - 1)), 30.0)
                print("StatusViewModel: Status file not found, will retry in \(Int(backoffSeconds))s (attempt \(watcherFailureCount)/\(maxWatcherFailures))")
            } else if watcherFailureCount == maxWatcherFailures + 1 {
                print("StatusViewModel: Status file not found after \(maxWatcherFailures) attempts, falling back to polling only")
            }
            return
        }
        
        // Update current path
        currentStatusPath = statusPath
        
        let fd = open(statusPath, O_EVTONLY)
        guard fd >= 0 else {
            // Failed to open file descriptor
            watcherFailureCount += 1
            lastWatcherRetryTime = Date()
            let errnoValue = errno
            if watcherFailureCount <= maxWatcherFailures {
                let backoffSeconds = min(pow(2.0, Double(watcherFailureCount - 1)), 30.0)
                print("StatusViewModel: Failed to open status file '\(statusPath)' (errno: \(errnoValue)), will retry in \(Int(backoffSeconds))s (attempt \(watcherFailureCount)/\(maxWatcherFailures))")
            } else if watcherFailureCount == maxWatcherFailures + 1 {
                print("StatusViewModel: Failed to open status file after \(maxWatcherFailures) attempts, falling back to polling only")
            }
            return
        }

        // Successfully opened file descriptor, reset failure count
        if watcherFailureCount > 0 {
            print("StatusViewModel: Successfully set up watcher for '\(statusPath)' after \(watcherFailureCount) previous failures")
            watcherFailureCount = 0
        }

        let source = DispatchSource.makeFileSystemObjectSource(fileDescriptor: fd, eventMask: [.write, .rename, .delete, .attrib], queue: DispatchQueue.main)
        source.setEventHandler { [weak self, weak source] in
            guard let self, let source else { return }
            let flags = DispatchSource.FileSystemEvent(rawValue: source.data)
            Task { @MainActor in await self.refresh() }
            if flags.contains(.rename) || flags.contains(.delete) {
                self.recreateWatcherAfterDelay()
            }
        }
        source.setCancelHandler {
            close(fd)
        }
        
        source.resume()
        vnodeSource = source
    }

    private func recreateWatcherAfterDelay() {
        vnodeSource?.cancel()
        vnodeSource = nil
        // Give the writer a moment to move the new file into place
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) { [weak self] in
            self?.setupWatcher()
        }
    }

    // MARK: - Metrics sampling (CPU/memory only)
    private func startMetricsSampling() {
        metricsTimerCancellable = Timer.publish(every: 10, on: .main, in: .common)
            .autoconnect()
            .sink { [weak self] _ in
                guard let self else { return }
                self.metrics = self.metricsCollector.collect()
                self.updateIconAndTooltip()
            }
        // initial sample
        metrics = metricsCollector.collect()
    }

    // MARK: - Sleep/Wake handling
    private func setupSleepWake() {
        let nc = NSWorkspace.shared.notificationCenter
        let will = nc.addObserver(forName: NSWorkspace.willSleepNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                self?.stopPolling()
            }
        }
        let did = nc.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            Task { @MainActor in
                guard let self else { return }
                self.setupWatcher()
                self.startPolling()
                await self.refresh()
            }
        }
        sleepWakeObservers.append(contentsOf: [will, did])
    }
}




