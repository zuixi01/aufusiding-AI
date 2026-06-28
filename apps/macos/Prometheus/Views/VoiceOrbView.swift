import SwiftUI

enum VoiceState {
    case idle, listening, processing, responding

    var color: [Color] {
        switch self {
        case .idle: return [.blue, .purple]
        case .listening: return [.green, .cyan]
        case .processing: return [.orange, .yellow]
        case .responding: return [.purple, .pink]
        }
    }

    var icon: String {
        switch self {
        case .idle: return "waveform"
        case .listening: return "mic.fill"
        case .processing: return "ellipsis"
        case .responding: return "speaker.wave.2.fill"
        }
    }
}

struct VoiceOrbView: View {
    @ObservedObject var chatViewModel: ChatViewModel
    @State private var voiceState: VoiceState = .idle
    @State private var scale: CGFloat = 1.0
    @State private var rotation: Double = 0

    var body: some View {
        ZStack {
            // 外圈脉冲
            if voiceState == .listening {
                Circle()
                    .stroke(lineWidth: 2)
                    .fill(Color.green.opacity(0.3))
                    .frame(width: 130, height: 130)
                    .scaleEffect(scale)
            }

            // 主圆
            Circle()
                .fill(
                    LinearGradient(
                        colors: voiceState.color,
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
                .frame(width: 100, height: 100)
                .scaleEffect(voiceState == .listening ? 1.1 : 1.0)
                .shadow(color: voiceState.color.first?.opacity(0.4) ?? .clear, radius: 10)

            // 图标
            Image(systemName: voiceState.icon)
                .font(.system(size: 36))
                .foregroundColor(.white)
                .rotationEffect(.degrees(voiceState == .processing ? rotation : 0))
        }
        .animation(.easeInOut(duration: 0.3), value: voiceState)
        .animation(
            voiceState == .listening
                ? .easeInOut(duration: 1.0).repeatForever(autoreverses: true)
                : .default,
            value: scale
        )
        .onLongPressGesture(minimumDuration: 0.2, pressing: { pressing in
            if pressing {
                startListening()
            } else {
                stopListening()
            }
        }, perform: {})
        .onTapGesture {
            if voiceState == .idle {
                startListening()
            } else {
                cancelListening()
            }
        }
        .onChange(of: voiceState) { _, newState in
            scale = newState == .listening ? 1.2 : 1.0
            if newState == .processing {
                withAnimation(.linear(duration: 2.0).repeatForever(autoreverses: false)) {
                    rotation = 360
                }
            } else {
                rotation = 0
            }
        }
    }

    private func startListening() {
        voiceState = .listening
    }

    private func stopListening() {
        guard voiceState == .listening else { return }
        voiceState = .processing
        // 模拟语音识别 → 发送到聊天
        Task {
            try? await Task.sleep(for: .seconds(1))
            await MainActor.run {
                voiceState = .responding
                chatViewModel.sendMessage("(语音输入)")
            }
            try? await Task.sleep(for: .seconds(0.5))
            await MainActor.run {
                voiceState = .idle
            }
        }
    }

    private func cancelListening() {
        voiceState = .idle
    }
}

#Preview {
    VoiceOrbView(chatViewModel: ChatViewModel())
}
