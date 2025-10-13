//
//  ContentView.swift
//  AdapterOSInstaller
//
//  Main navigation and screen management
//

import SwiftUI

enum InstallationScreen {
    case preCheck
    case installing
    case completion
}

struct ContentView: View {
    @State private var currentScreen: InstallationScreen = .preCheck
    @State private var installMode: InstallMode = .full
    @State private var airGapped: Bool = false
    
    @StateObject private var hardwareChecker = HardwareChecker()
    @StateObject private var processRunner = ProcessRunner()
    
    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Image(systemName: "cpu")
                    .font(.system(size: 32))
                    .foregroundColor(.blue)
                Text("AdapterOS Installer")
                    .font(.title)
                    .fontWeight(.semibold)
                Spacer()
            }
            .padding()
            .background(Color(NSColor.windowBackgroundColor))
            
            Divider()
            
            // Content
            Group {
                switch currentScreen {
                case .preCheck:
                    PreCheckView(
                        hardwareChecker: hardwareChecker,
                        installMode: $installMode,
                        airGapped: $airGapped,
                        onContinue: {
                            currentScreen = .installing
                        }
                    )
                case .installing:
                    InstallView(
                        processRunner: processRunner,
                        installMode: installMode,
                        airGapped: airGapped,
                        onComplete: {
                            currentScreen = .completion
                        }
                    )
                case .completion:
                    CompletionView()
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

