package ai.prometheus.ui.screen

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import ai.prometheus.bridge.KnowledgeResult
import ai.prometheus.bridge.PrometheusEngine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class MemoryViewModel : ViewModel() {
    private val _searchResults = MutableStateFlow<List<KnowledgeResult>>(emptyList())
    val searchResults: StateFlow<List<KnowledgeResult>> = _searchResults.asStateFlow()

    private val _knowledgeCount = MutableStateFlow(0u)
    val knowledgeCount: StateFlow<UInt> = _knowledgeCount.asStateFlow()

    private val _workingMemoryCount = MutableStateFlow(0u)
    val workingMemoryCount: StateFlow<UInt> = _workingMemoryCount.asStateFlow()

    init { loadStats() }

    fun loadStats() {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                _knowledgeCount.value = PrometheusEngine.knowledgeCount()
                _workingMemoryCount.value = PrometheusEngine.workingMemoryCount()
            } catch (_: Exception) {}
        }
    }

    fun search(query: String) {
        if (query.isBlank()) return
        viewModelScope.launch(Dispatchers.IO) {
            try { _searchResults.value = PrometheusEngine.searchKnowledge(query) }
            catch (_: Exception) {}
        }
    }

    fun addKnowledge(source: String, content: String) {
        if (source.isBlank() || content.isBlank()) return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                PrometheusEngine.addKnowledge(source, content)
                loadStats()
            } catch (_: Exception) {}
        }
    }
}

@Composable
fun MemoryScreen(viewModel: MemoryViewModel = viewModel()) {
    val searchResults by viewModel.searchResults.collectAsState()
    val knowledgeCount by viewModel.knowledgeCount.collectAsState()
    val workingMemoryCount by viewModel.workingMemoryCount.collectAsState()
    var searchQuery by remember { mutableStateOf("") }
    var newSource by remember { mutableStateOf("") }
    var newContent by remember { mutableStateOf("") }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        // Stats
        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                    horizontalArrangement = Arrangement.SpaceEvenly
                ) {
                    StatItem("知识条目", "$knowledgeCount")
                    StatItem("工作记忆", "$workingMemoryCount")
                }
            }
        }

        // Search
        item {
            OutlinedTextField(
                value = searchQuery,
                onValueChange = { searchQuery = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("搜索知识...") },
                trailingIcon = {
                    TextButton(onClick = { viewModel.search(searchQuery) }) { Text("搜索") }
                },
                singleLine = true
            )
        }

        // Results
        items(searchResults) { result ->
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(12.dp)) {
                    Text(result.source, style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary)
                    Spacer(modifier = Modifier.height(4.dp))
                    Text(result.content, style = MaterialTheme.typography.bodyMedium, maxLines = 3)
                }
            }
        }

        // Add knowledge
        item {
            Card(modifier = Modifier.fillMaxWidth()) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("添加知识", style = MaterialTheme.typography.titleSmall)
                    OutlinedTextField(
                        value = newSource, onValueChange = { newSource = it },
                        modifier = Modifier.fillMaxWidth(), label = { Text("来源") }, singleLine = true
                    )
                    OutlinedTextField(
                        value = newContent, onValueChange = { newContent = it },
                        modifier = Modifier.fillMaxWidth(), label = { Text("内容") },
                        minLines = 3, maxLines = 5
                    )
                    Button(
                        onClick = {
                            viewModel.addKnowledge(newSource, newContent)
                            newSource = ""; newContent = ""
                        },
                        enabled = newSource.isNotBlank() && newContent.isNotBlank()
                    ) { Text("添加") }
                }
            }
        }
    }
}

@Composable
private fun StatItem(label: String, value: String) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(value, style = MaterialTheme.typography.headlineMedium)
        Text(label, style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
