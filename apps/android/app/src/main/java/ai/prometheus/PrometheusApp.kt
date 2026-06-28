package ai.prometheus

import android.app.Application
import ai.prometheus.bridge.PrometheusEngine

class PrometheusApp : Application() {
    override fun onCreate() {
        super.onCreate()
        try {
            PrometheusEngine.initialize()
        } catch (e: Exception) {
            android.util.Log.e("PrometheusApp", "引擎初始化失败", e)
        }
    }
}
