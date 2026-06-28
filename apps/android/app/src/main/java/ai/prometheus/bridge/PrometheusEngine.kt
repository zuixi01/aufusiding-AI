package ai.prometheus.bridge

import uniffi.prometheus.EngineHandle
import uniffi.prometheus.PrometheusApi
import uniffi.prometheus.FfiChatMessage
import uniffi.prometheus.FfiConversation
import uniffi.prometheus.FfiModelInfo
import uniffi.prometheus.FfiMemoryItem
import uniffi.prometheus.FfiKnowledgeResult
import uniffi.prometheus.FfiDeviceInfo
import uniffi.prometheus.FfiPersonaInfo
import org.json.JSONObject
import java.util.Date

// MARK: - Data Models

data class ChatMessage(
    val id: String = java.util.UUID.randomUUID().toString(),
    val role: String,
    val content: String,
    val isUser: Boolean = (role == "user"),
    val timestamp: Long = System.currentTimeMillis()
)

data class Conversation(
    val id: String,
    val title: String,
    val messages: List<ChatMessage>
)

data class ModelInfo(
    val id: String,
    val provider: String,
    val isLocal: Boolean
)

data class DeviceInfo(
    val id: String,
    val name: String,
    val deviceType: String
)

data class PersonaInfo(
    val id: String,
    val name: String,
    val tone: String
)

data class KnowledgeResult(
    val source: String,
    val content: String,
    val score: Double
)

data class SyncStatus(
    val lastSync: String?,
    val pendingOps: Int,
    val connected: Boolean
)

// MARK: - Engine Singleton

object PrometheusEngine {
    private var api: PrometheusApi? = null
    private var engine: EngineHandle? = null

    @Synchronized
    fun initialize() {
        if (engine != null) return
        val a = PrometheusApi()
        api = a
        engine = a.createEngine()
    }

    private fun getEngine(): EngineHandle {
        return engine ?: throw IllegalStateException("引擎未初始化")
    }

    // Chat
    fun sendMessage(message: String): String = getEngine().sendMessage(message)

    fun getConversation(): Conversation {
        val ffi = getEngine().getConversation()
        return Conversation(
            id = ffi.id,
            title = ffi.title,
            messages = ffi.messages.map { msg ->
                ChatMessage(
                    role = msg.role,
                    content = msg.content,
                    isUser = msg.role == "user",
                    timestamp = msg.timestampMs
                )
            }
        )
    }

    // Models
    fun listModels(): List<ModelInfo> {
        return api?.listModelInfo()?.map { info ->
            ModelInfo(id = info.id, provider = info.provider, isLocal = info.isLocal)
        } ?: emptyList()
    }

    fun getActiveModel(): String = getEngine().getActiveModel()
    fun switchModel(modelId: String): Boolean = getEngine().setActiveModel(modelId)

    // Memory
    fun addWorkingMemory(content: String, importance: Double): Boolean =
        getEngine().addWorkingMemory(content, importance)

    fun getWorkingMemory(maxItems: UInt = 20u): List<FfiMemoryItem> =
        getEngine().getWorkingMemory(maxItems)

    fun workingMemoryCount(): UInt = getEngine().workingMemoryCount()

    fun addKnowledge(source: String, content: String): Int =
        getEngine().addKnowledge(source, content)

    fun searchKnowledge(query: String, topK: UInt = 5u): List<KnowledgeResult> =
        getEngine().searchKnowledge(query, topK).map { r ->
            KnowledgeResult(source = r.source, content = r.content, score = r.score)
        }

    fun knowledgeCount(): UInt = getEngine().knowledgeCount()

    fun searchEpisodes(query: String, topK: UInt = 5u): List<String> =
        getEngine().searchEpisodes(query, topK)

    // IoT
    fun scanDevices(): List<DeviceInfo> =
        getEngine().scanIotDevices().map { d ->
            DeviceInfo(id = d.id, name = d.name, deviceType = d.deviceType)
        }

    fun controlDevice(id: String, command: String, params: String = "{}"): String =
        getEngine().controlDevice(id, command, params)

    // Persona
    fun listPersonas(): List<PersonaInfo> =
        getEngine().listPersonas().map { p ->
            PersonaInfo(id = p.id, name = p.name, tone = p.tone)
        }

    fun switchPersona(id: String): Boolean = getEngine().switchPersona(id)

    // Sync
    fun getSyncStatus(): SyncStatus {
        val json = getEngine().getSyncStatus()
        return try {
            val obj = JSONObject(json)
            SyncStatus(
                lastSync = obj.optString("last_sync", null),
                pendingOps = obj.optInt("pending_ops", 0),
                connected = obj.optBoolean("connected", false)
            )
        } catch (_: Exception) {
            SyncStatus(lastSync = null, pendingOps = 0, connected = false)
        }
    }

    // Lifecycle
    fun pause(): Boolean = getEngine().pause()
    fun resume(): Boolean = getEngine().resume()
    fun shutdown(): Boolean = getEngine().shutdown()

    // Privacy
    fun exportData(): String = getEngine().exportUserData()
    fun deleteData(): Boolean = getEngine().deleteUserData()
    fun recordConsent(purpose: String, granted: Boolean): Boolean =
        getEngine().recordConsent(purpose, granted)

    // Settings
    fun getSetting(key: String): String = getEngine().getSetting(key)
    fun setSetting(key: String, value: String): Boolean = getEngine().setSetting(key, value)
}

