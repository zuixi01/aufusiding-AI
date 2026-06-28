import SwiftUI

struct IoTView: View {
    @StateObject private var viewModel = IoTViewModel()

    var body: some View {
        VStack {
            HStack {
                Text("已发现 \(viewModel.devices.count) 个设备")
                    .font(.headline)
                Spacer()
                Button(action: { viewModel.scanDevices() }) {
                    HStack {
                        if viewModel.isScanning { ProgressView().controlSize(.small) }
                        Text("扫描")
                    }
                }
                .disabled(viewModel.isScanning)
            }
            .padding()

            if viewModel.devices.isEmpty {
                ContentUnavailableView(
                    "暂无设备",
                    systemImage: "wifi.slash",
                    description: Text("点击扫描按钮发现附近设备")
                )
            } else {
                List(viewModel.devices) { device in
                    HStack {
                        Image(systemName: iconForType(device.deviceType))
                            .font(.title2).foregroundStyle(.blue).frame(width: 40)
                        VStack(alignment: .leading) {
                            Text(device.name).font(.headline)
                            Text(device.deviceType).font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("控制") { viewModel.controlDevice(device.id, command: "toggle") }
                            .buttonStyle(.bordered)
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private func iconForType(_ type: String) -> String {
        switch type {
        case "light": return "lightbulb.fill"
        case "thermostat": return "thermometer"
        case "camera": return "video.fill"
        case "speaker": return "hifispeaker.fill"
        default: return "sensor.fill"
        }
    }
}

@MainActor
class IoTViewModel: ObservableObject {
    @Published var devices: [DeviceInfo] = []
    @Published var isScanning = false
    @Published var error: String?
    private let engine = PrometheusEngine.shared

    func scanDevices() {
        isScanning = true
        Task {
            do { devices = try await engine.scanDevices() } catch { self.error = error.localizedDescription }
            isScanning = false
        }
    }

    func controlDevice(_ id: String, command: String) {
        Task { do { _ = try await engine.controlDevice(id, command: command) } catch {} }
    }
}
