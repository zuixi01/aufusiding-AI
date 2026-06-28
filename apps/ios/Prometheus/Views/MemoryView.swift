import SwiftUI

struct MemoryView: View {
    @StateObject private var viewModel = MemoryViewModel()

    var body: some View {
        List {
            Section("知识库搜索") {
                HStack {
                    TextField("搜索知识...", text: $viewModel.searchQuery)
                        .textFieldStyle(.roundedBorder)
                        .onSubmit { viewModel.searchKnowledge() }

                    Button(action: { viewModel.searchKnowledge() }) {
                        Image(systemName: "magnifyingglass")
                    }
                    .disabled(viewModel.searchQuery.isEmpty)
                }

                ForEach(viewModel.searchResults) { result in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(result.source)
                            .font(.caption)
                            .foregroundStyle(.blue)
                        Text(result.content)
                            .font(.body)
                            .lineLimit(3)
                    }
                    .padding(.vertical, 4)
                }
            }

            Section("工作记忆 (\(viewModel.workingMemoryCount))") {
                ForEach(viewModel.workingMemory, id: \.content) { item in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item.content)
                            .font(.body)
                        Text("重要度: \(item.importance, specifier: "%.1f")")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 2)
                }
            }

            Section("添加知识") {
                TextField("来源", text: $viewModel.newSource)
                    .textFieldStyle(.roundedBorder)
                TextEditor(text: $viewModel.newContent)
                    .frame(minHeight: 80)
                    .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.gray.opacity(0.3)))

                Button("添加") { viewModel.addKnowledge() }
                    .disabled(viewModel.newSource.isEmpty || viewModel.newContent.isEmpty)
            }

            Section("统计") {
                LabeledContent("知识条目", value: "\(viewModel.knowledgeCount)")
                LabeledContent("工作记忆", value: "\(viewModel.workingMemoryCount)")
                LabeledContent("情景记忆", value: "\(viewModel.episodicCount)")
            }
        }
        .navigationTitle("记忆")
        .onAppear { viewModel.loadStats() }
    }
}

@MainActor
class MemoryViewModel: ObservableObject {
    @Published var searchQuery = ""
    @Published var searchResults: [KnowledgeResult] = []
    @Published var workingMemory: [FfiMemoryItem] = []
    @Published var workingMemoryCount: UInt32 = 0
    @Published var knowledgeCount: UInt32 = 0
    @Published var episodicCount: UInt32 = 0
    @Published var newSource = ""
    @Published var newContent = ""
    @Published var error: String?

    private let engine = PrometheusEngine.shared

    func loadStats() {
        do {
            knowledgeCount = try engine.knowledgeCount()
            workingMemoryCount = try engine.workingMemoryCount()
            workingMemory = try engine.getWorkingMemory()
            episodicCount = 0 // episodic count not directly exposed as simple call
        } catch {
            self.error = error.localizedDescription
        }
    }

    func searchKnowledge() {
        guard !searchQuery.isEmpty else { return }
        do {
            searchResults = try engine.searchKnowledge(searchQuery)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func addKnowledge() {
        guard !newSource.isEmpty, !newContent.isEmpty else { return }
        do {
            let chunks = try engine.addKnowledge(source: newSource, content: newContent)
            if chunks > 0 {
                newSource = ""
                newContent = ""
                loadStats()
            }
        } catch {
            self.error = error.localizedDescription
        }
    }
}

#Preview {
    NavigationStack {
        MemoryView()
    }
}
