package ai.prometheus.ui.screen

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import ai.prometheus.bridge.ModelInfo
import ai.prometheus.bridge.PrometheusEngine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class SettingsViewModel : ViewModel() {
    private val _models = MutableStateFlow<List<ModelInfo>>(emptyList())
    val models: StateFlow<List<ModelInfo>> = _models.asStateFlow()

    private val _selectedModel = MutableStateFlow("")
    val selectedModel: StateFlow<String> = _selectedModel.asStateFlow()

    private val _showDeleteDialog = MutableStateFlow(false)
    val showDeleteDialog: StateFlow<Boolean> = _showDeleteDialog.asStateFlow()

    init { loadModels() }

    fun loadModels() {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                _models.value = PrometheusEngine.listModels()
                _selectedModel.value = PrometheusEngine.getActiveModel()
            } catch (_: Exception) {}
        }
    }

    fun switchModel(modelId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                if (PrometheusEngine.switchModel(modelId)) {
                    _selectedModel.value = modelId
                }
            } catch (_: Exception) {}
        }
    }

    fun requestDelete() { _showDeleteDialog.value = true }
    fun dismissDelete() { _showDeleteDialog.value = false }

    fun deleteData() {
        viewModelScope.launch(Dispatchers.IO) {
            try { PrometheusEngine.deleteData() } catch (_: Exception) {}
        }
        _showDeleteDialog.value = false
    }

    fun exportData(): String {
        return try { PrometheusEngine.exportData() } catch (_: Exception) { "{}" }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    viewModel: SettingsViewModel = viewModel(),
    onNavigateToPersona: () -> Unit = {}
) {
    val models by viewModel.models.collectAsState()
    val selectedModel by viewModel.selectedModel.collectAsState()
    val showDeleteDialog by viewModel.showDeleteDialog.collectAsState()
    var modelExpanded by remember { mutableStateOf(false) }

    LazySettingsColumn {
        // Model section
        item {
            SettingsSection("模型") {
                ExposedDropdownMenuBox(
                    expanded = modelExpanded,
                    onExpandedChange = { modelExpanded = it }
                ) {
                    OutlinedTextField(
                        value = selectedModel,
                        onValueChange = {},
                        readOnly = true,
                        label = { Text("当前模型") },
                        trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = modelExpanded) },
                        modifier = Modifier.menuAnchor().fillMaxWidth()
                    )
                    ExposedDropdownMenu(
                        expanded = modelExpanded,
                        onDismissRequest = { modelExpanded = false }
                    ) {
                        models.forEach { model ->
                            DropdownMenuItem(
                                text = {
                                    Row {
                                        Text(model.id)
                                        if (model.isLocal) {
                                            Spacer(modifier = Modifier.width(8.dp))
                                            Text("(本地)", style = MaterialTheme.typography.bodySmall)
                                        }
                                    }
                                },
                                onClick = {
                                    viewModel.switchModel(model.id)
                                    modelExpanded = false
                                }
                            )
                        }
                    }
                }
            }
        }

        // Persona
        item {
            SettingsSection("人格") {
                OutlinedButton(
                    onClick = onNavigateToPersona,
                    modifier = Modifier.fillMaxWidth()
                ) { Text("人格管理") }
            }
        }

        // Privacy
        item {
            SettingsSection("隐私") {
                OutlinedButton(
                    onClick = { viewModel.exportData() },
                    modifier = Modifier.fillMaxWidth()
                ) { Text("导出数据") }
                Spacer(modifier = Modifier.height(8.dp))
                Button(
                    onClick = { viewModel.requestDelete() },
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error
                    )
                ) { Text("删除所有数据") }
            }
        }

        // About
        item {
            SettingsSection("关于") {
                Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text("版本"); Text("0.1.0")
                }
            }
        }
    }

    if (showDeleteDialog) {
        AlertDialog(
            onDismissRequest = { viewModel.dismissDelete() },
            title = { Text("确认删除") },
            text = { Text("此操作将删除所有用户数据，不可恢复。") },
            confirmButton = {
                TextButton(onClick = { viewModel.deleteData() }) {
                    Text("删除", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { viewModel.dismissDelete() }) { Text("取消") }
            }
        )
    }
}

@Composable
private fun SettingsSection(title: String, content: @Composable ColumnScope.() -> Unit) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.primary)
            Spacer(modifier = Modifier.height(12.dp))
            content()
        }
    }
}

@Composable
private fun LazySettingsColumn(content: androidx.compose.foundation.lazy.LazyListScope.() -> Unit) {
    androidx.compose.foundation.lazy.LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        content = content
    )
}
