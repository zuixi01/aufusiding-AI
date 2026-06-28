package ai.prometheus

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import ai.prometheus.ui.navigation.AppNavigation
import ai.prometheus.ui.theme.PrometheusTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            PrometheusTheme {
                AppNavigation()
            }
        }
    }
}
