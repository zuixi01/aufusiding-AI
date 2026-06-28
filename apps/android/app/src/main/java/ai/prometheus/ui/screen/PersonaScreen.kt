package ai.prometheus.ui.screen

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AccountCircle
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import ai.prometheus.bridge.PersonaInfo
import ai.prometheus.bridge.PrometheusEngine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class PersonaViewModel : ViewModel() {
    private val _personas = MutableStateFlow<List<PersonaInfo>>(emptyList())
    val personas: StateFlow<List<PersonaInfo>> = _personas.asStateFlow()

    private val _activeId = MutableStateFlow("")
    val activeId: StateFlow<String> = _activeId.asStateFlow()

    init { loadPersonas() }

    fun loadPersonas() {
        viewModelScope.launch(Dispatchers.IO) {
            try { _personas.value = PrometheusEngine.listPersonas() } catch (_: Exception) {}
        }
    }

    fun switchPersona(id: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try { if (PrometheusEngine.switchPersona(id)) _activeId.value = id } catch (_: Exception) {}
        }
    }
}

@Composable
fun PersonaScreen(viewModel: PersonaViewModel = viewModel()) {
    val personas by viewModel.personas.collectAsState()
    val activeId by viewModel.activeId.collectAsState()

    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        contentPadding = PaddingValues(16.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
        items(personas, key = { it.id }) { persona ->
            val isActive = persona.id == activeId
            Card(
                onClick = { viewModel.switchPersona(persona.id) },
                border = if (isActive) BorderStroke(2.dp, MaterialTheme.colorScheme.primary) else null,
                colors = CardDefaults.cardColors(
                    containerColor = if (isActive)
                        MaterialTheme.colorScheme.primaryContainer
                    else
                        MaterialTheme.colorScheme.surfaceVariant
                )
            ) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(16.dp),
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    Icon(Icons.Default.AccountCircle, contentDescription = null,
                        modifier = Modifier.size(48.dp),
                        tint = if (isActive) MaterialTheme.colorScheme.primary
                        else MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(modifier = Modifier.height(8.dp))
                    Text(persona.name, style = MaterialTheme.typography.titleSmall)
                    Text(persona.tone, style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2)
                }
            }
        }
    }
}
