//
//  adapterOSInstallerApp.swift
//  adapterOSInstaller
//
//  SwiftUI app entry point for adapterOS graphical installer
//

import SwiftUI

@main
struct adapterOSInstallerApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .frame(minWidth: 700, minHeight: 600)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
    }
}

