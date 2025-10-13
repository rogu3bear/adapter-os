//
//  CompletionView.swift
//  AdapterOSInstaller
//
//  Success screen with determinism explanation and next steps
//

import SwiftUI
import AppKit

struct CompletionView: View {
    @State private var savedSuccessfully = false
    
    var body: some View {
        ScrollView {
            VStack(spacing: 24) {
                // Success indicator
                VStack(spacing: 16) {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 64))
                        .foregroundColor(.green)
                        .padding(.top, 20)
                    
                    Text("Installation Complete!")
                        .font(.title)
                        .fontWeight(.bold)
                    
                    Text("AdapterOS has been successfully installed")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
                
                Divider()
                    .padding(.vertical, 8)
                
                // Determinism Explainer
                VStack(alignment: .leading, spacing: 16) {
                    HStack {
                        Image(systemName: "lock.shield.fill")
                            .foregroundColor(.blue)
                            .font(.title2)
                        Text("What is Determinism?")
                            .font(.title3)
                            .fontWeight(.semibold)
                    }
                    
                    Text("AdapterOS runs bit-reproducible AI workloads. Every inference is cryptographically verified.")
                        .font(.body)
                    
                    // What This Means
                    VStack(alignment: .leading, spacing: 12) {
                        ExplainerSection(
                            icon: "equal.circle.fill",
                            color: .purple,
                            title: "Reproducible Results",
                            description: "Same input + same model = identical output, every time"
                        )
                        
                        ExplainerSection(
                            icon: "list.bullet.clipboard.fill",
                            color: .orange,
                            title: "Full Audit Trail",
                            description: "Complete record of every inference decision"
                        )
                        
                        ExplainerSection(
                            icon: "checkmark.seal.fill",
                            color: .green,
                            title: "Cryptographic Proof",
                            description: "Mathematical verification of computation integrity"
                        )
                    }
                    
                    // How It Works
                    Text("How It Works")
                        .font(.headline)
                        .padding(.top, 8)
                    
                    VStack(alignment: .leading, spacing: 8) {
                        BulletPoint(text: "All kernels compiled to deterministic Metal bytecode")
                        BulletPoint(text: "RNG seeded from HKDF derivation")
                        BulletPoint(text: "Floating-point modes locked at kernel launch")
                        BulletPoint(text: "Event hashes form Merkle tree for verification")
                    }
                }
                .padding()
                .background(Color(NSColor.controlBackgroundColor))
                .cornerRadius(12)
                
                // Next Steps
                VStack(alignment: .leading, spacing: 16) {
                    Text("Next Steps")
                        .font(.title3)
                        .fontWeight(.semibold)
                    
                    VStack(alignment: .leading, spacing: 12) {
                        NextStepCard(
                            number: 1,
                            title: "Start the Control Plane",
                            command: "./target/release/aos-cp --config configs/cp.toml"
                        )
                        
                        NextStepCard(
                            number: 2,
                            title: "Run Your First Inference",
                            command: "cargo run --bin aosctl serve --tenant default --plan qwen7b --socket /var/run/aos/default/aos.sock"
                        )
                        
                        NextStepCard(
                            number: 3,
                            title: "Learn More",
                            command: "See docs/architecture.md for detailed documentation"
                        )
                    }
                }
                .padding()
                .background(Color(NSColor.controlBackgroundColor))
                .cornerRadius(12)
                
                // Action buttons
                HStack(spacing: 12) {
                    Button(action: openTerminal) {
                        Label("Open Terminal", systemImage: "terminal")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)
                    
                    Button(action: copyNextSteps) {
                        Label("Copy Commands", systemImage: "doc.on.doc")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)
                }
                
                Button(action: { NSApplication.shared.terminate(nil) }) {
                    Text("Done")
                        .font(.headline)
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .padding(.bottom, 20)
            }
            .padding()
        }
        .onAppear {
            savedSuccessfully = DeterminismExplainer.saveToFile()
        }
    }
    
    private func openTerminal() {
        let workspace = "/Users/star/Dev/adapter-os"
        let script = """
        tell application "Terminal"
            do script "cd \(workspace)"
            activate
        end tell
        """
        
        if let appleScript = NSAppleScript(source: script) {
            var error: NSDictionary?
            appleScript.executeAndReturnError(&error)
            if let error = error {
                print("Error opening terminal: \(error)")
            }
        }
    }
    
    private func copyNextSteps() {
        let commands = """
        # Start the Control Plane
        ./target/release/aos-cp --config configs/cp.toml
        
        # Run Your First Inference
        cargo run --bin aosctl serve --tenant default --plan qwen7b --socket /var/run/aos/default/aos.sock
        """
        
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(commands, forType: .string)
    }
}

struct ExplainerSection: View {
    let icon: String
    let color: Color
    let title: String
    let description: String
    
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon)
                .foregroundColor(color)
                .font(.title3)
            
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.subheadline)
                    .fontWeight(.semibold)
                Text(description)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
    }
}

struct BulletPoint: View {
    let text: String
    
    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Text("•")
                .font(.caption)
                .foregroundColor(.secondary)
            Text(text)
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }
}

struct NextStepCard: View {
    let number: Int
    let title: String
    let command: String
    
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Text("\(number)")
                .font(.title3)
                .fontWeight(.bold)
                .foregroundColor(.white)
                .frame(width: 32, height: 32)
                .background(Color.blue)
                .clipShape(Circle())
            
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.subheadline)
                    .fontWeight(.medium)
                Text(command)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundColor(.secondary)
                    .textSelection(.enabled)
            }
        }
        .padding(12)
        .background(Color(NSColor.textBackgroundColor))
        .cornerRadius(8)
    }
}

