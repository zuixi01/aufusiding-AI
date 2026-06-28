import SwiftUI

struct SettingsView: View {
    @AppStorage("apiKey") private var apiKey = ""
    @AppStorage("preferLocal") private var preferLocal = true
    @AppStorage("language") private var language = "zh-CN"
    @StateObject private var viewModel = SettingsViewModel()

    var body: some View {
        Form {
            Section("模型") {
                Picker("当前模型", selection: $viewModel.selectedModel) {
                    ForEach(viewModel.models) { model in
                        HStack {
                            Text(model.id)
                            if model.isLocal {
                                Image(systemName: "desktopcomputer")
                                    .font(.caption)
                            }
                        }
                        .tag(model.id)
                    }
                }
                .onChange(of: viewModel.selectedModel) { _, newValue in
                    viewModel.switchModel(newValue)
                }
            }

            Section("通用") {
                Picker("语言", selection: $language) {
                    Text("简体中文").tag("zh-CN")
                    Text("English").tag("en-US")
                }
                Toggle("优先本地推理", isOn: $preferLocal)
            }

            Section("API 配置") {
                SecureField("OpenAI API Key", text: $apiKey)
                    .textFieldStyle(.roundedBorder)
            }

            Section("人格") {
                NavigationLink(destination: PersonaView()) {
                    Label("人格管理", systemImage: "person.2.fill")
                }
            }

            Section("同步") {
                NavigationLink(destination: SyncView()) {
                    Label("同步状态", systemImage: "arrow.triangle.2.circlepath")
                }
            }

            Section("隐私") {
                Button("导出数据") { viewModel.exportData() }
                Button("删除所有数据", role: .destructive) {
                    viewModel.showDeleteConfirm = true
                }
            }
            .alert("确认删除", isPresented: $viewModel.showDeleteConfirm) {
                Button("取消", role: .cancel) {}
                Button("删除", role: .destructive) { viewModel.deleteData() }
            } message: {
                Text("此操作将删除所有用户数据，不可恢复。")
            }

            Section("关于") {
                LabeledContent("版本", value: "0.1.0")
                LabeledContent("构建", value: "Phase 2")
            }
        }
        .formStyle(.grouped)
        .navigationTitle("设置")
        .onAppear { viewModel.loadModels() }
        .sheet(isPresented: $viewModel.showExportSheet) {
            if let data = viewModel.exportedData {
                NavigationStack {
                    ScrollView {
                        Text(data).font(.system(.caption, design: .monospaced)).padding()
                    }
                    .navigationTitle("导出数据")
                    .toolbar {
                        ToolbarItem(placement: .topBarTrailing) {
                            Button("完成") { viewModel.showExportSheet = false }
                        }
                    }
                }
            }
        }
    }
}

@MainActor
class SettingsViewModel: ObservableObject {
    @Published var models: [ModelInfo] = []
    @Published var selectedModel = ""
    @Published var showDeleteConfirm = false
    @Published var showExportSheet = false
    @Published var exportedData: String?

    private let engine = PrometheusEngine.shared

    func loadModels() {
        do {
            models = try engine.listModels()
            selectedModel = try engine.getActiveModel()
        } catch {}
    }

    func switchModel(_ modelId: String) {
        do { _ = try engine.switchModel(modelId) } catch {}
    }

    func exportData() {
        do {
            exportedData = try engine.exportData()
            showExportSheet = true
        } catch {}
    }

    func deleteData() {
        do { _ = try engine.deleteData() } catch {}
    }
}

#Preview {
    NavigationStack {
        SettingsView()
    }
}
