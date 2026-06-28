import Foundation
import prometheus  // UniFFI 生成的 Swift 模块

// MARK: - Swift 数据模型

struct Conversation: Identifiable {
    let id: String
    let title: String
    let messages: [ChatMessage]
}

struct ChatMessage: Identifiable {
    let id: UUID
    let role: String
    let content: String
    let isUser: Bool
    let timestamp: Date

    init(role: String, content: String, timestamp: Date = Date()) {
        self.id = UUID()
        self.role = role
        self.content = content
        self.isUser = (role == "user")
        self.timestamp = timestamp
    }

    init(content: String, isUser: Bool, timestamp: Date = Date()) {
        self.id = UUID()
        self.role = isUser ? "user" : "assistant"
        self.content = content
        self.isUser = isUser
        self.timestamp = timestamp
    }
}

struct ModelInfo: Identifiable {
    let id: String
    let provider: String
    let isLocal: Bool
}

struct DeviceInfo: Identifiable {
    let id: String
    let name: String
    let deviceType: String
}

struct PersonaInfo: Identifiable {
    let id: String
    let name: String
    let tone: String
}

struct KnowledgeResult: Identifiable {
    let id = UUID()
    let source: String
    let content: String
    let score: Double
}

struct SyncStatus {
    let lastSync: String?
    let pendingOps: Int
    let connected: Bool
}

// MARK: - 错误类型

enum PrometheusError: Error, LocalizedError {
    case engineNotInitialized
    case enginePaused
    case engineShutdown
    case ffiError(String)
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .engineNotInitialized: return "引擎未初始化"
        case .enginePaused: return "引擎已暂停"
        case .engineShutdown: return "引擎已关闭"
        case .ffiError(let msg): return "FFI 错误: \(msg)"
        case .invalidResponse: return "无效响应"
        }
    }
}

// MARK: - PrometheusEngine 单例

final class PrometheusEngine: @unchecked Sendable {
    static let shared = PrometheusEngine()

    private var api: PrometheusApi?
    private var engine: EngineHandle?
    private let lock = NSLock()

    private init() {}

    func initialize() throws {
        lock.lock()
        defer { lock.unlock() }
        guard engine == nil else { return }
        let api = PrometheusApi()
        self.api = api
        self.engine = api.createEngine()
    }

    private func getEngine() throws -> EngineHandle {
        lock.lock()
        defer { lock.unlock() }
        guard let engine = engine else {
            throw PrometheusError.engineNotInitialized
        }
        return engine
    }

    // MARK: - Chat

    func sendMessage(_ text: String) async throws -> String {
        let engine = try getEngine()
        return engine.sendMessage(message: text)
    }

    func getConversation() throws -> Conversation {
        let engine = try getEngine()
        let ffi = engine.getConversation()
        let messages = ffi.messages.map { msg in
            ChatMessage(
                role: msg.role,
                content: msg.content,
                timestamp: Date(timeIntervalSince1970: Double(msg.timestampMs) / 1000.0)
            )
        }
        return Conversation(id: ffi.id, title: ffi.title, messages: messages)
    }

    // MARK: - Models

    func listModels() throws -> [ModelInfo] {
        guard let api = api else { throw PrometheusError.engineNotInitialized }
        return api.listModelInfo().map { info in
            ModelInfo(id: info.id, provider: info.provider, isLocal: info.isLocal)
        }
    }

    func getActiveModel() throws -> String {
        let engine = try getEngine()
        return engine.getActiveModel()
    }

    func switchModel(_ modelId: String) throws -> Bool {
        let engine = try getEngine()
        return engine.setActiveModel(model: modelId)
    }

    // MARK: - Memory

    func addMemory(_ content: String, importance: Double) throws -> Bool {
        let engine = try getEngine()
        return engine.addWorkingMemory(content: content, importance: importance)
    }

    func getWorkingMemory(maxItems: UInt32 = 20) throws -> [FfiMemoryItem] {
        let engine = try getEngine()
        return engine.getWorkingMemory(maxItems: maxItems)
    }

    func workingMemoryCount() throws -> UInt32 {
        let engine = try getEngine()
        return engine.workingMemoryCount()
    }

    func addKnowledge(source: String, content: String) throws -> Int32 {
        let engine = try getEngine()
        return engine.addKnowledge(source: source, content: content)
    }

    func searchKnowledge(_ query: String, topK: UInt32 = 5) throws -> [KnowledgeResult] {
        let engine = try getEngine()
        return engine.searchKnowledge(query: query, topK: topK).map { r in
            KnowledgeResult(source: r.source, content: r.content, score: r.score)
        }
    }

    func knowledgeCount() throws -> UInt32 {
        let engine = try getEngine()
        return engine.knowledgeCount()
    }

    func searchEpisodes(_ query: String, topK: UInt32 = 5) throws -> [String] {
        let engine = try getEngine()
        return engine.searchEpisodes(query: query, topK: topK)
    }

    // MARK: - IoT

    func scanDevices() async throws -> [DeviceInfo] {
        let engine = try getEngine()
        return engine.scanIotDevices().map { d in
            DeviceInfo(id: d.id, name: d.name, deviceType: d.deviceType)
        }
    }

    func controlDevice(_ id: String, command: String, params: String = "{}") async throws -> String {
        let engine = try getEngine()
        return engine.controlDevice(deviceId: id, command: command, params: params)
    }

    // MARK: - Persona

    func listPersonas() throws -> [PersonaInfo] {
        let engine = try getEngine()
        return engine.listPersonas().map { p in
            PersonaInfo(id: p.id, name: p.name, tone: p.tone)
        }
    }

    func switchPersona(_ id: String) throws -> Bool {
        let engine = try getEngine()
        return engine.switchPersona(personaId: id)
    }

    // MARK: - Sync

    func getSyncStatus() throws -> SyncStatus {
        let engine = try getEngine()
        let json = engine.getSyncStatus()
        guard let data = json.data(using: .utf8),
              let dict = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return SyncStatus(lastSync: nil, pendingOps: 0, connected: false)
        }
        return SyncStatus(
            lastSync: dict["last_sync"] as? String,
            pendingOps: dict["pending_ops"] as? Int ?? 0,
            connected: dict["connected"] as? Bool ?? false
        )
    }

    // MARK: - Lifecycle

    func pause() throws { let engine = try getEngine(); _ = engine.pause() }
    func resume() throws { let engine = try getEngine(); _ = engine.resume() }
    func shutdown() throws { let engine = try getEngine(); _ = engine.shutdown() }

    // MARK: - Privacy

    func exportData() throws -> String {
        let engine = try getEngine()
        return engine.exportUserData()
    }

    func deleteData() throws -> Bool {
        let engine = try getEngine()
        return engine.deleteUserData()
    }

    func recordConsent(purpose: String, granted: Bool) throws -> Bool {
        let engine = try getEngine()
        return engine.recordConsent(purpose: purpose, granted: granted)
    }

    func anonymizeText(_ text: String) throws -> String {
        let engine = try getEngine()
        return engine.anonymizeText(text: text)
    }

    // MARK: - Settings

    func getSetting(_ key: String) throws -> String {
        let engine = try getEngine()
        return engine.getSetting(key: key)
    }

    func setSetting(_ key: String, value: String) throws -> Bool {
        let engine = try getEngine()
        return engine.setSetting(key: key, value: value)
    }
}
