import SwiftUI

struct ChatView: View {
    @ObservedObject var viewModel: ChatViewModel
    @State private var inputText = ""
    @FocusState private var isInputFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            if let error = viewModel.error {
                errorBanner(error)
            }
            messageList
            Divider()
            inputBar
        }
    }

    // MARK: - Error Banner

    private func errorBanner(_ error: String) -> some View {
        HStack {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.yellow)
            Text(error).font(.caption)
            Spacer()
            Button(action: viewModel.dismissError) {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(.secondary)
            }
        }
        .padding(10)
        .background(Color.red.opacity(0.1))
    }

    // MARK: - Message List

    private var messageList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                if viewModel.messages.isEmpty {
                    emptyStateView
                } else {
                    messagesContent
                }
            }
            .onChange(of: viewModel.messages.count) { _, _ in
                scrollToBottom(proxy: proxy)
            }
        }
    }

    private var messagesContent: some View {
        LazyVStack(alignment: .leading, spacing: 12) {
            ForEach(viewModel.messages) { message in
                MessageBubble(message: message).id(message.id)
            }
            if viewModel.isLoading {
                HStack { TypingIndicator(); Spacer() }
                    .padding(.horizontal).id("loading")
            }
        }
        .padding()
    }

    private var emptyStateView: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 60))
                .foregroundStyle(.tertiary)
            Text("开始对话")
                .font(.title2).foregroundStyle(.secondary)
            Text("输入消息与灵犀 AI 交流")
                .font(.subheadline).foregroundStyle(.tertiary)
            Spacer()
        }
        .frame(maxWidth: .infinity).padding(.top, 100)
    }

    // MARK: - Input Bar

    private var inputBar: some View {
        HStack(spacing: 12) {
            TextField("输入消息...", text: $inputText, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...5)
                .focused($isInputFocused)
                .onSubmit { sendMessage() }
            Button(action: sendMessage) {
                Image(systemName: "paperplane.fill").font(.title3)
            }
            .disabled(inputText.isEmpty || viewModel.isLoading)
        }
        .padding()
    }

    private func sendMessage() {
        let text = inputText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        viewModel.sendMessage(text)
        inputText = ""
    }

    private func scrollToBottom(proxy: ScrollViewProxy) {
        withAnimation(.easeOut(duration: 0.2)) {
            if let last = viewModel.messages.last {
                proxy.scrollTo(last.id, anchor: .bottom)
            }
        }
    }
}

// MARK: - MessageBubble

struct MessageBubble: View {
    let message: ChatMessage

    var body: some View {
        HStack(alignment: .top) {
            if message.isUser { Spacer(minLength: 60) }
            VStack(alignment: message.isUser ? .trailing : .leading, spacing: 4) {
                Text(message.content)
                    .padding(12)
                    .background(message.isUser ? Color.blue : Color(.systemGray5))
                    .foregroundColor(message.isUser ? .white : .primary)
                    .cornerRadius(16)
                Text(message.timestamp, style: .time)
                    .font(.caption2).foregroundStyle(.tertiary)
            }
            .contextMenu {
                Button(action: { UIPasteboard.general.string = message.content }) {
                    Label("复制", systemImage: "doc.on.doc")
                }
            }
            if !message.isUser { Spacer(minLength: 60) }
        }
    }
}

// MARK: - TypingIndicator

struct TypingIndicator: View {
    @State private var phase = 0.0

    var body: some View {
        HStack(spacing: 4) {
            ForEach(0..<3, id: \.self) { i in
                Circle().fill(Color.gray).frame(width: 8, height: 8)
                    .offset(y: sin(phase + Double(i) * 0.8) * 4)
            }
        }
        .padding(12)
        .background(Color(.systemGray5))
        .cornerRadius(16)
        .onAppear {
            withAnimation(.linear(duration: 1.0).repeatForever(autoreverses: false)) {
                phase = .pi * 2
            }
        }
    }
}
