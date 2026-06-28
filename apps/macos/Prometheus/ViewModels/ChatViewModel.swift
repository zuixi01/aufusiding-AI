import Foundation
import Combine

@MainActor
class ChatViewModel: ObservableObject {
    @Published var messages: [ChatMessage] = []
    @Published var isLoading = false
    @Published var currentModel = ""
    @Published var error: String?
    @Published var availableModels: [ModelInfo] = []

    private let engine = PrometheusEngine.shared

    init() {
        loadModels()
        loadConversation()
    }

    func sendMessage(_ text: String) {
        guard !text.isEmpty else { return }
        let userMessage = ChatMessage(content: text, isUser: true)
        messages.append(userMessage)
        isLoading = true
        error = nil

        Task {
            do {
                let response = try await engine.sendMessage(text)
                let aiMessage = ChatMessage(content: response, isUser: false)
                messages.append(aiMessage)
            } catch {
                self.error = error.localizedDescription
            }
            isLoading = false
        }
    }

    func loadConversation() {
        do {
            let conversation = try engine.getConversation()
            messages = conversation.messages
        } catch {
            self.error = error.localizedDescription
        }
    }

    func loadModels() {
        do {
            availableModels = try engine.listModels()
            currentModel = try engine.getActiveModel()
        } catch {
            self.error = error.localizedDescription
        }
    }

    func switchModel(_ modelId: String) {
        do {
            if try engine.switchModel(modelId) {
                currentModel = modelId
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    func clearConversation() {
        messages.removeAll()
    }

    func dismissError() {
        error = nil
    }
}
