import SwiftUI

struct StatusRow: View {
    let label: String
    let value: String
    let systemImage: String?
    let color: Color?

    init(label: String, value: String, systemImage: String? = nil, color: Color? = nil) {
        self.label = label
        self.value = value
        self.systemImage = systemImage
        self.color = color
    }

    var body: some View {
        HStack(spacing: 8) {
            if let systemImage {
                Image(systemName: systemImage)
                    .foregroundColor(color ?? .secondary)
            }
            Text(label)
                .foregroundColor(.secondary)
            Spacer()
            Text(value)
                .fontWeight(.medium)
        }
        .font(DesignTokens.metricsFont)
    }
}


