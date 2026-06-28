package ai.prometheus.ui.component

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreHoriz
import androidx.compose.material.icons.filled.VolumeUp
import androidx.compose.material.icons.filled.GraphicEq
import androidx.compose.material3.Icon
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp

enum class VoiceState {
    Idle, Listening, Processing, Responding
}

@Composable
fun VoiceOrb(
    modifier: Modifier = Modifier,
    onVoiceResult: (String) -> Unit = {}
) {
    var state by remember { mutableStateOf(VoiceState.Idle) }

    val infiniteTransition = rememberInfiniteTransition(label = "pulse")
    val pulseScale by infiniteTransition.animateFloat(
        initialValue = 1f,
        targetValue = if (state == VoiceState.Listening) 1.15f else 1f,
        animationSpec = infiniteRepeatable(
            animation = tween(1000, easing = EaseInOut),
            repeatMode = RepeatMode.Reverse
        ),
        label = "scale"
    )

    val primaryColor by animateColorAsState(
        targetValue = when (state) {
            VoiceState.Idle -> Color(0xFF1976D2)
            VoiceState.Listening -> Color(0xFF4CAF50)
            VoiceState.Processing -> Color(0xFFFF9800)
            VoiceState.Responding -> Color(0xFF9C27B0)
        },
        label = "color"
    )

    val secondaryColor by animateColorAsState(
        targetValue = when (state) {
            VoiceState.Idle -> Color(0xFF7B1FA2)
            VoiceState.Listening -> Color(0xFF00BCD4)
            VoiceState.Processing -> Color(0xFFFFEB3B)
            VoiceState.Responding -> Color(0xFFE91E63)
        },
        label = "color2"
    )

    Box(
        modifier = modifier.size(100.dp),
        contentAlignment = Alignment.Center
    ) {
        Canvas(
            modifier = Modifier
                .size(100.dp)
                .pointerInput(Unit) {
                    detectTapGestures(
                        onPress = {
                            state = VoiceState.Listening
                            tryAwaitRelease()
                            if (state == VoiceState.Listening) {
                                state = VoiceState.Processing
                                // Simulate voice processing
                                kotlinx.coroutines.delay(1000)
                                state = VoiceState.Responding
                                onVoiceResult("(语音输入)")
                                kotlinx.coroutines.delay(500)
                                state = VoiceState.Idle
                            }
                        },
                        onTap = {
                            if (state != VoiceState.Idle) {
                                state = VoiceState.Idle
                            }
                        }
                    )
                }
        ) {
            val radius = size.minDimension / 2 * pulseScale
            drawCircle(
                brush = Brush.linearGradient(
                    colors = listOf(primaryColor, secondaryColor),
                    start = Offset.Zero,
                    end = Offset(size.width, size.height)
                ),
                radius = radius
            )
        }

        Icon(
            imageVector = when (state) {
                VoiceState.Idle -> Icons.Default.GraphicEq
                VoiceState.Listening -> Icons.Default.Mic
                VoiceState.Processing -> Icons.Default.MoreHoriz
                VoiceState.Responding -> Icons.Default.VolumeUp
            },
            contentDescription = "语音",
            tint = Color.White,
            modifier = Modifier.size(36.dp)
        )
    }
}
