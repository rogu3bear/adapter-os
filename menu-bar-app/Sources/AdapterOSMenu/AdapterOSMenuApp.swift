import SwiftUI

@main
struct AdapterOSMenuApp: App {
    @StateObject private var viewModel = StatusViewModel()
    @State private var showTooltip = false

    var body: some Scene {
<<<<<<< HEAD
        MenuBarExtra {
            StatusMenuView(viewModel: viewModel)
        } label: {
            HStack(spacing: 4) {
                Image(systemName: viewModel.iconName)
                    .foregroundColor(iconColor)
                if let status = viewModel.appStatus {
                    Text(status.headline.prefix(1)) // First letter of status
                        .font(.caption2)
                        .foregroundColor(iconColor)
                }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .background(
                RoundedRectangle(cornerRadius: 4)
                    .fill(Color.clear)
                    .contentShape(Rectangle())
            )
            .onHover { hovering in
                showTooltip = hovering
            }
            .popover(isPresented: $showTooltip, arrowEdge: .bottom) {
                StatusHistoryTooltip(recentStatuses: viewModel.recentStatusSnapshots)
                    .padding(.top, 8)
            }
        }
        .menuBarExtraStyle(.window)
    }

    private var iconColor: Color {
        guard let status = viewModel.appStatus?.health else { return .gray }
        switch status {
        case .ok: return .green
        case .degraded: return .yellow
        case .error: return .red
        }
    }
}

=======
        MenuBarExtra("AdapterOS", systemImage: viewModel.iconName) {
            StatusMenuView(viewModel: viewModel)
        }
        .menuBarExtraStyle(.window)
    }
}
}


>>>>>>> integration-branch


