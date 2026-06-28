import SwiftUI

struct SettingsView: View {
    @AppStorage("apiKey") private var apiKey = ""
    @AppStorage("preferLocal") private var preferLocal = true
    @AppStorage("language") private var language = "zh-CN"
    @StateObject private var viewModel = SettingsViewModel()

    var body: some View {
        TabView {
            generalTab.tabItem { Label("通用", systemImage: "gearshape") }
            modelTab.tabItem { Label("模型", systemImage: "cpu") }
            privacyTab.tabItem { Label("隐私", systemImage: "lock.shield") }
            aboutTab.tabItem { Label("关于", systemImage: "info.circle") }
        }
        .frame(width: 500, height: 400)
        .onAppear { viewModel.loadModels() }
    }

    private var generalTab: some View {
        Form {
            Picker("语言", selection: $language) {
                Text("简体中文").tag("zh-CN")
                Text("English").tag("en-US")
            }
            Toggle("优先本地推理", isOn: $preferLocal)
            SecureField("OpenAI API Key", text: $apiKey)
        }
        .formStyle(.grouped)
        .padding()
    }

    private var modelTab: some View {
        Form {
            Picker("当前模型", selection: $viewModel.selectedModel) {
                ForEach(viewModel.models) { model in
                    HStack {
                        Text(model.id)
                        if model.isLocal {
                            Image(systemName: "desktopcomputer").font(.caption)
                        }
                    }.tag(model.id)
                }
            }
            .onChange(of: viewModel.selectedModel) { _, v in viewModel.switchModel(v) }

            NavigationLink("人格管理", destination: PersonaView())
            NavigationLink("同步状态", destination: SyncView())
        }
        .formStyle(.grouped)
        .padding()
    }

    private var privacyTab: some View {
        Form {
            Button("导出数据") { viewModel.exportData() }
            Button("删除所有数据", role: .destructive) { viewModel.showDeleteConfirm = true }
        }
        .formStyle(.grouped)
        .padding()
        .alert("确认删除", isPresented: $viewModel.showDeleteConfirm) {
            Button("取消", role: .cancel) {}
            Button("删除", role: .destructive) { viewModel.deleteData() }
        } message: {
            Text("此操作将删除所有用户数据，不可恢复。")
        }
        .sheet(isPresented: $viewModel.showExportSheet) {
            VStack {
                Text("导出数据").font(.headline).padding()
                ScrollView {
                    Text(viewModel.exportedData ?? "")
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .padding()
                }
                Button("关闭") { viewModel.showExportSheet = false }.padding()
            }
            .frame(width: 500, height: 400)
        }
    }

    private var aboutTab: some View {
        Form {
            LabeledContent("版本", value: "0.1.0")
            LabeledContent("构建", value: "Phase 2")
        }
        .formStyle(.grouped)
        .padding()
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
        do { models = try engine.listModels(); selectedModel = try engine.getActiveModel() } catch {}
    }
    func switchModel(_ id: String) { do { _ = try engine.switchModel(id) } catch {} }
    func exportData() {
        do { exportedData = try engine.exportData(); showExportSheet = true } catch {}
    }
    func deleteData() { do { _ = try engine.deleteData() } catch {} }
}

#Preview { SettingsView() }
