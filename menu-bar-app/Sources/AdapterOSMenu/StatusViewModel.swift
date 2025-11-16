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
<<<<<<< HEAD
    @Published var appStatus: AppStatusViewState?
    @Published var tenants: [TenantViewState] = []
    @Published var activeOperations: [ActiveOperationViewState] = []
    @Published var trustState: TrustStateViewState = .pending
    @Published var accessibilityPreferences = AccessibilityPreferences()
    @Published var commandToast: CommandToast?

    // MARK: - Request Tracking
    @Published private(set) var totalRequests: Int = 0

    // MARK: - Private State

    private let statusPath = "/var/run/adapteros_status.json"
    private let reader = StatusReader() // Will check local var/ first, then /var/run
    private let metricsCollector = SystemMetricsCollector()
    private let serviceClient = ServicePanelClient()
    private let notificationManager = NotificationManager.shared
=======
    
    // MARK: - Private State
    
    private let statusPath = "/var/run/adapteros_status.json"
    private let reader = StatusReader()
    private let metricsCollector = SystemMetricsCollector()
>>>>>>> integration-branch
    private var pollTimerCancellable: AnyCancellable?
    private var metricsTimerCancellable: AnyCancellable?
    private var vnodeSource: DispatchSourceFileSystemObject?
    private var lastHash: Data?
    private var transientErrorSuppressed = false
    private var sleepWakeObservers: [NSObjectProtocol] = []
<<<<<<< HEAD
    private var currentStatusPath: String?
    private var watcherFailureCount: Int = 0
    private let maxWatcherFailures = 3
    private var lastWatcherRetryTime: Date?
    private var isSettingUpWatcher = false
    private var previousServiceStates: [String: String] = [:] // serviceId -> state
    private var previousServiceHealth: [String: String] = [:] // serviceId -> health_status
    private var _recentStatusSnapshots: [StatusSnapshot] = []
    private let maxStatusHistory = 3

    // Public accessor for recent status snapshots
    var recentStatusSnapshots: [StatusSnapshot] {
        _recentStatusSnapshots
    }
    
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
=======
    
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
                if self.vnodeSource == nil { self.setupWatcher() }
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
>>>>>>> integration-branch
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
            let hasNewContent = lastHash != hash || status == nil
            if hasNewContent {
                lastHash = hash
                status = newStatus
                // Record status change for history
                addStatusSnapshot(newStatus)
            }
            lastUpdate = Date()
            appStatus = AppStatusViewState(status: newStatus, metrics: metrics, lastUpdated: lastUpdate)
            updateTrustState(with: newStatus)
            checkServiceNotifications(for: newStatus)
            transientErrorSuppressed = false
        case .failure(let error):
            // Suppress transient errors for one cycle
            if transientErrorSuppressed {
                lastError = error
                isOffline = true
                status = nil
                appStatus = nil
                trustState = .pending
            } else {
                transientErrorSuppressed = true
            }
        }
    }

    private func checkServiceNotifications(for newStatus: AdapterOSStatus) {
        guard let services = newStatus.services else { return }

        for service in services {
            let previousState = previousServiceStates[service.id]

            // Check for failures
            if service.state == "failed" && previousState != "failed" {
                notificationManager.notifyCriticalFailure(service)
            }
            // Check for recoveries
            else if service.state == "running" && previousState == "failed" {
                notificationManager.notifyRecovery(service)
            }

            // Update previous states
            previousServiceStates[service.id] = service.state
            previousServiceHealth[service.id] = service.health_status
        }
    }

    /// Get health trend for a specific service
    func getServiceHealthTrend(for serviceId: String) -> ServiceHealthTrend? {
        guard let currentHealth = status?.services?.first(where: { $0.id == serviceId })?.health_status,
              let previousHealth = previousServiceHealth[serviceId],
              currentHealth != previousHealth else {
            return nil
        }

        return ServiceHealthTrend(
            serviceId: serviceId,
            previousHealth: previousHealth,
            currentHealth: currentHealth,
            changeTime: lastUpdate ?? Date()
        )
    }

    private func addStatusSnapshot(_ status: AdapterOSStatus) {
        let snapshot = StatusSnapshot(status: status, timestamp: Date())
        _recentStatusSnapshots.insert(snapshot, at: 0)

        // Keep only the most recent snapshots
        if _recentStatusSnapshots.count > maxStatusHistory {
            _recentStatusSnapshots.removeLast()
        }
    }

    // MARK: - Icon & Tooltip Logic
    
    private func updateIconAndTooltip() {
        guard let status = status else {
            iconName = "bolt.slash.circle.fill"
            tooltip = "AdapterOS OFFLINE"
            return
        }

<<<<<<< HEAD
        // Determine icon based on state, prioritizing service failures
        if let metrics = metrics, metrics.cpuUsage > 80 {
            iconName = "flame.fill"
        } else if status.hasServiceFailures {
            iconName = "bolt.trianglebadge.exclamationmark"
=======
        // Determine icon based on state
        if let metrics = metrics, metrics.cpuUsage > 80 {
            iconName = "flame.fill"
>>>>>>> integration-branch
        } else if status.status == "error" {
            iconName = "bolt.slash.circle.fill"
        } else if status.status == "degraded" {
            iconName = "bolt.badge.exclamationmark"
        } else {
            iconName = "bolt.circle.fill"
<<<<<<< HEAD
        }

        // Build tooltip
        var tooltipComponents: [String] = []
        tooltipComponents.append("AdapterOS \(status.status.uppercased())")

        // Add service failure information
        if !status.failedServices.isEmpty {
            let failedServices = status.failedServices
            let failureText = failedServices.count == 1
                ? "1 service failed"
                : "\(failedServices.count) services failed"
            tooltipComponents.append(failureText)
        }

        // Add CPU/memory info
        if let metrics = metrics {
            let cpu = String(format: "%.0f%%", metrics.cpuUsage)
            let mem = String(format: "%.0fGB", metrics.memoryUsedGB)
            tooltipComponents.append("\(cpu) CPU · \(mem) RAM")
        }

        tooltip = tooltipComponents.joined(separator: " · ")
=======
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
>>>>>>> integration-branch
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
    
    func unloadModel() async {
        guard let status = status, let modelId = status.base_model_id else {
            Logger.shared.warning("No model loaded to unload")
            return
        }

        let operationId = UUID().uuidString
        let operation = ActiveOperationViewState(
            id: operationId,
            title: "Unloading base model",
            detail: status.base_model_name,
            startedAt: Date(),
            progress: nil,
            supportsCancellation: false,
            cancelAction: nil,
            testID: "operation-unload-model"
        )
        activeOperations.append(operation)

        do {
            incrementRequestCount()
            _ = try await serviceClient.unloadModel(modelId)
            Logger.shared.info("Model unloaded successfully", context: ["model_id": modelId])

            // Refresh status to show unloaded state
            await refresh()
            commandToast = CommandToast(message: "Model unloaded", kind: .success)
        } catch {
            Logger.shared.error("Failed to unload model", error: error, context: ["model_id": modelId])
            lastError = StatusReadError.readError("Failed to unload model: \(error.localizedDescription)")
            commandToast = CommandToast(message: "Failed to unload model", kind: .error)
        }

        activeOperations.removeAll { $0.id == operationId }
    }

    func quit() {
        NSApplication.shared.terminate(nil)
    }

<<<<<<< HEAD
    // MARK: - Request Tracking

    func incrementRequestCount() {
        totalRequests += 1
    }

    // MARK: - Debug Test Injections

    func loadSampleOKStatus() async {
        // Create a sample status manually since no static samples exist
        let sampleStatus = AdapterOSStatus(
            schema_version: "1.0",
            status: "ok",
            uptime_secs: 3600,
            adapters_loaded: 3,
            deterministic: true,
            kernel_hash: "abcdef123456",
            telemetry_mode: "full",
            worker_count: 4,
            base_model_loaded: true,
            base_model_id: "llama-2-7b",
            base_model_name: "Llama-2-7B",
            base_model_status: "loaded",
            base_model_memory_mb: 14336,
            services: [
                ServiceStatus(
                    id: "api-server",
                    name: "API Server",
                    state: "running",
                    pid: 12345,
                    port: 8080,
                    health_status: "healthy",
                    restart_count: 0,
                    last_error: nil
                ),
                ServiceStatus(
                    id: "worker-pool",
                    name: "Worker Pool",
                    state: "running",
                    pid: nil,
                    port: nil,
                    health_status: "healthy",
                    restart_count: 0,
                    last_error: nil
                )
            ]
        )
        status = sampleStatus
        lastError = nil
        isOffline = false
        lastUpdate = Date()
        appStatus = AppStatusViewState(status: sampleStatus, metrics: metrics, lastUpdated: lastUpdate)
        addStatusSnapshot(sampleStatus)
        commandToast = CommandToast(message: "Loaded sample OK status", kind: .info)
    }

    func loadSampleDegradedStatus() async {
        // Create a sample degraded status
        let sampleStatus = AdapterOSStatus(
            schema_version: "1.0",
            status: "degraded",
            uptime_secs: 1800,
            adapters_loaded: 2,
            deterministic: true,
            kernel_hash: "def456789012",
            telemetry_mode: "minimal",
            worker_count: 3,
            base_model_loaded: true,
            base_model_id: "llama-2-7b",
            base_model_name: "Llama-2-7B",
            base_model_status: "loaded",
            base_model_memory_mb: 14336,
            services: [
                ServiceStatus(
                    id: "api-server",
                    name: "API Server",
                    state: "running",
                    pid: 12345,
                    port: 8080,
                    health_status: "healthy",
                    restart_count: 0,
                    last_error: nil
                ),
                ServiceStatus(
                    id: "worker-pool",
                    name: "Worker Pool",
                    state: "failed",
                    pid: nil,
                    port: nil,
                    health_status: "unhealthy",
                    restart_count: 2,
                    last_error: "Connection timeout"
                )
            ]
        )
        status = sampleStatus
        lastError = nil
        isOffline = false
        lastUpdate = Date()
        appStatus = AppStatusViewState(status: sampleStatus, metrics: metrics, lastUpdated: lastUpdate)
        addStatusSnapshot(sampleStatus)
        commandToast = CommandToast(message: "Loaded sample degraded status", kind: .info)
    }

    func simulateMissingFile() async {
        lastError = StatusReadError.fileMissing
        isOffline = true
        status = nil
        appStatus = nil
        trustState = .pending
        commandToast = CommandToast(message: "Simulated missing file", kind: .error)
    }

    // MARK: - VNODE watcher
    private func setupWatcher() {
        if isSettingUpWatcher {
            return
        }
        isSettingUpWatcher = true
        defer { isSettingUpWatcher = false }

=======
    // MARK: - VNODE watcher
    private func setupWatcher() {
>>>>>>> integration-branch
        // Cancel any existing watcher first
        vnodeSource?.cancel()
        vnodeSource = nil

<<<<<<< HEAD
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
=======
        guard FileManager.default.fileExists(atPath: statusPath) else { return }
        let fd = open(statusPath, O_EVTONLY)
        guard fd >= 0 else { return }
>>>>>>> integration-branch

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
<<<<<<< HEAD
        
=======
>>>>>>> integration-branch
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
<<<<<<< HEAD
                if let status = self.status {
                    self.appStatus = AppStatusViewState(status: status, metrics: self.metrics, lastUpdated: self.lastUpdate)
                }
=======
>>>>>>> integration-branch
                self.updateIconAndTooltip()
            }
        // initial sample
        metrics = metricsCollector.collect()
<<<<<<< HEAD
        if let status = status {
            appStatus = AppStatusViewState(status: status, metrics: metrics, lastUpdated: lastUpdate)
        }
=======
>>>>>>> integration-branch
    }

    // MARK: - Sleep/Wake handling
    private func setupSleepWake() {
        let nc = NSWorkspace.shared.notificationCenter
        let will = nc.addObserver(forName: NSWorkspace.willSleepNotification, object: nil, queue: .main) { [weak self] _ in
<<<<<<< HEAD
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
=======
            self?.stopPolling()
        }
        let did = nc.addObserver(forName: NSWorkspace.didWakeNotification, object: nil, queue: .main) { [weak self] _ in
            guard let self else { return }
            self.setupWatcher()
            self.startPolling()
            Task { @MainActor in await self.refresh() }
>>>>>>> integration-branch
        }
        sleepWakeObservers.append(contentsOf: [will, did])
    }
}




// MARK: - Derived State Helpers

private extension StatusViewModel {
    func updateTrustState(with status: AdapterOSStatus) {
        let previousTrustState = trustState

        if status.deterministic {
            trustState = .signed(
                .init(
                    issuer: "AdapterOS Control Plane",
                    verifiedAt: Date()
                )
            )
        } else if status.status == "error" {
            trustState = .failed(reason: "Trust evidence invalidated")
        } else {
            trustState = .unsigned
        }

        // Notify about trust verification failures
        if case .failed(let reason) = trustState,
           previousTrustState != trustState {
            notificationManager.notifyTrustIssue(reason)
        }
    }

    // MARK: - Debug Methods

    func injectTestStatus() {
        // Create a test status for debugging
        let testStatus = AdapterOSStatus(
            schema_version: "1.0",
            status: "ok",
            uptime_secs: 3600, // 1 hour
            adapters_loaded: 5,
            deterministic: true,
            kernel_hash: "abc123def456",
            telemetry_mode: "enabled",
            worker_count: 3,
            base_model_loaded: true,
            base_model_id: "test-model",
            base_model_name: "Test Model",
            base_model_status: "loaded",
            base_model_memory_mb: 1024,
            services: [
                ServiceStatus(
                    id: "web-api",
                    name: "Web API",
                    state: "running",
                    pid: 1234,
                    port: 8080,
                    health_status: "healthy",
                    restart_count: 0,
                    last_error: nil
                ),
                ServiceStatus(
                    id: "database",
                    name: "Database",
                    state: "running",
                    pid: 5678,
                    port: 5432,
                    health_status: "healthy",
                    restart_count: 1,
                    last_error: nil
                )
            ]
        )

        // Inject the test status
        let metadata = (hash: Data("test".utf8), snippet: "test status")
        let _ = reader.injectValidStatusForTesting(testStatus, metadata: metadata)

        // Refresh to pick up the injected status
        Task { @MainActor in
            await refresh()
        }
    }

    func clearAllCaches() {
        // Clear response cache
        ResponseCache.shared.clearCache()

        // Clear any other cached data
        // Note: Status snapshots are kept for debugging

        logger.info("Cleared all application caches")
    }

    func showPerformanceMetrics() {
        let metrics = metricsCollector.collect()
        let memoryMB = Double(metrics.memoryUsedGB)
        let cpuPercent = metrics.cpuUsage

        logger.info("Performance Metrics", context: [
            "cpu_usage": cpuPercent,
            "memory_used_gb": memoryMB,
            "memory_total_gb": metrics.memoryTotalGB
        ])
    }
}

