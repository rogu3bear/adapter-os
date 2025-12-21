//
//  AdapterOSInstallerApp.swift
//  AdapterOSInstaller
//
//  SwiftUI app entry point for AdapterOS graphical installer
//

import SwiftUI

@main
struct AdapterOSInstallerApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .frame(minWidth: 700, minHeight: 600)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
    }
}

