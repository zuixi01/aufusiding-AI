import SwiftUI

@main
struct PrometheusApp: App {
    @StateObject private var chatViewModel = ChatViewModel()

    init() {
        do {
            try PrometheusEngine.shared.initialize()
        } catch {
            print("引擎初始化失败: \(error)")
        }
    }

    var body: some Scene {
        WindowGroup {
            MainView()
                .environmentObject(chatViewModel)
        }
        .windowStyle(.hiddenTitleBar)
        .windowToolbarStyle(.unified)

        Settings {
            SettingsView()
        }
    }
}
