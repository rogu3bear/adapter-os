import SwiftUI

struct StatusMenuView: View {
    @ObservedObject var viewModel: StatusViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
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
        .padding(12)
        .frame(minWidth: 300)
    }

    private var header: some View {
        HStack(spacing: 8) {
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
        VStack(alignment: .leading, spacing: 8) {
            // Chips line
            HStack(spacing: 8) {
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
            HStack(spacing: 8) {
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
        }
    }

    private var actionsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                if let url = URL(string: "http://localhost:3200") { NSWorkspace.shared.open(url) }
            } label: {
                Label("Open Dashboard", systemImage: "safari")
            }

            if let status = viewModel.status,
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

            #if DEBUG
            Divider()
            Text("Debug").font(.caption).foregroundColor(.secondary)
            Button("Load Sample OK JSON") {
                // no-op placeholder for UI testing; real samples can be added later
            }
            Button("Load Sample Degraded JSON") {
                // no-op placeholder for UI testing; real samples can be added later
            }
            Button("Simulate Missing File") {
                // no-op placeholder for UI testing; real samples can be added later
            }
            #endif

            Button {
                if let status = viewModel.status, let data = try? JSONEncoder().encode(status), let str = String(data: data, encoding: .utf8) {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(str, forType: .string)
                }
            } label: {
                Label("Copy Status JSON", systemImage: "doc.on.doc")
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


