import SwiftUI

struct MemoryView: View {
    @StateObject private var viewModel = MemoryViewModel()

    var body: some View {
        HSplitView {
            // 左侧：搜索和结果
            VStack(alignment: .leading) {
                HStack {
                    TextField("搜索知识...", text: $viewModel.searchQuery)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { viewModel.searchKnowledge() }
                    Button("搜索") { viewModel.searchKnowledge() }
                        .disabled(viewModel.searchQuery.isEmpty)
                }
                .padding()

                List(viewModel.searchResults) { result in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(result.source).font(.caption).foregroundStyle(.blue)
                        Text(result.content).font(.body).lineLimit(3)
                    }
                    .padding(.vertical, 4)
                }
            }
            .frame(minWidth: 300)

            // 右侧：统计和添加
            VStack(alignment: .leading, spacing: 16) {
                GroupBox("统计") {
                    VStack(alignment: .leading, spacing: 8) {
                        LabeledContent("知识条目", value: "\(viewModel.knowledgeCount)")
                        LabeledContent("工作记忆", value: "\(viewModel.workingMemoryCount)")
                    }
                    .padding(4)
                }

                GroupBox("添加知识") {
                    VStack(alignment: .leading, spacing: 8) {
                        TextField("来源", text: $viewModel.newSource)
                            .textFieldStyle(.roundedBorder)
                        TextEditor(text: $viewModel.newContent)
                            .frame(minHeight: 100)
                            .border(Color.gray.opacity(0.3))
                        Button("添加") { viewModel.addKnowledge() }
                            .disabled(viewModel.newSource.isEmpty || viewModel.newContent.isEmpty)
                    }
                    .padding(4)
                }

                Spacer()
            }
            .padding()
            .frame(minWidth: 250)
        }
        .onAppear { viewModel.loadStats() }
    }
}

@MainActor
class MemoryViewModel: ObservableObject {
    @Published var searchQuery = ""
    @Published var searchResults: [KnowledgeResult] = []
    @Published var workingMemoryCount: UInt32 = 0
    @Published var knowledgeCount: UInt32 = 0
    @Published var newSource = ""
    @Published var newContent = ""
    private let engine = PrometheusEngine.shared

    func loadStats() {
        do {
            knowledgeCount = try engine.knowledgeCount()
            workingMemoryCount = try engine.workingMemoryCount()
        } catch {}
    }

    func searchKnowledge() {
        guard !searchQuery.isEmpty else { return }
        do { searchResults = try engine.searchKnowledge(searchQuery) } catch {}
    }

    func addKnowledge() {
        guard !newSource.isEmpty, !newContent.isEmpty else { return }
        do {
            let chunks = try engine.addKnowledge(source: newSource, content: newContent)
            if chunks > 0 { newSource = ""; newContent = ""; loadStats() }
        } catch {}
    }
}
