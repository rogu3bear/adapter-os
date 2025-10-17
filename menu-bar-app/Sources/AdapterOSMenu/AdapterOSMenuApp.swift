import SwiftUI

@main
struct AdapterOSMenuApp: App {
    @StateObject private var viewModel = StatusViewModel()
    
    var body: some Scene {
        MenuBarExtra("AdapterOS", systemImage: viewModel.iconName) {
            StatusMenuView(viewModel: viewModel)
        }
        .menuBarExtraStyle(.window)
    }
}
}




