import SwiftUI

struct PersonaView: View {
    @StateObject private var viewModel = PersonaViewModel()

    var body: some View {
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 200))], spacing: 16) {
                ForEach(viewModel.personas) { persona in
                    PersonaCard(
                        persona: persona,
                        isActive: persona.id == viewModel.activePersonaId,
                        onSelect: { viewModel.switchPersona(persona.id) }
                    )
                }
            }
            .padding()
        }
        .onAppear { viewModel.loadPersonas() }
    }
}

struct PersonaCard: View {
    let persona: PersonaInfo
    let isActive: Bool
    let onSelect: () -> Void

    var body: some View {
        Button(action: onSelect) {
            VStack(spacing: 12) {
                Image(systemName: "person.circle.fill")
                    .font(.system(size: 44))
                    .foregroundStyle(isActive ? .blue : .gray)
                Text(persona.name).font(.headline).foregroundStyle(.primary)
                Text(persona.tone).font(.caption).foregroundStyle(.secondary)
                    .lineLimit(2).multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity).padding()
            .background(isActive ? Color.blue.opacity(0.1) : Color(.windowBackgroundColor))
            .cornerRadius(16)
            .overlay(RoundedRectangle(cornerRadius: 16).stroke(isActive ? Color.blue : Color.clear, lineWidth: 2))
        }
        .buttonStyle(.plain)
    }
}

@MainActor
class PersonaViewModel: ObservableObject {
    @Published var personas: [PersonaInfo] = []
    @Published var activePersonaId = ""
    @Published var error: String?
    private let engine = PrometheusEngine.shared

    func loadPersonas() {
        do { personas = try engine.listPersonas() } catch { self.error = error.localizedDescription }
    }

    func switchPersona(_ id: String) {
        do { if try engine.switchPersona(id) { activePersonaId = id } } catch { self.error = error.localizedDescription }
    }
}
