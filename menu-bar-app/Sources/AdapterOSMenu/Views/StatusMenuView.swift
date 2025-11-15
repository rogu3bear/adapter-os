import SwiftUI
import Combine
import AppKit

protocol StatusMenuPresenting: ObservableObject {
    var appStatus: AppStatusViewState? { get }
    var tenants: [TenantViewState] { get }
    var activeOperations: [ActiveOperationViewState] { get }
    var trustState: TrustStateViewState { get }
    var accessibilityPreferences: AccessibilityPreferences { get }
    var commandToast: CommandToast? { get set }
    var commandToastPublisher: Published<CommandToast?>.Publisher { get }
    var lastError: StatusReadError? { get }
    var isOffline: Bool { get }
    var statusSnapshot: AdapterOSStatus? { get }
    var recentStatusSnapshots: [StatusSnapshot] { get }
    var totalRequests: Int { get }
    func refresh() async
    func openLogs()
    func unloadModel() async
    func incrementRequestCount()
    func loadSampleOKStatus() async
    func loadSampleDegradedStatus() async
    func simulateMissingFile() async
}

@MainActor
extension StatusViewModel: StatusMenuPresenting {
    var statusSnapshot: AdapterOSStatus? { status }
    var commandToastPublisher: Published<CommandToast?>.Publisher { $commandToast }
}

struct StatusMenuView<ViewModel: StatusMenuPresenting>: View {
    @ObservedObject var viewModel: ViewModel

    @State private var toast: CommandToast?
    @State private var toastDismissWorkItem: DispatchWorkItem?
    @AppStorage("showPerformanceOverlay") private var showPerformanceOverlay = false

    private var accessibility: AccessibilityPreferences { viewModel.accessibilityPreferences }
    private var reducedMotion: Bool { accessibility.reduceMotion }
    private var highContrast: Bool { accessibility.highContrast }
    private var animation: Animation? {
        reducedMotion ? nil : .spring(response: 0.35, dampingFraction: 0.9)
    }

    private func debugSection(performanceOverlayBinding: Binding<Bool>) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
            Text("Debug").font(.caption).foregroundColor(.secondary)
            Toggle("Performance Overlay", isOn: performanceOverlayBinding)
                .toggleStyle(.switch)
            Button("Load Sample OK JSON") {
                Task { await viewModel.loadSampleOKStatus() }
            }
            Button("Load Sample Degraded JSON") {
                Task { await viewModel.loadSampleDegradedStatus() }
            }
            Button("Simulate Missing File") {
                Task { await viewModel.simulateMissingFile() }
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingMD) {
            problemsBanner
            header
            Divider()
            tenantsSection
            servicesSection
            Divider()
            operationsSection
            Divider()
            managementSection
            #if DEBUG
            Divider()
            debugSection(performanceOverlayBinding: $showPerformanceOverlay)
            #endif
            footer
        }
        .padding(DesignTokens.spacingMD)
        .background(DesignTokens.surface)
        .frame(minWidth: 280, maxWidth: 380)
        // .textScaleEffect(accessibility.textScale, anchor: .leading) // API may not be available
        .accessibilityElement()
        .overlay(alignment: .top) { toastView }
        .onReceive(viewModel.commandToastPublisher) { toast in
            guard let toast else { return }
            presentToast(toast, clearSource: true)
        }
        .overlay(alignment: .bottomTrailing) {
            if showPerformanceOverlay, let statusViewModel = viewModel as? StatusViewModel {
                PerformanceOverlay(metrics: statusViewModel.metrics, requestCount: statusViewModel.totalRequests)
                    .padding(8)
            }
        }
    }
}

// MARK: - Sections

private extension StatusMenuView where ViewModel: StatusMenuPresenting {
    @ViewBuilder
    var problemsBanner: some View {
        if let error = viewModel.lastError {
            ProblemsView(error: error) {
                Task { await viewModel.refresh() }
            } openLogs: {
                viewModel.openLogs()
            }
            .accessibilityIdentifier("status-problem-error")
        } else if let content = derivedProblemContent {
            ProblemsView(content: content)
                .accessibilityIdentifier("status-problem-degraded")
        }
    }

    @ViewBuilder
    var header: some View {
        HStack(alignment: .top, spacing: DesignTokens.spacingMD) {
            VStack(alignment: .leading, spacing: DesignTokens.spacingXS) {
                HStack(spacing: DesignTokens.spacingSM) {
                    Text("AdapterOS")
                        .font(DesignTokens.headerFont)
                        .accessibilityIdentifier("status-title")
                    if let status = viewModel.appStatus {
                        statusChip(for: status)
                    } else if viewModel.isOffline {
                        statusOfflineChip
                    }
                }

                if let status = viewModel.appStatus {
                    Text("Up \(status.uptimeText)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    if let metrics = status.metricsSummary {
                        Text(metrics)
                            .font(DesignTokens.metricsFont)
                            .foregroundColor(.secondary)
                    }
                    Text("Telemetry: \(status.telemetryLabel)")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                } else if viewModel.isOffline {
                    Text("Waiting for status…")
                        .font(.caption)
                        .foregroundColor(.secondary)
                        .accessibilityIdentifier("status-waiting")
                }
            }

            Spacer(minLength: DesignTokens.spacingMD)

            TrustBadgeView(state: viewModel.trustState, highContrast: highContrast)
                .accessibilityIdentifier("trust-badge")

            if let status = viewModel.appStatus {
                Button {
                    copyKernelHash(status.kernelHashFull)
                } label: {
                    Label(status.kernelHashShort, systemImage: "doc.on.doc")
                        .labelStyle(.titleAndIcon)
                }
                .buttonStyle(.plain)
                .padding(.horizontal, DesignTokens.spacingXS)
                .padding(.vertical, 6)
                .background(
                    Capsule()
                        .fill(highContrast ? Color.primary.opacity(0.1) : Color.secondary.opacity(0.12))
                )
                .accessibilityLabel("Copy kernel hash")
                .accessibilityIdentifier("kernel-hash-copy")
            }
        }
    }

    @ViewBuilder
    var tenantsSection: some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
            HStack {
                Text("Tenants")
                    .font(DesignTokens.headerFont)
                Spacer()
                if !viewModel.tenants.isEmpty {
                    Text("\(viewModel.tenants.count)")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
            }

            if viewModel.tenants.isEmpty {
                Text("No tenants mapped")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .accessibilityIdentifier("tenant-empty-state")
            } else {
                VStack(spacing: DesignTokens.spacingSM) {
                    ForEach(viewModel.tenants) { tenant in
                        tenantRow(for: tenant)
                    }
                }
            }
        }
        .accessibilityIdentifier("tenant-section")
    }

    @ViewBuilder
    var servicesSection: some View {
        if let services = viewModel.statusSnapshot?.services, !services.isEmpty {
            VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
                HStack {
                    Text("Services")
                        .font(DesignTokens.headerFont)
                    Spacer()
                    if services.count > 0 {
                        Text("\(services.count)")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }

                VStack(spacing: DesignTokens.spacingSM) {
                    ForEach(services, id: \.id) { service in
                        serviceRow(for: service)
                    }
                }
            }
            .accessibilityIdentifier("services-section")
        }
    }

    @ViewBuilder
    var operationsSection: some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
            HStack {
                Text("Operations")
                    .font(DesignTokens.headerFont)
                Spacer()
            }

            if viewModel.activeOperations.isEmpty {
                Text("No active operations")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .accessibilityIdentifier("operations-empty")
            } else {
                VStack(spacing: DesignTokens.spacingSM) {
                    ForEach(viewModel.activeOperations) { operation in
                        operationRow(for: operation)
                    }
                }
            }
        }
        .accessibilityIdentifier("operations-section")
    }

    @ViewBuilder
    var managementSection: some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
            Text("Management")
                .font(DesignTokens.headerFont)

            Button {
                openDashboard()
            } label: {
                Label("Open Dashboard", systemImage: "safari")
            }
            .accessibilityIdentifier("action-open-dashboard")

            if viewModel.appStatus?.baseModelLoaded == true {
                let name = viewModel.appStatus?.baseModelName ?? "Model"
                Button {
                    Task { await viewModel.unloadModel() }
                } label: {
                    Label("Unload \(name)", systemImage: "minus.circle")
                }
                .accessibilityIdentifier("action-unload-model")
            }

            Button {
                Task { await viewModel.refresh() }
            } label: {
                Label("Reload Now", systemImage: "arrow.clockwise")
            }
            .accessibilityIdentifier("action-reload")

            Button {
                copyStatusJSON()
            } label: {
                Label("Copy Status JSON", systemImage: "doc.on.doc")
            }
            .accessibilityIdentifier("action-copy-json")
        }
        .accessibilityIdentifier("management-section")
    }

    @ViewBuilder
    var footer: some View {
        if let timestamp = viewModel.appStatus?.lastUpdated {
            HStack {
                Spacer()
                Text(timestamp.formatted(date: .omitted, time: .standard))
                    .font(.caption2.monospacedDigit())
                    .foregroundColor(.secondary)
                    .accessibilityIdentifier("status-last-updated")
            }
        }
    }
}

// MARK: - Derived Content Builders

private extension StatusMenuView where ViewModel: StatusMenuPresenting {
    var derivedProblemContent: ProblemsView.Content? {
        // Check for service failures first (highest priority)
        if let status = viewModel.statusSnapshot, status.hasServiceFailures {
            let failedServices = status.failedServices
            let serviceNames = failedServices.map { $0.name }.joined(separator: ", ")
            let message = "The following services have failed: \(serviceNames). Check service logs for details."

            return ProblemsView.Content(
                iconSystemName: "bolt.trianglebadge.exclamationmark",
                iconColor: DesignTokens.errorColor,
                title: "Service Launch Failures",
                message: message,
                primaryLabel: "Refresh Status",
                primaryAction: { Task { await viewModel.refresh() } },
                secondaryLabel: "Open Logs",
                secondaryAction: { viewModel.openLogs() }
            )
        }

        if let status = viewModel.appStatus, status.health != .ok {
            let mode = status.health.rawValue.capitalized
            let message = "Cluster reported \(mode). Reverify trust to restore normal service."
            return ProblemsView.Content(
                iconSystemName: "exclamationmark.triangle.fill",
                iconColor: DesignTokens.degradedColor,
                title: "AdapterOS Degraded",
                message: message,
                primaryLabel: "Reverify",
                primaryAction: { Task { await viewModel.refresh() } },
                secondaryLabel: "Open Logs",
                secondaryAction: { viewModel.openLogs() }
            )
        }

        if case .failed(let reason) = viewModel.trustState {
            let message = reason.isEmpty ? "Trust verification failed." : reason
            return ProblemsView.Content(
                iconSystemName: "shield.lefthalf.fill",
                iconColor: DesignTokens.errorColor,
                title: "Trust Verification Failed",
                message: message,
                primaryLabel: "Reverify",
                primaryAction: { Task { await viewModel.refresh() } },
                secondaryLabel: "Open Logs",
                secondaryAction: { viewModel.openLogs() }
            )
        }

        return nil
    }

    @ViewBuilder
    func statusChip(for status: AppStatusViewState) -> some View {
        Text(status.headline)
            .font(.caption2)
            .fontWeight(.semibold)
            .padding(.horizontal, DesignTokens.spacingXS)
            .padding(.vertical, 4)
            .background(
                Capsule()
                    .fill(color(for: status.health))
                    .opacity(highContrast ? 0.35 : 0.2)
            )
            .accessibilityIdentifier("status-chip")
    }

    var statusOfflineChip: some View {
        Text("Offline")
            .font(.caption2)
            .fontWeight(.semibold)
            .padding(.horizontal, DesignTokens.spacingXS)
            .padding(.vertical, 4)
            .background(
                Capsule()
                    .fill(Color.secondary.opacity(highContrast ? 0.35 : 0.15))
            )
            .accessibilityIdentifier("status-chip-offline")
    }

    func tenantRow(for tenant: TenantViewState) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingXS) {
            HStack(alignment: .firstTextBaseline, spacing: DesignTokens.spacingXS) {
                Text(tenant.displayName)
                    .font(.headline)
                if let badge = tenant.badge {
                    Text(badge.text)
                        .font(.caption2)
                        .padding(.horizontal, DesignTokens.spacingXS)
                        .padding(.vertical, 3)
                        .background(
                            Capsule()
                                .fill(badgeColor(for: badge.style).opacity(highContrast ? 0.35 : 0.18))
                        )
                }
                Spacer()
            }

            if let subtitle = tenant.subtitle {
                Text(subtitle)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack(spacing: DesignTokens.spacingSM) {
                ForEach(tenant.quickActions) { action in
                    let disabled = !tenant.permissionsVerified && action.requiresVerifiedPermissions

                    Button {
                        action.action()
                    } label: {
                        Label(action.label, systemImage: action.systemImage)
                            .labelStyle(.titleAndIcon)
                    }
                    .buttonStyle(.bordered)
                    .tint(action.isDestructive ? DesignTokens.errorColor : nil)
                    .disabled(disabled)
                    .accessibilityLabel(action.accessibilityLabel)
                    .accessibilityIdentifier(action.testID)
                }
            }

            if !tenant.permissionsVerified {
                Text("Permissions pending — quick actions disabled")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .accessibilityIdentifier(tenant.testID + "-permissions")
            }
        }
        .padding(.vertical, DesignTokens.spacingXS)
        .accessibilityIdentifier(tenant.testID)
    }

    func serviceRow(for service: ServiceStatus) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingXS) {
            HStack(alignment: .firstTextBaseline, spacing: DesignTokens.spacingXS) {
                Text(service.name)
                    .font(.headline)
                    .foregroundColor(service.state == "failed" ? DesignTokens.errorColor : .primary)

                let badge = serviceStatusBadge(for: service)
                Text(badge.text)
                    .font(.caption2)
                    .padding(.horizontal, DesignTokens.spacingXS)
                    .padding(.vertical, 3)
                    .background(
                        Capsule()
                            .fill(badgeColor(for: badge.style).opacity(highContrast ? 0.35 : 0.18))
                    )

                Spacer()
            }

            if let error = service.last_error, service.state == "failed" {
                Text(error)
                    .font(.caption)
                    .foregroundColor(DesignTokens.errorColor)
                    .lineLimit(2)
            } else if let port = service.port {
                Text("Port: \(port)")
                    .font(.caption)
                    .foregroundColor(.secondary)
            } else if let pid = service.pid {
                Text("PID: \(pid)")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            HStack(spacing: DesignTokens.spacingSM) {
                if service.restart_count > 0 {
                    Text("Restarts: \(service.restart_count)")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                }

                Text(service.health_status.capitalized)
                    .font(.caption2)
                    .foregroundColor(serviceHealthColor(for: service.health_status))

                // Health trend indicator
                if let trend = (viewModel as? StatusViewModel)?.getServiceHealthTrend(for: service.id) {
                    Text(trend.trendIcon)
                        .font(.caption2)
                        .accessibilityLabel(trend.accessibilityLabel)
                }
            }
        }
        .padding(.vertical, DesignTokens.spacingXS)
        .accessibilityIdentifier("service-\(service.id)")
    }

    func operationRow(for operation: ActiveOperationViewState) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingXS) {
            HStack {
                Text(operation.title)
                    .font(.headline)
                Spacer()
                Text(operation.elapsedText())
                    .font(.caption.monospacedDigit())
                    .foregroundColor(.secondary)
            }

            if let detail = operation.detail {
                Text(detail)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            if let progress = operation.progress {
                ProgressView(value: progress, total: 1.0)
                    .progressViewStyle(.linear)
            } else {
                ProgressView()
            }

            if operation.supportsCancellation, let cancel = operation.cancelAction {
                Button("Cancel") {
                    cancel()
                }
                .buttonStyle(.bordered)
                .tint(DesignTokens.degradedColor)
                .accessibilityIdentifier(operation.testID + "-cancel")
            }
        }
        .padding(.vertical, DesignTokens.spacingXS)
        .accessibilityIdentifier(operation.testID)
    }

    func color(for health: AppStatusViewState.Health) -> Color {
        guard !highContrast else { return .primary }
        switch health {
        case .ok:
            return DesignTokens.okColor
        case .degraded:
            return DesignTokens.degradedColor
        case .error:
            return DesignTokens.errorColor
        }
    }

    func badgeColor(for style: TenantViewState.Badge.Style) -> Color {
        guard !highContrast else { return .primary }
        switch style {
        case .ok:
            return DesignTokens.okColor
        case .warning:
            return DesignTokens.degradedColor
        case .error:
            return DesignTokens.errorColor
        }
    }

    func serviceStatusBadge(for service: ServiceStatus) -> (text: String, style: TenantViewState.Badge.Style) {
        switch service.state {
        case "running":
            return ("Running", .ok)
        case "starting":
            return ("Starting", .warning)
        case "stopping":
            return ("Stopping", .warning)
        case "failed":
            return ("Failed", .error)
        case "stopped":
            return ("Stopped", .warning)
        case "restarting":
            return ("Restarting", .warning)
        default:
            return ("Unknown", .warning)
        }
    }

    func serviceHealthColor(for healthStatus: String) -> Color {
        guard !highContrast else { return .primary }
        switch healthStatus {
        case "healthy":
            return DesignTokens.okColor
        case "unhealthy":
            return DesignTokens.errorColor
        case "checking":
            return DesignTokens.degradedColor
        default:
            return .secondary
        }
    }
}

// MARK: - Toast

private extension StatusMenuView where ViewModel: StatusMenuPresenting {
    func presentToast(_ toast: CommandToast, clearSource: Bool) {
        toastDismissWorkItem?.cancel()

        if clearSource {
            viewModel.commandToast = nil
        }

        if let animation {
            withAnimation(animation) {
                self.toast = toast
            }
        } else {
            self.toast = toast
        }

        let workItem = DispatchWorkItem {
            if let animation {
                withAnimation(animation) {
                    self.toast = nil
                }
            } else {
                self.toast = nil
            }
        }
        toastDismissWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.5, execute: workItem)
    }

    @ViewBuilder
    var toastView: some View {
        if let toast {
            HStack(spacing: DesignTokens.spacingSM) {
                Image(systemName: toastIcon(for: toast.kind))
                    .foregroundStyle(toastTint(for: toast.kind))
                Text(toast.message)
                    .font(.caption)
                    .foregroundStyle(.primary)
            }
            .padding(.horizontal, DesignTokens.spacingMD)
            .padding(.vertical, DesignTokens.spacingSM)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(Color(nsColor: .windowBackgroundColor).opacity(highContrast ? 1.0 : 0.92))
                    .shadow(radius: highContrast ? 0 : 8, y: 4)
            )
            .padding(.top, DesignTokens.spacingSM)
            .accessibilityIdentifier("status-toast")
            .accessibilityLabel(toast.message)
            .transition(toastTransition)
        }
    }

    func toastIcon(for kind: CommandToast.Kind) -> String {
        switch kind {
        case .success: return "checkmark.circle.fill"
        case .error: return "exclamationmark.octagon.fill"
        case .info: return "info.circle.fill"
        }
    }

    func toastTint(for kind: CommandToast.Kind) -> Color {
        switch kind {
        case .success: return DesignTokens.okColor
        case .error: return DesignTokens.errorColor
        case .info: return .accentColor
        }
    }

    var toastTransition: AnyTransition {
        if reducedMotion {
            return .opacity
        }
        return .move(edge: .top).combined(with: .opacity)
    }

    // MARK: - Copy Functions

    func copyKernelHash(_ fullHash: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(fullHash, forType: .string)
        presentToast(CommandToast(message: "Kernel hash copied", kind: .success), clearSource: false)
    }

    func copyStatusJSON() {
        guard let status = viewModel.statusSnapshot else {
            presentToast(CommandToast(message: "No status available", kind: .info), clearSource: false)
            return
        }

        do {
            let data = try JSONEncoder().encode(status)
            guard let string = String(data: data, encoding: .utf8) else {
                throw NSError(domain: "adapteros.menu", code: -1, userInfo: [NSLocalizedDescriptionKey: "Encoding failed"])
            }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(string, forType: .string)
            presentToast(CommandToast(message: "Status JSON copied", kind: .success), clearSource: false)
        } catch {
            presentToast(CommandToast(message: "Failed to copy JSON", kind: .error), clearSource: false)
        }
    }

    func copyFormattedStatus() {
        guard let status = viewModel.statusSnapshot else {
            presentToast(CommandToast(message: "No status available", kind: .info), clearSource: false)
            return
        }

        let formattedStatus = formatStatusForSharing(status)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(formattedStatus, forType: .string)
        presentToast(CommandToast(message: "Formatted status copied", kind: .success), clearSource: false)
    }

    private func formatStatusForSharing(_ status: AdapterOSStatus) -> String {
        var output = """
        AdapterOS Status Report
        Generated: \(Date.now.formatted())

        System Health: \(status.status.capitalized)
        Uptime: \(status.uptimeFormatted)
        Adapters Loaded: \(status.adapters_loaded)
        Workers: \(status.worker_count)

        Base Model: \(status.base_model_name ?? "Not Loaded")
        """

        if let loaded = status.base_model_loaded, loaded {
            output += " (Loaded)"
        }

        output += "\nTelemetry: \(status.telemetry_mode.capitalized)\n"

        if let services = status.services, !services.isEmpty {
            output += "\nServices:\n"
            for service in services {
                let stateEmoji = serviceStateEmoji(for: service.state)
                let healthEmoji = serviceHealthEmoji(for: service.health_status)
                output += "• \(service.name): \(service.state.capitalized) \(stateEmoji) | \(service.health_status.capitalized) \(healthEmoji)\n"

                if let error = service.last_error, service.state == "failed" {
                    output += "  Error: \(error)\n"
                }
                if service.restart_count > 0 {
                    output += "  Restarts: \(service.restart_count)\n"
                }
                if let port = service.port {
                    output += "  Port: \(port)\n"
                }
                if let pid = service.pid {
                    output += "  PID: \(pid)\n"
                }
            }
        }

        output += "\nKernel: \(status.kernelHashShort)\n"

        return output
    }

    private func serviceStateEmoji(for state: String) -> String {
        switch state {
        case "running": return "🟢"
        case "starting": return "🟡"
        case "stopping": return "🟠"
        case "failed": return "🔴"
        case "stopped": return "⚫"
        case "restarting": return "🔄"
        default: return "⚪"
        }
    }

    private func serviceHealthEmoji(for health: String) -> String {
        switch health {
        case "healthy": return "💚"
        case "unhealthy": return "💔"
        case "checking": return "🔍"
        default: return "❓"
        }
    }

    func openDashboard() {
        guard let url = URL(string: "http://localhost:3200") else { return }
        NSWorkspace.shared.open(url)
        presentToast(CommandToast(message: "Opening dashboard", kind: .info), clearSource: false)
    }

}

// MARK: - Trust Badge View

private struct TrustBadgeView: View {
    let state: TrustStateViewState
    let highContrast: Bool

    private var icon: String {
        switch state {
        case .signed: return "checkmark.shield.fill"
        case .pending: return "clock.badge.questionmark"
        case .unsigned: return "shield"
        case .failed: return "exclamationmark.shield.fill"
        }
    }

    private var label: String {
        switch state {
        case .signed(let signature):
            if let verifiedAt = signature.verifiedAt {
                return "Signed • " + verifiedAt.formatted(date: .omitted, time: .shortened)
            }
            return "Signed"
        case .pending:
            return "Verifying"
        case .unsigned:
            return "Unsigned"
        case .failed:
            return "Trust Failed"
        }
    }

    private var tint: Color {
        guard !highContrast else { return .primary }
        switch state {
        case .signed:
            return DesignTokens.okColor
        case .pending, .unsigned:
            return DesignTokens.degradedColor
        case .failed:
            return DesignTokens.errorColor
        }
    }

    private var accessibilityLabelText: String {
        switch state {
        case .signed:
            return "Trust verified"
        case .pending:
            return "Trust verification pending"
        case .unsigned:
            return "Trust unsigned"
        case .failed(let reason):
            return reason.isEmpty ? "Trust verification failed" : reason
        }
    }

    var body: some View {
        HStack(spacing: DesignTokens.spacingXS) {
            Image(systemName: icon)
                .foregroundColor(tint)
            Text(label)
                .font(.caption2)
                .foregroundColor(.primary)
        }
        .padding(.horizontal, DesignTokens.spacingXS)
        .padding(.vertical, 4)
        .background(
            Capsule()
                .fill(tint.opacity(highContrast ? 0.35 : 0.2))
        )
        .accessibilityLabel(accessibilityLabelText)
    }
}

// MARK: - Status History Tooltip

struct StatusHistoryTooltip: View {
    let recentStatuses: [StatusSnapshot]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Recent Status Changes")
                .font(.caption)
                .foregroundColor(.secondary)

            if recentStatuses.isEmpty {
                Text("No recent changes")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .padding(.vertical, 4)
            } else {
                ForEach(recentStatuses.prefix(3), id: \.timestamp) { snapshot in
                    HStack(spacing: 8) {
                        statusIcon(for: snapshot.status)
                            .font(.caption)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(snapshot.status.capitalized)
                                .font(.caption)
                                .foregroundColor(.primary)
                            Text(snapshot.timeAgo)
                                .font(.caption2)
                                .foregroundColor(.secondary)
                        }
                        Spacer()
                        Text(snapshot.uptimeFormatted)
                            .font(.caption2.monospacedDigit())
                            .foregroundColor(.secondary)
                    }
                    .padding(.vertical, 2)
                }
            }
        }
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(Color(nsColor: .windowBackgroundColor).opacity(0.95))
                .shadow(radius: 8, y: 4)
        )
        .frame(minWidth: 200, maxWidth: 280)
    }

    private func statusIcon(for status: String) -> some View {
        let (systemName, color): (String, Color) = switch status {
        case "ok": ("checkmark.circle.fill", .green)
        case "degraded": ("exclamationmark.triangle.fill", .yellow)
        case "error": ("xmark.circle.fill", .red)
        default: ("questionmark.circle.fill", .gray)
        }

        return Image(systemName: systemName)
            .foregroundColor(color)
    }
}

// MARK: - Performance Overlay

struct PerformanceOverlay: View {
    let metrics: SystemMetrics?
    let requestCount: Int

    var body: some View {
        VStack(alignment: .trailing, spacing: 2) {
            if let metrics = metrics {
                Text("CPU: \(Int(metrics.cpuUsage))%")
                    .font(.caption2.monospaced())
                    .foregroundColor(.green)
                Text("Mem: \(String(format: "%.1f", metrics.memoryUsedGB))GB")
                    .font(.caption2.monospaced())
                    .foregroundColor(.blue)
                Text("Req: \(requestCount)")
                    .font(.caption2.monospaced())
                    .foregroundColor(.orange)
            } else {
                Text("Performance metrics unavailable")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(
            Color.black.opacity(0.8)
                .cornerRadius(6)
        )
        .foregroundColor(.white)
    }
}

// MARK: - Preview

#if DEBUG
class PreviewStatusMenuViewModel: StatusMenuPresenting {
    @Published var appStatus: AppStatusViewState? = .sampleOk
    @Published var tenants: [TenantViewState] = .samples
    var metrics: SystemMetrics? = SystemMetrics(
        cpuUsage: 15.2,
        memoryUsedGB: 2.8,
        memoryTotalGB: 16.0
    )
    var totalRequests: Int = 42
    @Published var activeOperations: [ActiveOperationViewState] = [.sample]
    @Published var trustState: TrustStateViewState = .signed(.init(issuer: "Preview", verifiedAt: Date()))
    @Published var accessibilityPreferences = AccessibilityPreferences()
    @Published var commandToast: CommandToast?
    var lastError: StatusReadError?
    var isOffline: Bool { false }
    var statusSnapshot: AdapterOSStatus? { nil }
    var recentStatusSnapshots: [StatusSnapshot] { [] }
    var commandToastPublisher: Published<CommandToast?>.Publisher { $commandToast }

    func refresh() async {}
    func openLogs() {}
    func unloadModel() async {}
    func incrementRequestCount() {
        totalRequests += 1
    }
    func loadSampleOKStatus() async {}
    func loadSampleDegradedStatus() async {}
    func simulateMissingFile() async {}
}

final class PreviewDegradedStatusMenuViewModel: PreviewStatusMenuViewModel {
    override init() {
        super.init()
        appStatus = .sampleDegraded
        trustState = .failed(reason: "Signature mismatch detected")
        tenants = [.sample]
        activeOperations = []
        lastError = nil
    }
}

struct StatusMenuView_Previews: PreviewProvider {
    static var previews: some View {
        Group {
            StatusMenuView(viewModel: PreviewStatusMenuViewModel())
                .previewDisplayName("Healthy")
            StatusMenuView(viewModel: PreviewDegradedStatusMenuViewModel())
                .previewDisplayName("Degraded")
        }
        .frame(width: 360)
        .padding()
    }
}
#endif

// MARK: - Accessibility / QA Notes

// Focus order (top to bottom): Problem banner (if present) → header copy hash button → trust badge → tenant quick
// actions (left to right per tenant) → operations cancel buttons → management actions (Open Dashboard → Unload
// Model → Reload Now → Copy Status JSON). Supporting accessibility labels map to `accessibilityLabel` per element,
// and test IDs are assigned via `accessibilityIdentifier` with prefixes: `status-*`, `tenant-*`, `operations-*`,
// `action-*`, and `status-toast` for QA automation.

struct ConcreteStatusMenuView: View {
    @ObservedObject var viewModel: StatusViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingMD) {
            if let error = viewModel.lastError {
                ProblemsView(error: error) {
                    Task { await viewModel.refresh() }
                } openLogs: {
                    viewModel.openLogs()
                }
            }

            header

            if let status = viewModel.status {
                infoSections(status: status)
            } else if viewModel.lastError == nil {
                Text("Waiting for status…")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }

            Divider()

            actionsSection

            footer
        }
        .padding(DesignTokens.spacingMD)
        .frame(minWidth: 300)
    }

    private var header: some View {
        HStack(spacing: DesignTokens.spacingSM) {
            Image(systemName: viewModel.iconName)
                .foregroundColor(iconColor)
            Text("AdapterOS")
                .font(DesignTokens.headerFont)
            Spacer()
            if let status = viewModel.status {
                Text(status.uptimeFormatted)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
    }

    private func infoSections(status: AdapterOSStatus) -> some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
            // Chips line
            HStack(spacing: DesignTokens.spacingSM) {
                Label(status.deterministic ? "Deterministic On" : "Deterministic Off", systemImage: status.deterministic ? "checkmark.seal.fill" : "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundColor(status.deterministic ? DesignTokens.okColor : DesignTokens.degradedColor)
            }

            // Metrics one-liner
            if let metrics = viewModel.metrics {
                Text("CPU \(Int(metrics.cpuUsage))% • Mem \(String(format: "%.0f", metrics.memoryUsedGB)) GB")
                    .font(DesignTokens.metricsFont)
                    .foregroundColor(.secondary)
            }

            // Primary rows
            StatusRow(label: "Adapters", value: String(status.adapters_loaded), systemImage: "puzzlepiece.extension", color: .secondary)
            StatusRow(label: "Workers", value: String(status.worker_count), systemImage: "person.3", color: .secondary)
            StatusRow(label: "Base Model", value: status.base_model_name ?? ((status.base_model_loaded ?? false) ? "Loaded" : "Not Loaded"), systemImage: "cube", color: .secondary)
            StatusRow(label: "Telemetry", value: status.telemetry_mode, systemImage: "antenna.radiowaves.left.and.right", color: .secondary)

            // Kernel hash short + copy full
            HStack(spacing: DesignTokens.spacingSM) {
                Text("Kernel")
                    .foregroundColor(.secondary)
                Spacer()
                Text(status.kernelHashShort)
                    .fontWeight(.medium)
                Button(action: { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(status.kernel_hash, forType: .string) }) {
                    Image(systemName: "doc.on.doc")
                }
                .buttonStyle(.plain)
                .help("Copy full kernel hash")
            }
            .font(DesignTokens.metricsFont)
            
            // Design tokens checksum + copy full
            if !DesignTokens.checksum.isEmpty {
                HStack(spacing: DesignTokens.spacingSM) {
                    Text("Tokens")
                        .foregroundColor(.secondary)
                    Spacer()
                    Text(DesignTokens.checksum.prefix(8))
                        .fontWeight(.medium)
                        .foregroundColor(DesignTokens.isDegradedMode ? DesignTokens.degradedColor : .primary)
                    Button(action: {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(DesignTokens.checksum, forType: .string)
                    }) {
                        Image(systemName: "doc.on.doc")
                    }
                    .buttonStyle(.plain)
                    .help("Copy full tokens checksum")
                }
                .font(DesignTokens.metricsFont)
            }
        }
    }



    private var footer: some View {
        HStack {
            Spacer()
            if let ts = viewModel.lastUpdate {
                Text(ts.formatted(date: .omitted, time: .standard))
                    .font(.caption2.monospaced())
                    .foregroundColor(.secondary)
            }
        }
    }

    // MARK: - Helper Functions

    private func openDashboard() {
        guard let url = URL(string: "http://localhost:3200") else { return }
        NSWorkspace.shared.open(url)
        // Note: presentToast is not available here since it's in the extension
        // This is a simplified version without toast
    }

    private func copyFormattedStatus() {
        guard let status = viewModel.statusSnapshot else {
            return
        }

        let formattedStatus = formatStatusForSharing(status)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(formattedStatus, forType: .string)
    }

    private func copyStatusJSON() {
        guard let status = viewModel.statusSnapshot else {
            return
        }

        do {
            let data = try JSONEncoder().encode(status)
            guard let string = String(data: data, encoding: .utf8) else {
                return
            }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(string, forType: .string)
        } catch {
            // Error handling without toast
        }
    }

    private func formatStatusForSharing(_ status: AdapterOSStatus) -> String {
        var output = """
        AdapterOS Status Report
        Generated: \(Date.now.formatted())

        System Health: \(status.status.capitalized)
        Uptime: \(status.uptimeFormatted)
        Adapters Loaded: \(status.adapters_loaded)
        Workers: \(status.worker_count)

        Base Model: \(status.base_model_name ?? "Not Loaded")
        """

        if let loaded = status.base_model_loaded, loaded {
            output += " (Loaded)"
        }

        output += "\nTelemetry: \(status.telemetry_mode.capitalized)\n"

        if let services = status.services, !services.isEmpty {
            output += "\nServices:\n"
            for service in services {
                let stateEmoji = serviceStateEmoji(for: service.state)
                let healthEmoji = serviceHealthEmoji(for: service.health_status)
                output += "• \(service.name): \(service.state.capitalized) \(stateEmoji) | \(service.health_status.capitalized) \(healthEmoji)\n"

                if let error = service.last_error, service.state == "failed" {
                    output += "  Error: \(error)\n"
                }
                if service.restart_count > 0 {
                    output += "  Restarts: \(service.restart_count)\n"
                }
                if let port = service.port {
                    output += "  Port: \(port)\n"
                }
                if let pid = service.pid {
                    output += "  PID: \(pid)\n"
                }
            }
        }

        output += "\nKernel: \(status.kernelHashShort)\n"

        return output
    }

    private func serviceStateEmoji(for state: String) -> String {
        switch state {
        case "running": return "🟢"
        case "starting": return "🟡"
        case "stopping": return "🟠"
        case "failed": return "🔴"
        case "stopped": return "⚫"
        case "restarting": return "🔄"
        default: return "⚪"
        }
    }

    private func serviceHealthEmoji(for health: String) -> String {
        switch health {
        case "healthy": return "💚"
        case "unhealthy": return "💔"
        case "checking": return "🔍"
        default: return "❓"
        }
    }

    private var actionsSection: some View {
        VStack(alignment: .leading, spacing: DesignTokens.spacingSM) {
            Button {
                openDashboard()
            } label: {
                Label("Open Dashboard", systemImage: "safari")
            }

            if let status = viewModel.statusSnapshot,
               status.base_model_loaded == true,
               let modelName = status.base_model_name {
                Button {
                    Task { await viewModel.unloadModel() }
                } label: {
                    Label("Unload Model (\(modelName))", systemImage: "minus.circle")
                }
            }

            Button {
                Task { await viewModel.refresh() }
            } label: {
                Label("Reload Now", systemImage: "arrow.clockwise")
            }
            .keyboardShortcut("r", modifiers: .command)

            Button {
                viewModel.openLogs()
            } label: {
                Label("Open Logs", systemImage: "doc.text.magnifyingglass")
            }
            .keyboardShortcut("l", modifiers: [.command, .shift])

            Button {
                copyFormattedStatus()
            } label: {
                Label("Copy Status Report", systemImage: "doc.text")
            }

            Button {
                copyStatusJSON()
            } label: {
                Label("Copy Status JSON", systemImage: "doc.on.doc")
            }
        }
    }

    private var iconColor: Color {
        guard let status = viewModel.status?.status else { return .gray }
        switch status {
        case "ok": return DesignTokens.okColor
        case "degraded": return DesignTokens.degradedColor
        case "error": return DesignTokens.errorColor
        default: return .gray
        }
    }
}


