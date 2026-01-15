import Foundation
import UserNotifications

/// Manages macOS notifications for adapterOS status events
final class NotificationManager {
    static let shared = NotificationManager()

    private let notificationCenter = UNUserNotificationCenter.current()
    private let logger = Logger.shared
    private let userDefaults = UserDefaults.standard

    // Keys for user preferences
    private let notifyCriticalFailuresKey = "notifyCriticalFailures"
    private let notifyRecoveriesKey = "notifyRecoveries"
    private let notifyTrustIssuesKey = "notifyTrustIssues"

    // User preferences for notification types (with defaults)
    private var notifyCriticalFailures: Bool {
        get { userDefaults.bool(forKey: notifyCriticalFailuresKey) }
        set { userDefaults.set(newValue, forKey: notifyCriticalFailuresKey) }
    }

    private var notifyRecoveries: Bool {
        get { userDefaults.bool(forKey: notifyRecoveriesKey) }
        set { userDefaults.set(newValue, forKey: notifyRecoveriesKey) }
    }

    private var notifyTrustIssues: Bool {
        get { userDefaults.bool(forKey: notifyTrustIssuesKey) }
        set { userDefaults.set(newValue, forKey: notifyTrustIssuesKey) }
    }

    private init() {
        // Set default values if not already set
        if !userDefaults.bool(forKey: notifyCriticalFailuresKey + "_set") {
            userDefaults.set(true, forKey: notifyCriticalFailuresKey)
            userDefaults.set(true, forKey: notifyCriticalFailuresKey + "_set")
        }
        if !userDefaults.bool(forKey: notifyRecoveriesKey + "_set") {
            userDefaults.set(false, forKey: notifyRecoveriesKey)
            userDefaults.set(true, forKey: notifyRecoveriesKey + "_set")
        }
        if !userDefaults.bool(forKey: notifyTrustIssuesKey + "_set") {
            userDefaults.set(true, forKey: notifyTrustIssuesKey)
            userDefaults.set(true, forKey: notifyTrustIssuesKey + "_set")
        }

        requestAuthorization()
    }

    /// Request notification permissions from the user
    private func requestAuthorization() {
        notificationCenter.requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
            if let error = error {
                self.logger.error("Failed to request notification permission", error: error)
            } else if granted {
                self.logger.info("Notification permission granted")
            } else {
                self.logger.info("Notification permission denied")
            }
        }
    }

    /// Notify about critical service failures
    /// - Parameter service: The service that failed
    func notifyCriticalFailure(_ service: ServiceStatus) {
        guard notifyCriticalFailures else { return }

        // Only notify for user-facing services (not internal services)
        guard isUserFacingService(service) else { return }

        let content = UNMutableNotificationContent()
        content.title = "adapterOS Service Failed"
        content.body = "\(service.name) has failed. Check service logs for details."
        content.sound = .default
        content.categoryIdentifier = "SERVICE_FAILURE"

        let request = UNNotificationRequest(identifier: "service-\(service.id)-failure-\(Date().timeIntervalSince1970)",
                                          content: content,
                                          trigger: nil)

        notificationCenter.add(request) { error in
            if let error = error {
                self.logger.error("Failed to schedule failure notification", error: error)
            }
        }

        logger.info("Sent critical failure notification", context: ["service": service.name])
    }

    /// Notify about service recovery
    /// - Parameter service: The service that recovered
    func notifyRecovery(_ service: ServiceStatus) {
        guard notifyRecoveries else { return }
        guard isUserFacingService(service) else { return }

        let content = UNMutableNotificationContent()
        content.title = "adapterOS Service Recovered"
        content.body = "\(service.name) is now running normally."
        content.sound = .default
        content.categoryIdentifier = "SERVICE_RECOVERY"

        let request = UNNotificationRequest(identifier: "service-\(service.id)-recovery-\(Date().timeIntervalSince1970)",
                                          content: content,
                                          trigger: nil)

        notificationCenter.add(request) { error in
            if let error = error {
                self.logger.error("Failed to schedule recovery notification", error: error)
            }
        }

        logger.info("Sent recovery notification", context: ["service": service.name])
    }

    /// Notify about trust verification issues
    /// - Parameter reason: Description of the trust issue
    func notifyTrustIssue(_ reason: String) {
        guard notifyTrustIssues else { return }

        let content = UNMutableNotificationContent()
        content.title = "adapterOS Trust Issue"
        content.body = "Trust verification failed: \(reason)"
        content.sound = .default
        content.categoryIdentifier = "TRUST_ISSUE"

        let request = UNNotificationRequest(identifier: "trust-issue-\(Date().timeIntervalSince1970)",
                                          content: content,
                                          trigger: nil)

        notificationCenter.add(request) { error in
            if let error = error {
                self.logger.error("Failed to schedule trust notification", error: error)
            }
        }

        logger.info("Sent trust issue notification", context: ["reason": reason])
    }

    /// Check if a service is user-facing and worth notifying about
    private func isUserFacingService(_ service: ServiceStatus) -> Bool {
        // Define which services are important enough to notify users about
        let importantServices = ["web-api", "database", "worker", "scheduler", "auth"]

        // Check if service name contains any important keywords
        return importantServices.contains { service.name.lowercased().contains($0) } ||
               service.name.lowercased().hasPrefix("api") ||
               service.name.lowercased().hasPrefix("web")
    }

    /// Get current notification settings
    func getNotificationSettings() -> NotificationSettings {
        NotificationSettings(
            criticalFailures: notifyCriticalFailures,
            recoveries: notifyRecoveries,
            trustIssues: notifyTrustIssues
        )
    }

    /// Update notification settings
    func updateSettings(_ settings: NotificationSettings) {
        notifyCriticalFailures = settings.criticalFailures
        notifyRecoveries = settings.recoveries
        notifyTrustIssues = settings.trustIssues

        logger.info("Updated notification settings", context: [
            "failures": settings.criticalFailures,
            "recoveries": settings.recoveries,
            "trust": settings.trustIssues
        ])
    }
}

/// Notification settings structure
struct NotificationSettings {
    var criticalFailures: Bool
    var recoveries: Bool
    var trustIssues: Bool
}
