import SwiftUI

enum AppTab: String, CaseIterable {
    case chat = "聊天"
    case iot = "IoT"
    case memory = "记忆"
    case settings = "设置"

    var icon: String {
        switch self {
        case .chat: return "bubble.left.and.bubble.right.fill"
        case .iot: return "antenna.radiowaves.left.and.right"
        case .memory: return "brain.head.profile"
        case .settings: return "gearshape.fill"
        }
    }
}

struct MainView: View {
    @StateObject private var chatViewModel = ChatViewModel()
    @State private var selectedTab: AppTab = .chat

    var body: some View {
        TabView(selection: $selectedTab) {
            NavigationStack {
                ChatView(viewModel: chatViewModel)
                    .navigationTitle("灵犀")
                    .toolbar {
                        ToolbarItem(placement: .topBarTrailing) {
                            Button(action: chatViewModel.clearConversation) {
                                Image(systemName: "plus")
                            }
                        }
                    }
            }
            .tabItem {
                Label(AppTab.chat.rawValue, systemImage: AppTab.chat.icon)
            }
            .tag(AppTab.chat)

            NavigationStack {
                IoTView()
            }
            .tabItem {
                Label(AppTab.iot.rawValue, systemImage: AppTab.iot.icon)
            }
            .tag(AppTab.iot)

            NavigationStack {
                MemoryView()
            }
            .tabItem {
                Label(AppTab.memory.rawValue, systemImage: AppTab.memory.icon)
            }
            .tag(AppTab.memory)

            NavigationStack {
                SettingsView()
            }
            .tabItem {
                Label(AppTab.settings.rawValue, systemImage: AppTab.settings.icon)
            }
            .tag(AppTab.settings)
        }
    }
}

#Preview {
    MainView()
}
