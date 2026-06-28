import SwiftUI

struct ChatView: View {
    @ObservedObject var viewModel: ChatViewModel
    @State private var inputText = ""

    var body: some View {
        VStack(spacing: 0) {
            if let error = viewModel.error {
                HStack {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.yellow)
                    Text(error).font(.caption)
                    Spacer()
                    Button(action: viewModel.dismissError) {
                        Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                }
                .padding(8)
                .background(Color.red.opacity(0.1))
            }

            ScrollViewReader { proxy in
                ScrollView {
                    if viewModel.messages.isEmpty {
                        emptyStateView
                    } else {
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
                }
                .onChange(of: viewModel.messages.count) { _, _ in
                    if let last = viewModel.messages.last {
                        withAnimation { proxy.scrollTo(last.id, anchor: .bottom) }
                    }
                }
            }

            Divider()

            HStack(spacing: 12) {
                TextField("输入消息... (⌘+Enter 发送)", text: $inputText, axis: .vertical)
                    .textFieldStyle(.plain)
                    .lineLimit(1...8)
                    .padding(8)
                    .background(Color(.textBackgroundColor))
                    .cornerRadius(8)
                    .onSubmit { sendMessage() }

                Button(action: sendMessage) {
                    Image(systemName: "paperplane.fill").font(.title3)
                }
                .buttonStyle(.borderedProminent)
                .disabled(inputText.isEmpty || viewModel.isLoading)
                .keyboardShortcut(.return, modifiers: .command)
            }
            .padding()
        }
    }

    private var emptyStateView: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 60)).foregroundStyle(.tertiary)
            Text("开始对话").font(.title2).foregroundStyle(.secondary)
            Text("输入消息与灵犀 AI 交流").font(.subheadline).foregroundStyle(.tertiary)
            Spacer()
        }
        .frame(maxWidth: .infinity).padding(.top, 100)
    }

    private func sendMessage() {
        let text = inputText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        viewModel.sendMessage(text)
        inputText = ""
    }
}

struct MessageBubble: View {
    let message: ChatMessage

    var body: some View {
        HStack(alignment: .top) {
            if message.isUser { Spacer(minLength: 100) }
            VStack(alignment: message.isUser ? .trailing : .leading, spacing: 4) {
                Text(message.content)
                    .textSelection(.enabled)
                    .padding(12)
                    .background(message.isUser ? Color.blue : Color(.windowBackgroundColor))
                    .foregroundColor(message.isUser ? .white : .primary)
                    .cornerRadius(12)
                Text(message.timestamp, style: .time)
                    .font(.caption2).foregroundStyle(.tertiary)
            }
            .contextMenu {
                Button("复制") { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(message.content, forType: .string) }
            }
            if !message.isUser { Spacer(minLength: 100) }
        }
    }
}

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
        .background(Color(.windowBackgroundColor))
        .cornerRadius(12)
        .onAppear {
            withAnimation(.linear(duration: 1.0).repeatForever(autoreverses: false)) {
                phase = .pi * 2
            }
        }
    }
}
