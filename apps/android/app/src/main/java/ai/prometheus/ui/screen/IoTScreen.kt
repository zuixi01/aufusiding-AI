package ai.prometheus.ui.screen

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import ai.prometheus.bridge.DeviceInfo
import ai.prometheus.bridge.PrometheusEngine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class IoTViewModel : ViewModel() {
    private val _devices = MutableStateFlow<List<DeviceInfo>>(emptyList())
    val devices: StateFlow<List<DeviceInfo>> = _devices.asStateFlow()

    private val _isScanning = MutableStateFlow(false)
    val isScanning: StateFlow<Boolean> = _isScanning.asStateFlow()

    fun scanDevices() {
        _isScanning.value = true
        viewModelScope.launch(Dispatchers.IO) {
            try { _devices.value = PrometheusEngine.scanDevices() } catch (_: Exception) {}
            _isScanning.value = false
        }
    }

    fun controlDevice(id: String, command: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try { PrometheusEngine.controlDevice(id, command) } catch (_: Exception) {}
        }
    }
}

@Composable
fun IoTScreen(viewModel: IoTViewModel = viewModel()) {
    val devices by viewModel.devices.collectAsState()
    val isScanning by viewModel.isScanning.collectAsState()

    Column(modifier = Modifier.fillMaxSize()) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text("已发现 ${devices.size} 个设备", style = MaterialTheme.typography.titleMedium)
            Button(
                onClick = { viewModel.scanDevices() },
                enabled = !isScanning
            ) {
                if (isScanning) {
                    CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                    Spacer(modifier = Modifier.width(8.dp))
                }
                Text("扫描")
            }
        }

        if (devices.isEmpty()) {
            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(Icons.Default.WifiOff, contentDescription = null,
                        modifier = Modifier.size(64.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f))
                    Spacer(modifier = Modifier.height(16.dp))
                    Text("暂无设备", style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        } else {
            LazyColumn(
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                items(devices, key = { it.id }) { device ->
                    DeviceCard(device = device, onControl = { cmd ->
                        viewModel.controlDevice(device.id, cmd)
                    })
                }
            }
        }
    }
}

@Composable
private fun DeviceCard(device: DeviceInfo, onControl: (String) -> Unit) {
    var isOn by remember { mutableStateOf(false) }

    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Icon(
                imageVector = when (device.deviceType) {
                    "light" -> Icons.Default.Lightbulb
                    "thermostat" -> Icons.Default.Thermostat
                    "camera" -> Icons.Default.Videocam
                    "speaker" -> Icons.Default.Speaker
                    else -> Icons.Default.Sensors
                },
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(32.dp)
            )
            Spacer(modifier = Modifier.width(16.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(device.name, style = MaterialTheme.typography.titleSmall)
                Text(device.deviceType, style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Switch(checked = isOn, onCheckedChange = { checked ->
                isOn = checked
                onControl(if (checked) "on" else "off")
            })
        }
    }
}
