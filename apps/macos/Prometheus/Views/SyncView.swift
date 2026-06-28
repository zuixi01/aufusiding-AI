import SwiftUI

struct SyncView: View {
    @StateObject private var viewModel = SyncViewModel()

    var body: some View {
        Form {
            Section("同步状态") {
                LabeledContent("连接状态") {
                    HStack {
                        Circle().fill(viewModel.status.connected ? .green : .red).frame(width: 8, height: 8)
                        Text(viewModel.status.connected ? "已连接" : "未连接")
                    }
                }
                LabeledContent("最后同步") {
                    Text(viewModel.status.lastSync ?? "从未同步").foregroundStyle(.secondary)
                }
                LabeledContent("待同步操作") {
                    Text("\(viewModel.status.pendingOps)")
                        .foregroundStyle(viewModel.status.pendingOps > 0 ? .orange : .secondary)
                }
            }

            Section {
                Button(action: { viewModel.refresh() }) {
                    HStack {
                        if viewModel.isRefreshing { ProgressView().controlSize(.small) }
                        Text("刷新状态")
                    }
                }
                .disabled(viewModel.isRefreshing)
            }
        }
        .formStyle(.grouped)
        .onAppear { viewModel.refresh() }
    }
}

@MainActor
class SyncViewModel: ObservableObject {
    @Published var status = SyncStatus(lastSync: nil, pendingOps: 0, connected: false)
    @Published var isRefreshing = false
    private let engine = PrometheusEngine.shared

    func refresh() {
        isRefreshing = true
        do { status = try engine.getSyncStatus() } catch {}
        isRefreshing = false
    }
}
