import SwiftUI

@main
struct AdapterOSMenuApp: App {
    @StateObject private var viewModel = StatusViewModel()
    
    var body: some Scene {
        MenuBarExtra("AdapterOS", systemImage: viewModel.iconName) {
            MenuContent(viewModel: viewModel)
        }
        .menuBarExtraStyle(.window)
    }
}

struct MenuContent: View {
    @ObservedObject var viewModel: StatusViewModel
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if viewModel.isOffline {
                offlineView
            } else {
                statusView
            }
            
            Divider()
                .padding(.vertical, 4)
            
            Button("View Logs") {
                viewModel.openLogs()
            }
            .keyboardShortcut("l", modifiers: .command)
        }
        .padding(12)
        .frame(minWidth: 280)
    }
    
    // MARK: - Offline View
    
    private var offlineView: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Image(systemName: "bolt.slash")
                    .foregroundColor(.red)
                Text("AdapterOS OFFLINE")
                    .font(.headline)
                    .foregroundColor(.red)
            }
            
            Text("Control plane not running")
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }
    
    // MARK: - Status View
    
    private var statusView: some View {
        VStack(alignment: .leading, spacing: 10) {
            // Status header
            HStack {
                statusIndicator
                
                Spacer()
                
                if let status = viewModel.status {
                    Text(status.deterministic ? "✅" : "⚠️")
                        .font(.title3)
                        .help(status.deterministic ? "Deterministic mode enabled" : "Deterministic mode disabled")
                }
            }
            
            // Adapter & worker info
            if let status = viewModel.status {
                HStack(spacing: 20) {
                    InfoItem(label: "Adapters", value: "\(status.adapters_loaded)")
                    InfoItem(label: "Workers", value: "\(status.worker_count)")
                }
                .font(.system(.body, design: .monospaced))
            }
            
            // System metrics
            if let metrics = viewModel.metrics {
                VStack(alignment: .leading, spacing: 4) {
                    MetricRow(
                        label: "CPU",
                        value: String(format: "%.0f%%", metrics.cpuUsage),
                        level: metrics.cpuUsage
                    )
                    
                    MetricRow(
                        label: "GPU",
                        value: String(format: "%.0f%%", metrics.gpuUsage),
                        level: metrics.gpuUsage
                    )
                    
                    MetricRow(
                        label: "RAM",
                        value: String(format: "%.0f GB", metrics.memoryUsedGB),
                        level: metrics.memoryPercent
                    )
                }
                .font(.system(.callout, design: .monospaced))
            }
            
            // Uptime
            if let status = viewModel.status {
                HStack {
                    Text("Uptime:")
                        .foregroundColor(.secondary)
                    Text(status.uptimeFormatted)
                        .font(.system(.body, design: .monospaced))
                }
                .font(.caption)
            }
        }
    }
    
    // MARK: - Status Indicator
    
    private var statusIndicator: some View {
        HStack(spacing: 6) {
            if let status = viewModel.status {
                Circle()
                    .fill(statusColor(status.status))
                    .frame(width: 8, height: 8)
                
                Text("AdapterOS")
                    .font(.headline)
                
                Text(status.status.uppercased())
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
    }
    
    private func statusColor(_ status: String) -> Color {
        switch status {
        case "ok":
            return .green
        case "degraded":
            return .yellow
        case "error":
            return .red
        default:
            return .gray
        }
    }
}

// MARK: - Helper Views

struct InfoItem: View {
    let label: String
    let value: String
    
    var body: some View {
        HStack(spacing: 4) {
            Text("\(label):")
                .foregroundColor(.secondary)
            Text(value)
                .fontWeight(.medium)
        }
    }
}

struct MetricRow: View {
    let label: String
    let value: String
    let level: Double
    
    var body: some View {
        HStack {
            Text(label)
                .frame(width: 40, alignment: .leading)
                .foregroundColor(.secondary)
            
            Text(value)
                .frame(width: 60, alignment: .trailing)
                .fontWeight(.medium)
                .foregroundColor(levelColor)
            
            ProgressView(value: min(level, 100.0), total: 100.0)
                .tint(levelColor)
                .frame(width: 80)
        }
    }
    
    private var levelColor: Color {
        if level > 80 {
            return .red
        } else if level > 60 {
            return .yellow
        } else {
            return .green
        }
    }
}

// MARK: - Preview

#Preview {
    MenuContent(viewModel: {
        let vm = StatusViewModel()
        vm.status = AdapterOSStatus(
            status: "ok",
            uptime_secs: 13320,
            adapters_loaded: 3,
            deterministic: true,
            kernel_hash: "a84d9f1c",
            telemetry_mode: "local",
            worker_count: 2
        )
        vm.metrics = SystemMetrics(
            cpuUsage: 45.0,
            gpuUsage: 62.0,
            memoryUsedGB: 18.2,
            memoryTotalGB: 32.0
        )
        vm.isOffline = false
        return vm
    }())
}




