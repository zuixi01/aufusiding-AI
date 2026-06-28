import SwiftUI

struct IoTView: View {
    @StateObject private var viewModel = IoTViewModel()

    var body: some View {
        List {
            Section {
                Button(action: { viewModel.scanDevices() }) {
                    HStack {
                        Image(systemName: "antenna.radiowaves.left.and.right")
                        Text("扫描设备")
                        Spacer()
                        if viewModel.isScanning {
                            ProgressView()
                        }
                    }
                }
                .disabled(viewModel.isScanning)
            }

            Section("已发现设备") {
                if viewModel.devices.isEmpty {
                    ContentUnavailableView(
                        "暂无设备",
                        systemImage: "wifi.slash",
                        description: Text("点击扫描按钮发现附近设备")
                    )
                } else {
                    ForEach(viewModel.devices) { device in
                        DeviceRow(device: device, onControl: { command in
                            viewModel.controlDevice(device.id, command: command)
                        })
                    }
                }
            }
        }
        .navigationTitle("IoT 设备")
        .alert("错误", isPresented: .constant(viewModel.error != nil)) {
            Button("确定") { viewModel.error = nil }
        } message: {
            Text(viewModel.error ?? "")
        }
    }
}

struct DeviceRow: View {
    let device: DeviceInfo
    let onControl: (String) -> Void
    @State private var isOn = false

    var body: some View {
        HStack {
            Image(systemName: iconForType(device.deviceType))
                .font(.title2)
                .foregroundStyle(.blue)
                .frame(width: 40)

            VStack(alignment: .leading) {
                Text(device.name)
                    .font(.headline)
                Text(device.deviceType)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Toggle("", isOn: $isOn)
                .labelsHidden()
                .onChange(of: isOn) { _, newValue in
                    onControl(newValue ? "on" : "off")
                }
        }
        .padding(.vertical, 4)
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
        error = nil
        Task {
            do {
                devices = try await engine.scanDevices()
            } catch {
                self.error = error.localizedDescription
            }
            isScanning = false
        }
    }

    func controlDevice(_ id: String, command: String) {
        Task {
            do {
                _ = try await engine.controlDevice(id, command: command)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

#Preview {
    NavigationStack {
        IoTView()
    }
}
