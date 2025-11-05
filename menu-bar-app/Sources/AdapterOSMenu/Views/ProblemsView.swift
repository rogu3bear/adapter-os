import SwiftUI

struct ProblemsView: View {
    struct Content {
        let iconSystemName: String
        let iconColor: Color
        let title: String
        let message: String
        let primaryLabel: String
        let primaryAction: () -> Void
        let secondaryLabel: String?
        let secondaryAction: (() -> Void)?

        init(
            iconSystemName: String,
            iconColor: Color,
            title: String,
            message: String,
            primaryLabel: String,
            primaryAction: @escaping () -> Void,
            secondaryLabel: String? = nil,
            secondaryAction: (() -> Void)? = nil
        ) {
            self.iconSystemName = iconSystemName
            self.iconColor = iconColor
            self.title = title
            self.message = message
            self.primaryLabel = primaryLabel
            self.primaryAction = primaryAction
            self.secondaryLabel = secondaryLabel
            self.secondaryAction = secondaryAction
        }
    }

    private let content: Content

    init(content: Content) {
        self.content = content
    }

    init(error: StatusReadError?, retry: @escaping () -> Void, openLogs: @escaping () -> Void) {
        let message: String
        switch error {
        case .fileMissing:
            message = "Status unavailable — server not running or no permission"
        case .decodeFailed(let details):
            let reason = details.isEmpty ? "expecting AdapterOSStatus JSON" : details
            message = "Status corrupt — \(reason)"
        case .permissionDenied:
            message = "Permission denied — cannot read status file"
        case .readError(let text):
            message = text
        default:
            message = "Unknown error"
        }

        self.content = Content(
            iconSystemName: "bolt.slash.circle.fill",
            iconColor: DesignTokens.errorColor,
            title: "AdapterOS Problem",
            message: message,
            primaryLabel: "Retry",
            primaryAction: retry,
            secondaryLabel: "Open Logs Directory",
            secondaryAction: openLogs
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: content.iconSystemName)
                    .foregroundColor(content.iconColor)
                Text(content.title)
                    .font(DesignTokens.headerFont)
                    .foregroundColor(content.iconColor)
            }
            Text(content.message)
                .font(.caption)
                .foregroundColor(.secondary)

            HStack(spacing: 12) {
                Button(content.primaryLabel) { content.primaryAction() }
                if let secondaryLabel = content.secondaryLabel, let secondaryAction = content.secondaryAction {
                    Button(secondaryLabel) { secondaryAction() }
                }
            }
        }
        .padding(12)
        .background(DesignTokens.surface)
        .cornerRadius(8)
    }
}


