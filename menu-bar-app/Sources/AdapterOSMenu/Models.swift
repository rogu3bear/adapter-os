import Foundation
import SwiftUI

/// System metrics collected from native macOS APIs
struct SystemMetrics {
    let cpuUsage: Double        // 0.0 - 100.0
    let memoryUsedGB: Double    // GB
    let memoryTotalGB: Double   // GB
    
    var memoryPercent: Double {
        guard memoryTotalGB > 0 else { return 0 }
        return (memoryUsedGB / memoryTotalGB) * 100.0
    }
}

/// Snapshot of status at a specific point in time for history display
struct StatusSnapshot: Equatable {
    let status: String          // "ok", "degraded", "error"
    let timestamp: Date
    let uptimeFormatted: String

    var timeAgo: String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .abbreviated
        return formatter.localizedString(for: timestamp, relativeTo: Date())
    }

    init(status: AdapterOSStatus, timestamp: Date = Date()) {
        self.status = status.status
        self.timestamp = timestamp
        self.uptimeFormatted = status.uptimeFormatted
    }
}

/// Service health trend information for displaying improvement/deterioration indicators
struct ServiceHealthTrend {
    let serviceId: String
    let previousHealth: String
    let currentHealth: String
    let changeTime: Date

    var trendIcon: String {
        switch (previousHealth, currentHealth) {
        case ("healthy", "unhealthy"): return "⬇️"
        case ("unhealthy", "healthy"): return "⬆️"
        case ("checking", "healthy"): return "⬆️"
        case ("healthy", "checking"): return "⬇️"
        case ("unhealthy", "checking"): return "⬆️"
        case ("checking", "unhealthy"): return "⬇️"
        default: return ""
        }
    }

    var trendDescription: String {
        switch (previousHealth, currentHealth) {
        case ("healthy", "unhealthy"): return "Health deteriorated"
        case ("unhealthy", "healthy"): return "Health recovered"
        case ("checking", "healthy"): return "Health stabilized"
        case ("healthy", "checking"): return "Health checking"
        case ("unhealthy", "checking"): return "Health improving"
        case ("checking", "unhealthy"): return "Health worsening"
        default: return "No change"
        }
    }

    var accessibilityLabel: String {
        "\(serviceId): \(trendDescription)"
    }
}

// MARK: - Menu View State Models

struct AppStatusViewState: Equatable {
    enum Health: String {
        case ok
        case degraded
        case error
    }

    let health: Health
    let headline: String
    let uptimeText: String
    let deterministic: Bool
    let telemetryLabel: String
    let kernelHashShort: String
    let kernelHashFull: String
    let metricsSummary: String?
    let lastUpdated: Date?
    let baseModelName: String?
    let baseModelLoaded: Bool
    let baseModelStatus: String?
}

extension AppStatusViewState {
    init(status: AdapterOSStatus, metrics: SystemMetrics?, lastUpdated: Date?) {
        let health = Health(rawValue: status.status) ?? .degraded

        let metricsSummary: String?
        if let metrics {
            metricsSummary = "CPU \(Int(metrics.cpuUsage))% • Mem \(String(format: "%.1f", metrics.memoryUsedGB))/\(String(format: "%.1f", metrics.memoryTotalGB)) GB"
        } else {
            metricsSummary = nil
        }

        self.init(
            health: health,
            headline: health == .ok ? "Operational" : health == .degraded ? "Degraded" : "Attention Required",
            uptimeText: status.uptimeFormatted,
            deterministic: status.deterministic,
            telemetryLabel: status.telemetry_mode,
            kernelHashShort: status.kernelHashShort,
            kernelHashFull: status.kernel_hash,
            metricsSummary: metricsSummary,
            lastUpdated: lastUpdated,
            baseModelName: status.base_model_name,
            baseModelLoaded: status.base_model_loaded ?? false,
            baseModelStatus: status.base_model_status
        )
    }
}

struct TenantQuickAction: Identifiable {
    let id: String
    let label: String
    let systemImage: String
    let isDestructive: Bool
    let requiresVerifiedPermissions: Bool
    let accessibilityLabel: String
    let testID: String
    let action: () -> Void

    init(
        id: String = UUID().uuidString,
        label: String,
        systemImage: String,
        isDestructive: Bool = false,
        requiresVerifiedPermissions: Bool = false,
        accessibilityLabel: String,
        testID: String,
        action: @escaping () -> Void
    ) {
        self.id = id
        self.label = label
        self.systemImage = systemImage
        self.isDestructive = isDestructive
        self.requiresVerifiedPermissions = requiresVerifiedPermissions
        self.accessibilityLabel = accessibilityLabel
        self.testID = testID
        self.action = action
    }
}

struct TenantViewState: Identifiable {
    struct Badge: Equatable {
        enum Style {
            case ok
            case warning
            case error
        }

        let text: String
        let style: Style
    }

    let id: String
    let displayName: String
    let subtitle: String?
    let permissionsVerified: Bool
    let badge: Badge?
    let quickActions: [TenantQuickAction]
    let testID: String
}

struct ActiveOperationViewState: Identifiable {
    let id: String
    let title: String
    let detail: String?
    let startedAt: Date
    let progress: Double?
    let supportsCancellation: Bool
    let cancelAction: (() -> Void)?
    let testID: String

    func elapsedText(referenceDate: Date = .init()) -> String {
        let elapsed = referenceDate.timeIntervalSince(startedAt)
        guard elapsed > 0 else { return "Just now" }
        let minutes = Int(elapsed) / 60
        let seconds = Int(elapsed) % 60
        if minutes > 0 {
            return "\(minutes)m \(seconds)s"
        }
        return "\(seconds)s"
    }
}

enum TrustStateViewState: Equatable {
    struct Signature: Equatable {
        let issuer: String
        let verifiedAt: Date?
    }

    case pending
    case signed(Signature)
    case unsigned
    case failed(reason: String)

    var isFailure: Bool {
        if case .failed = self { return true }
        return false
    }
}

struct CommandToast: Identifiable, Equatable {
    enum Kind {
        case success
        case error
        case info
    }

    let id = UUID()
    let message: String
    let kind: Kind
}

struct AccessibilityPreferences: Equatable {
    var textScale: CGFloat
    var reduceMotion: Bool
    var highContrast: Bool

    init(textScale: CGFloat = 1.0, reduceMotion: Bool = false, highContrast: Bool = false) {
        self.textScale = textScale
        self.reduceMotion = reduceMotion
        self.highContrast = highContrast
    }
}

#if DEBUG
extension AppStatusViewState {
    static var sampleOk: AppStatusViewState {
        AppStatusViewState(
            health: .ok,
            headline: "Operational",
            uptimeText: "6h 42m",
            deterministic: true,
            telemetryLabel: "policy",
            kernelHashShort: "abc123ef",
            kernelHashFull: "abc123ef4567890fedcba9876543210abc123ef4567890fedcba9876543210",
            metricsSummary: "CPU 22% • Mem 12.3/32.0 GB",
            lastUpdated: Date(),
            baseModelName: "Llama 3 8B",
            baseModelLoaded: true,
            baseModelStatus: "ready"
        )
    }

    static var sampleDegraded: AppStatusViewState {
        AppStatusViewState(
            health: .degraded,
            headline: "Degraded",
            uptimeText: "18m",
            deterministic: false,
            telemetryLabel: "degraded",
            kernelHashShort: "deadbeef",
            kernelHashFull: String(repeating: "deadbeef", count: 8),
            metricsSummary: "CPU 89% • Mem 29.1/32.0 GB",
            lastUpdated: Date(),
            baseModelName: "Falcon",
            baseModelLoaded: true,
            baseModelStatus: "loading"
        )
    }
}

extension TenantViewState {
    static var sample: TenantViewState {
        TenantViewState(
            id: "tenant-1",
            displayName: "Acme",
            subtitle: "Active adapters: 3",
            permissionsVerified: true,
            badge: Badge(text: "Primary", style: .ok),
            quickActions: [
                TenantQuickAction(
                    label: "Open",
                    systemImage: "arrow.turn.up.right",
                    accessibilityLabel: "Open tenant tools",
                    testID: "tenant-acme-action-open",
                    action: {}
                ),
                TenantQuickAction(
                    label: "Pause",
                    systemImage: "pause.circle",
                    isDestructive: false,
                    requiresVerifiedPermissions: true,
                    accessibilityLabel: "Pause tenant operations",
                    testID: "tenant-acme-action-pause",
                    action: {}
                )
            ],
            testID: "tenant-acme"
        )
    }
}

extension ActiveOperationViewState {
    static var sample: ActiveOperationViewState {
        ActiveOperationViewState(
            id: "op-1",
            title: "Loading base model",
            detail: "Tenant Acme",
            startedAt: Date().addingTimeInterval(-95),
            progress: 0.42,
            supportsCancellation: true,
            cancelAction: {},
            testID: "op-load-base"
        )
    }
}

extension Array where Element == TenantViewState {
    static var samples: [TenantViewState] {
        [
            .sample,
            TenantViewState(
                id: "tenant-2",
                displayName: "Globex",
                subtitle: "Pending verification",
                permissionsVerified: false,
                badge: TenantViewState.Badge(text: "Requires approval", style: .warning),
                quickActions: [
                    TenantQuickAction(
                        label: "Open",
                        systemImage: "arrow.turn.up.right",
                        accessibilityLabel: "Open tenant tools",
                        testID: "tenant-globex-action-open",
                        action: {}
                    )
                ],
                testID: "tenant-globex"
            )
        ]
    }
}
#endif




