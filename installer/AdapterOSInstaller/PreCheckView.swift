//
//  PreCheckView.swift
//  AdapterOSInstaller
//
//  Hardware validation and configuration screen
//

import SwiftUI

struct PreCheckView: View {
    @ObservedObject var hardwareChecker: HardwareChecker
    @Binding var installMode: InstallMode
    @Binding var airGapped: Bool
    let onContinue: () -> Void
    
    @State private var checksCompleted = false
    
    var body: some View {
        VStack(spacing: 20) {
            // Title
            VStack(spacing: 8) {
                Image(systemName: "checkmark.shield")
                    .font(.system(size: 48))
                    .foregroundColor(.blue)
                Text("System Requirements")
                    .font(.title2)
                    .fontWeight(.semibold)
                Text("Validating your system meets AdapterOS requirements")
                    .font(.subheadline)
                    .foregroundColor(.secondary)
            }
            .padding(.top, 20)
            
            // Hardware checks
            VStack(alignment: .leading, spacing: 12) {
                if checksCompleted {
                    ForEach(hardwareChecker.checks, id: \.name) { check in
                        HardwareCheckRow(check: check)
                    }
                } else {
                    HStack {
                        ProgressView()
                            .scaleEffect(0.8)
                        Text("Running hardware checks...")
                            .foregroundColor(.secondary)
                    }
                }
            }
            .padding()
            .frame(maxWidth: 500)
            .background(Color(NSColor.controlBackgroundColor))
            .cornerRadius(8)
            
            Spacer()
            
            // Configuration
            VStack(alignment: .leading, spacing: 16) {
                Text("Installation Options")
                    .font(.headline)
                
                // Mode picker
                VStack(alignment: .leading, spacing: 8) {
                    Picker("Mode", selection: $installMode) {
                        Text(InstallMode.full.displayName).tag(InstallMode.full)
                        Text(InstallMode.minimal.displayName).tag(InstallMode.minimal)
                    }
                    .pickerStyle(.segmented)
                    
                    Text(installMode.description)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                
                // Air-gapped toggle
                Toggle(isOn: $airGapped) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Air-Gapped Mode")
                            .font(.subheadline)
                        Text("Skip all network operations (you'll need to manually import models)")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
            }
            .padding()
            .frame(maxWidth: 500)
            .background(Color(NSColor.controlBackgroundColor))
            .cornerRadius(8)
            
            Spacer()
            
            // Continue button
            Button(action: onContinue) {
                Text("Continue")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .padding()
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(!checksCompleted || !hardwareChecker.allRequiredPass)
            .padding(.horizontal)
            .padding(.bottom, 20)
        }
        .onAppear {
            // Run checks after a brief delay for UI smoothness
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
                hardwareChecker.runChecks()
                checksCompleted = true
            }
        }
    }
}

struct HardwareCheckRow: View {
    let check: HardwareCheckResult
    
    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: check.passed ? "checkmark.circle.fill" : "xmark.circle.fill")
                .foregroundColor(check.passed ? .green : (check.isRequired ? .red : .orange))
                .font(.title3)
            
            VStack(alignment: .leading, spacing: 2) {
                Text(check.name)
                    .font(.subheadline)
                    .fontWeight(.medium)
                Text(check.message)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            
            Spacer()
            
            if !check.isRequired {
                Text("OPTIONAL")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Color.secondary.opacity(0.1))
                    .cornerRadius(4)
            }
        }
        .padding(.vertical, 4)
    }
}

