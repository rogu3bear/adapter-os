import SwiftUI

struct ProblemsView: View {
    let error: StatusReadError?
    let retry: () -> Void
    let openLogs: () -> Void

    private var message: String {
        switch error {
        case .fileMissing:
            return "Status unavailable — server not running or no permission"
        case .decodeFailed:
            return "Status corrupt — expecting AdapterOSStatus JSON"
        case .permissionDenied:
            return "Permission denied — cannot read status file"
        default:
            return "Unknown error"
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "bolt.slash.circle.fill")
                    .foregroundColor(DesignTokens.errorColor)
                Text("AdapterOS Problem")
                    .font(DesignTokens.headerFont)
                    .foregroundColor(DesignTokens.errorColor)
            }
            Text(message)
                .font(.caption)
                .foregroundColor(.secondary)

            HStack(spacing: 12) {
                Button("Retry") { retry() }
                Button("Open Logs Directory") { openLogs() }
            }
        }
        .padding(12)
        .background(DesignTokens.surface)
        .cornerRadius(8)
    }
}


