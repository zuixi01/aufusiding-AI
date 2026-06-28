import SwiftUI

enum SidebarItem: String, CaseIterable, Identifiable {
    case chat = "聊天"
    case iot = "IoT"
    case memory = "记忆"
    case settings = "设置"

    var id: String { rawValue }

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
    @State private var selectedItem: SidebarItem = .chat

    var body: some View {
        NavigationSplitView {
            List(SidebarItem.allCases, selection: $selectedItem) { item in
                Label(item.rawValue, systemImage: item.icon)
                    .tag(item)
            }
            .navigationSplitViewColumnWidth(min: 180, ideal: 200)
            .navigationTitle("灵犀")
        } detail: {
            switch selectedItem {
            case .chat:
                ChatView(viewModel: chatViewModel)
                    .navigationTitle("聊天")
                    .toolbar {
                        ToolbarItem {
                            Button(action: chatViewModel.clearConversation) {
                                Image(systemName: "plus")
                            }
                            .help("新对话")
                        }
                    }
            case .iot:
                IoTView()
                    .navigationTitle("IoT 设备")
            case .memory:
                MemoryView()
                    .navigationTitle("记忆")
            case .settings:
                SettingsView()
                    .navigationTitle("设置")
            }
        }
        .frame(minWidth: 800, minHeight: 600)
    }
}

#Preview {
    MainView()
}
