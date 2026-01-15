//
//  InstallView.swift
//  adapterOSInstaller
//
//  Installation progress and log streaming
//

import SwiftUI

struct InstallView: View {
    @ObservedObject var processRunner: ProcessRunner
    let installMode: InstallMode
    let airGapped: Bool
    let onComplete: () -> Void
    
    @State private var showCancelAlert = false
    @StateObject private var checkpointManager = CheckpointManager()
    @State private var showResumeInfo = false
    
    var body: some View {
        VStack(spacing: 20) {
            // Title
            VStack(spacing: 8) {
                if case .running = processRunner.status {
                    ProgressView()
                        .scaleEffect(1.2)
                        .padding(.bottom, 8)
                }
                
                Text(currentStatusText)
                    .font(.title2)
                    .fontWeight(.semibold)
                
                Text(processRunner.currentStep.displayName)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
            }
            .padding(.top, 20)
            
            // Resume info if checkpoint exists
            if showResumeInfo, let info = checkpointManager.getCheckpointInfo() {
                HStack {
                    Image(systemName: "arrow.clockwise.circle.fill")
                        .foregroundColor(.orange)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Resuming from previous installation")
                            .font(.subheadline)
                            .fontWeight(.medium)
                        Text("Last completed: \(checkpointManager.displayNameForStep(info.step))")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                    Spacer()
                }
                .padding()
                .background(Color.orange.opacity(0.1))
                .cornerRadius(8)
                .padding(.horizontal)
            }
            
            // Progress bar
            VStack(alignment: .leading, spacing: 8) {
                ProgressView(value: processRunner.progress, total: 1.0)
                    .progressViewStyle(.linear)
                
                HStack {
                    Text("\(Int(processRunner.progress * 100))%")
                        .font(.caption)
                        .foregroundColor(.secondary)
                    Spacer()
                    if case .running = processRunner.status {
                        Text("Installing...")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
            .padding(.horizontal)
            
            // Log output
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(Array(processRunner.logs.enumerated()), id: \.offset) { index, log in
                            Text(log)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundColor(logColor(for: log))
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .id(index)
                        }
                    }
                    .padding(8)
                }
                .background(Color(NSColor.textBackgroundColor))
                .cornerRadius(8)
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(Color.secondary.opacity(0.2), lineWidth: 1)
                )
                .padding(.horizontal)
                .onChange(of: processRunner.logs.count) { _ in
                    // Auto-scroll to bottom
                    if let lastIndex = processRunner.logs.indices.last {
                        withAnimation {
                            proxy.scrollTo(lastIndex, anchor: .bottom)
                        }
                    }
                }
            }
            
            Spacer()
            
            // Action buttons
            HStack {
                if case .running = processRunner.status {
                    Button("Cancel") {
                        showCancelAlert = true
                    }
                    .buttonStyle(.bordered)
                } else if case .failed = processRunner.status {
                    Button("Retry") {
                        Task {
                            try? await processRunner.runBootstrap(mode: installMode, airGapped: airGapped)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                }
                
                Spacer()
            }
            .padding(.horizontal)
            .padding(.bottom, 20)
        }
        .alert("Cancel Installation?", isPresented: $showCancelAlert) {
            Button("No", role: .cancel) {}
            Button("Yes", role: .destructive) {
                processRunner.cancel()
            }
        } message: {
            Text("The installation is incomplete. You can resume it later from where it left off.")
        }
        .onAppear {
            // Check for existing checkpoint
            showResumeInfo = checkpointManager.hasCheckpoint()
            
            // Start installation
            Task {
                do {
                    try await processRunner.runBootstrap(mode: installMode, airGapped: airGapped)
                    // Wait a moment before transitioning
                    try await Task.sleep(nanoseconds: 1_000_000_000)
                    onComplete()
                } catch {
                    print("Installation failed: \(error)")
                }
            }
        }
    }
    
    private var currentStatusText: String {
        switch processRunner.status {
        case .notStarted:
            return "Preparing Installation"
        case .running:
            return "Installing adapterOS"
        case .completed:
            return "Installation Complete"
        case .failed:
            return "Installation Failed"
        }
    }
    
    private func logColor(for log: String) -> Color {
        if log.contains("ERROR") || log.contains("✗") || log.contains("Failed") {
            return .red
        } else if log.contains("WARNING") || log.contains("⚠") {
            return .orange
        } else if log.contains("✓") || log.contains("Complete") {
            return .green
        } else {
            return .primary
        }
    }
}

