package ai.prometheus.ui.navigation

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.navigation.NavDestination.Companion.hierarchy
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import ai.prometheus.ui.screen.*

enum class AppScreen(val route: String, val label: String, val icon: ImageVector) {
    Chat("chat", "聊天", Icons.Default.Chat),
    IoT("iot", "IoT", Icons.Default.Sensors),
    Memory("memory", "记忆", Icons.Default.Psychology),
    Settings("settings", "设置", Icons.Default.Settings)
}

@Composable
fun AppNavigation() {
    val navController = rememberNavController()
    val navBackStackEntry by navController.currentBackStackEntryAsState()
    val currentDestination = navBackStackEntry?.destination

    Scaffold(
        bottomBar = {
            NavigationBar {
                AppScreen.entries.forEach { screen ->
                    NavigationBarItem(
                        icon = { Icon(screen.icon, contentDescription = screen.label) },
                        label = { Text(screen.label) },
                        selected = currentDestination?.hierarchy?.any { it.route == screen.route } == true,
                        onClick = {
                            navController.navigate(screen.route) {
                                popUpTo(navController.graph.findStartDestination().id) {
                                    saveState = true
                                }
                                launchSingleTop = true
                                restoreState = true
                            }
                        }
                    )
                }
            }
        }
    ) { innerPadding ->
        NavHost(
            navController = navController,
            startDestination = AppScreen.Chat.route,
            modifier = Modifier.padding(innerPadding)
        ) {
            composable(AppScreen.Chat.route) { ChatScreen() }
            composable(AppScreen.IoT.route) { IoTScreen() }
            composable(AppScreen.Memory.route) { MemoryScreen() }
            composable(AppScreen.Settings.route) {
                SettingsScreen(
                    onNavigateToPersona = { navController.navigate("persona") }
                )
            }
            composable("persona") { PersonaScreen() }
        }
    }
}
