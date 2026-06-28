package ai.prometheus.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import ai.prometheus.bridge.ChatMessage
import ai.prometheus.bridge.ModelInfo
import ai.prometheus.bridge.PrometheusEngine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class ChatViewModel : ViewModel() {
    private val _messages = MutableStateFlow<List<ChatMessage>>(emptyList())
    val messages: StateFlow<List<ChatMessage>> = _messages.asStateFlow()

    private val _isLoading = MutableStateFlow(false)
    val isLoading: StateFlow<Boolean> = _isLoading.asStateFlow()

    private val _currentModel = MutableStateFlow("")
    val currentModel: StateFlow<String> = _currentModel.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    private val _availableModels = MutableStateFlow<List<ModelInfo>>(emptyList())
    val availableModels: StateFlow<List<ModelInfo>> = _availableModels.asStateFlow()

    init {
        loadModels()
        loadConversation()
    }

    fun sendMessage(text: String) {
        if (text.isBlank()) return
        val userMsg = ChatMessage(role = "user", content = text)
        _messages.value = _messages.value + userMsg
        _isLoading.value = true
        _error.value = null

        viewModelScope.launch(Dispatchers.IO) {
            try {
                val response = PrometheusEngine.sendMessage(text)
                val aiMsg = ChatMessage(role = "assistant", content = response)
                _messages.value = _messages.value + aiMsg
            } catch (e: Exception) {
                _error.value = e.message ?: "发送失败"
            } finally {
                _isLoading.value = false
            }
        }
    }

    fun loadConversation() {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                val conv = PrometheusEngine.getConversation()
                _messages.value = conv.messages
            } catch (e: Exception) {
                _error.value = e.message
            }
        }
    }

    fun loadModels() {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                _availableModels.value = PrometheusEngine.listModels()
                _currentModel.value = PrometheusEngine.getActiveModel()
            } catch (e: Exception) {
                _error.value = e.message
            }
        }
    }

    fun switchModel(modelId: String) {
        viewModelScope.launch(Dispatchers.IO) {
            try {
                if (PrometheusEngine.switchModel(modelId)) {
                    _currentModel.value = modelId
                }
            } catch (e: Exception) {
                _error.value = e.message
            }
        }
    }

    fun clearConversation() {
        _messages.value = emptyList()
    }

    fun dismissError() {
        _error.value = null
    }
}
