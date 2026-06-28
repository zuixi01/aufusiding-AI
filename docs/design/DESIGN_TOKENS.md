# Prometheus AI Design Tokens

## 一、概述

本文档定义了 Prometheus AI 的设计 Token，作为视觉设计的唯一事实来源（Single Source of Truth）。覆盖颜色、字体、间距、动效、阴影、圆角六大类别，提供 CSS、SwiftUI、Jetpack Compose 三平台可直接复制使用的代码。

设计语言以「环境感知」为核心，色彩取自自然材质——奶油、赤陶、鼠尾草、砂岩。支持 Light / Dark 双主题及 Home、Work、Sleep、Driving、Focus 五种场景模式。

---

## 二、颜色 Token

### 2.1 CSS Custom Properties

```css
:root {
  --ambient-cream: #FDF6EC;
  --ambient-warm-white: #FAF3E8;
  --ambient-sand: #E8DCC8;
  --ambient-gold: #D4A853;
  --ambient-terracotta: #C4704B;
  --ambient-ember: #E8734A;
  --ambient-dawn: #F0C27A;
  --ambient-sage: #8B9E7E;
  --ambient-clay: #B8856C;
  --ambient-dusk: #7B6B8A;
  --ambient-charcoal: #3D3229;
  --ambient-walnut: #6B5B4E;
  --ambient-stone: #9E8E80;
}

[data-theme="dark"] {
  --ambient-cream: #1A1410;
  --ambient-warm-white: #231D17;
  --ambient-sand: #2E261F;
  --ambient-gold: #E8B95A;
  --ambient-terracotta: #D4805A;
  --ambient-ember: #F08A5E;
  --ambient-dawn: #F0C27A;
  --ambient-sage: #9DB08E;
  --ambient-clay: #5A4A3E;
  --ambient-dusk: #9B8BAA;
  --ambient-charcoal: #E8DCC8;
  --ambient-walnut: #B8A898;
  --ambient-stone: #7A6E64;
}

[data-mode="home"]    { --mode-accent: #D4A853; --mode-glow: rgba(212,168,83,0.15); }
[data-mode="work"]    { --mode-accent: #7B8E6B; --mode-glow: rgba(123,142,107,0.12); }
[data-mode="sleep"]   { --mode-accent: #4A3E5C; --mode-glow: rgba(74,62,92,0.08); }
[data-mode="driving"] { --mode-accent: #E8734A; --mode-glow: rgba(232,115,74,0.18); }
[data-mode="focus"]   { --mode-accent: #8B7355; --mode-glow: rgba(139,115,85,0.10); }
```

### 2.2 SwiftUI

```swift
import SwiftUI

extension Color {
    static let ambientCream      = Color(hex: "FDF6EC")
    static let ambientWarmWhite  = Color(hex: "FAF3E8")
    static let ambientSand       = Color(hex: "E8DCC8")
    static let ambientGold       = Color(hex: "D4A853")
    static let ambientTerracotta = Color(hex: "C4704B")
    static let ambientEmber      = Color(hex: "E8734A")
    static let ambientDawn       = Color(hex: "F0C27A")
    static let ambientSage       = Color(hex: "8B9E7E")
    static let ambientClay       = Color(hex: "B8856C")
    static let ambientDusk       = Color(hex: "7B6B8A")
    static let ambientCharcoal   = Color(hex: "3D3229")
    static let ambientWalnut     = Color(hex: "6B5B4E")
    static let ambientStone      = Color(hex: "9E8E80")
}

struct PrometheusColorsDark {
    static let ambientCream      = Color(hex: "1A1410")
    static let ambientWarmWhite  = Color(hex: "231D17")
    static let ambientSand       = Color(hex: "2E261F")
    static let ambientGold       = Color(hex: "E8B95A")
    static let ambientTerracotta = Color(hex: "D4805A")
    static let ambientEmber      = Color(hex: "F08A5E")
    static let ambientDawn       = Color(hex: "F0C27A")
    static let ambientSage       = Color(hex: "9DB08E")
    static let ambientClay       = Color(hex: "5A4A3E")
    static let ambientDusk       = Color(hex: "9B8BAA")
    static let ambientCharcoal   = Color(hex: "E8DCC8")
    static let ambientWalnut     = Color(hex: "B8A898")
    static let ambientStone      = Color(hex: "7A6E64")
}

enum PrometheusMode {
    case home, work, sleep, driving, focus
    var accent: Color {
        switch self {
        case .home:    return Color(hex: "D4A853")
        case .work:    return Color(hex: "7B8E6B")
        case .sleep:   return Color(hex: "4A3E5C")
        case .driving: return Color(hex: "E8734A")
        case .focus:   return Color(hex: "8B7355")
        }
    }
    var glow: Color {
        switch self {
        case .home:    return Color(hex: "D4A853").opacity(0.15)
        case .work:    return Color(hex: "7B8E6B").opacity(0.12)
        case .sleep:   return Color(hex: "4A3E5C").opacity(0.08)
        case .driving: return Color(hex: "E8734A").opacity(0.18)
        case .focus:   return Color(hex: "8B7355").opacity(0.10)
        }
    }
}
```

### 2.3 Jetpack Compose

```kotlin
import androidx.compose.ui.graphics.Color

object PrometheusColors {
    val ambientCream      = Color(0xFFFDF6EC)
    val ambientWarmWhite  = Color(0xFFFAF3E8)
    val ambientSand       = Color(0xFFE8DCC8)
    val ambientGold       = Color(0xFFD4A853)
    val ambientTerracotta = Color(0xFFC4704B)
    val ambientEmber      = Color(0xFFE8734A)
    val ambientDawn       = Color(0xFFF0C27A)
    val ambientSage       = Color(0xFF8B9E7E)
    val ambientClay       = Color(0xFFB8856C)
    val ambientDusk       = Color(0xFF7B6B8A)
    val ambientCharcoal   = Color(0xFF3D3229)
    val ambientWalnut     = Color(0xFF6B5B4E)
    val ambientStone      = Color(0xFF9E8E80)
}

object PrometheusColorsDark {
    val ambientCream      = Color(0xFF1A1410)
    val ambientWarmWhite  = Color(0xFF231D17)
    val ambientSand       = Color(0xFF2E261F)
    val ambientGold       = Color(0xFFE8B95A)
    val ambientTerracotta = Color(0xFFD4805A)
    val ambientEmber      = Color(0xFFF08A5E)
    val ambientDawn       = Color(0xFFF0C27A)
    val ambientSage       = Color(0xFF9DB08E)
    val ambientClay       = Color(0xFF5A4A3E)
    val ambientDusk       = Color(0xFF9B8BAA)
    val ambientCharcoal   = Color(0xFFE8DCC8)
    val ambientWalnut     = Color(0xFFB8A898)
    val ambientStone      = Color(0xFF7A6E64)
}

enum class PrometheusMode { Home, Work, Sleep, Driving, Focus }

fun PrometheusMode.accent(): Color = when (this) {
    PrometheusMode.Home    -> Color(0xFFD4A853)
    PrometheusMode.Work    -> Color(0xFF7B8E6B)
    PrometheusMode.Sleep   -> Color(0xFF4A3E5C)
    PrometheusMode.Driving -> Color(0xFFE8734A)
    PrometheusMode.Focus   -> Color(0xFF8B7355)
}

fun PrometheusMode.glow(): Color = when (this) {
    PrometheusMode.Home    -> Color(0xFFD4A853).copy(alpha = 0.15f)
    PrometheusMode.Work    -> Color(0xFF7B8E6B).copy(alpha = 0.12f)
    PrometheusMode.Sleep   -> Color(0xFF4A3E5C).copy(alpha = 0.08f)
    PrometheusMode.Driving -> Color(0xFFE8734A).copy(alpha = 0.18f)
    PrometheusMode.Focus   -> Color(0xFF8B7355).copy(alpha = 0.10f)
}
```

---

## 三、字体 Token

基于 Inter 字族，7 个层级从环境展示到标注文字。

### 3.1 CSS

```css
:root {
  --font-family-primary: 'Inter', system-ui, -apple-system, sans-serif;
  --font-family-mono: 'JetBrains Mono', 'Fira Code', monospace;

  --type-ambient:    200 56px/1.0  var(--font-family-primary); /* letter-spacing: -0.02em */
  --type-display:    300 34px/1.15 var(--font-family-primary); /* letter-spacing: -0.01em */
  --type-heading:    500 24px/1.25 var(--font-family-primary); /* letter-spacing: -0.005em */
  --type-subheading: 500 18px/1.35 var(--font-family-primary);
  --type-body:       400 16px/1.5  var(--font-family-primary);
  --type-caption:    400 13px/1.4  var(--font-family-primary); /* letter-spacing: 0.01em */
  --type-overline:   600 11px/1.4  var(--font-family-primary); /* letter-spacing: 0.08em; text-transform: uppercase */
}
```

### 3.2 SwiftUI

```swift
extension Font {
    static let prometheusAmbient    = Font.system(size: 56, weight: .ultraLight)
    static let prometheusDisplay    = Font.system(size: 34, weight: .light)
    static let prometheusHeading    = Font.system(size: 24, weight: .medium)
    static let prometheusSubheading = Font.system(size: 18, weight: .medium)
    static let prometheusBody       = Font.system(size: 16, weight: .regular)
    static let prometheusCaption    = Font.system(size: 13, weight: .regular)
    static let prometheusOverline   = Font.system(size: 11, weight: .semibold)
}

extension View {
    func prometheusAmbientStyle() -> some View {
        self.font(.prometheusAmbient).tracking(-1.12)
    }
    func prometheusOverlineStyle() -> some View {
        self.font(.prometheusOverline).tracking(0.88).textCase(.uppercase)
    }
}
```

### 3.3 Jetpack Compose

```kotlin
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

object PrometheusTypography {
    val ambient = TextStyle(fontSize = 56.sp, fontWeight = FontWeight.W200, lineHeight = 56.sp, letterSpacing = (-0.02).sp)
    val display = TextStyle(fontSize = 34.sp, fontWeight = FontWeight.W300, lineHeight = 39.sp, letterSpacing = (-0.01).sp)
    val heading = TextStyle(fontSize = 24.sp, fontWeight = FontWeight.W500, lineHeight = 30.sp, letterSpacing = (-0.005).sp)
    val subheading = TextStyle(fontSize = 18.sp, fontWeight = FontWeight.W500, lineHeight = 24.sp)
    val body = TextStyle(fontSize = 16.sp, fontWeight = FontWeight.W400, lineHeight = 24.sp)
    val caption = TextStyle(fontSize = 13.sp, fontWeight = FontWeight.W400, lineHeight = 18.sp, letterSpacing = 0.01.sp)
    val overline = TextStyle(fontSize = 11.sp, fontWeight = FontWeight.W600, lineHeight = 15.sp, letterSpacing = 0.08.sp)
}
```

---

## 四、间距 Token

基于 8px 基准网格，所有 padding / margin / gap 使用以下 Token。

### 4.1 CSS

```css
:root {
  --space-xxs: 4px;   /* 半格 */
  --space-xs: 8px;    /* 1 格 */
  --space-sm: 12px;   /* 1.5 格 */
  --space-md: 16px;   /* 2 格 */
  --space-lg: 24px;   /* 3 格 */
  --space-xl: 32px;   /* 4 格 */
  --space-2xl: 48px;  /* 6 格 */
  --space-3xl: 64px;  /* 8 格 */
  --space-4xl: 80px;  /* 10 格 */
}
```

### 4.2 SwiftUI

```swift
enum PrometheusSpacing {
    static let xxs: CGFloat  = 4
    static let xs: CGFloat   = 8
    static let sm: CGFloat   = 12
    static let md: CGFloat   = 16
    static let lg: CGFloat   = 24
    static let xl: CGFloat   = 32
    static let xxl: CGFloat  = 48
    static let xxxl: CGFloat = 64
    static let xxxxl: CGFloat = 80
}
```

### 4.3 Jetpack Compose

```kotlin
import androidx.compose.ui.unit.dp

object PrometheusSpacing {
    val xxs  = 4.dp
    val xs   = 8.dp
    val sm   = 12.dp
    val md   = 16.dp
    val lg   = 24.dp
    val xl   = 32.dp
    val xxl  = 48.dp
    val xxxl = 64.dp
    val xxxxl = 80.dp
}
```

---

## 五、动效 Token

遵循「自然呼吸」原则，界面过渡平滑自然。

### 5.1 CSS

```css
:root {
  --duration-instant: 100ms;
  --duration-fast: 200ms;
  --duration-normal: 350ms;
  --duration-slow: 500ms;
  --duration-ambient: 1200ms;

  --ease-default: cubic-bezier(0.25, 0.1, 0.25, 1.0);
  --ease-in: cubic-bezier(0.42, 0, 1.0, 1.0);
  --ease-out: cubic-bezier(0, 0, 0.58, 1.0);
  --ease-in-out: cubic-bezier(0.42, 0, 0.58, 1.0);
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1.0);
  --ease-ambient: cubic-bezier(0.16, 0.68, 0.43, 0.99);

  --transition-fast: var(--duration-fast) var(--ease-default);
  --transition-normal: var(--duration-normal) var(--ease-in-out);
  --transition-slow: var(--duration-slow) var(--ease-out);
  --transition-ambient: var(--duration-ambient) var(--ease-ambient);
}
```

### 5.2 SwiftUI

```swift
enum PrometheusAnimation {
    static let durationInstant: Double = 0.1
    static let durationFast: Double    = 0.2
    static let durationNormal: Double  = 0.35
    static let durationSlow: Double    = 0.5
    static let durationAmbient: Double = 1.2

    static let instant = Animation.easeOut(duration: durationInstant)
    static let fast    = Animation.easeOut(duration: durationFast)
    static let normal  = Animation.easeInOut(duration: durationNormal)
    static let slow    = Animation.easeOut(duration: durationSlow)
    static let ambient = Animation.timingCurve(0.16, 0.68, 0.43, 0.99, duration: durationAmbient)
    static let spring  = Animation.spring(response: 0.35, dampingFraction: 0.7, blendDuration: 0)
}
```

### 5.3 Jetpack Compose

```kotlin
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.Spring

object PrometheusMotion {
    const val DURATION_INSTANT = 100
    const val DURATION_FAST    = 200
    const val DURATION_NORMAL  = 350
    const val DURATION_SLOW    = 500
    const val DURATION_AMBIENT = 1200

    val easeDefault = CubicBezierEasing(0.25f, 0.1f, 0.25f, 1.0f)
    val easeIn      = CubicBezierEasing(0.42f, 0f, 1.0f, 1.0f)
    val easeOut     = CubicBezierEasing(0f, 0f, 0.58f, 1.0f)
    val easeInOut   = CubicBezierEasing(0.42f, 0f, 0.58f, 1.0f)
    val easeSpring  = CubicBezierEasing(0.34f, 1.56f, 0.64f, 1.0f)
    val easeAmbient = CubicBezierEasing(0.16f, 0.68f, 0.43f, 0.99f)

    fun <T> tweenFast()    = tween<T>(DURATION_FAST, easing = easeDefault)
    fun <T> tweenNormal()  = tween<T>(DURATION_NORMAL, easing = easeInOut)
    fun <T> tweenSlow()    = tween<T>(DURATION_SLOW, easing = easeOut)
    fun <T> tweenAmbient() = tween<T>(DURATION_AMBIENT, easing = easeAmbient)
    fun <T> springDefault() = spring<T>(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium)
}
```

---

## 六、阴影 Token

四个层级，Dark Mode 下加深以保持视觉层次。

### 6.1 CSS

```css
:root {
  --shadow-xs: 0 1px 2px rgba(61, 50, 41, 0.06);
  --shadow-sm: 0 2px 8px rgba(61, 50, 41, 0.08);
  --shadow-md: 0 4px 16px rgba(61, 50, 41, 0.10);
  --shadow-lg: 0 8px 32px rgba(61, 50, 41, 0.14);
  --shadow-glow: 0 0 24px var(--mode-glow, rgba(212, 168, 83, 0.15));
}
[data-theme="dark"] {
  --shadow-xs: 0 1px 2px rgba(0, 0, 0, 0.20);
  --shadow-sm: 0 2px 8px rgba(0, 0, 0, 0.25);
  --shadow-md: 0 4px 16px rgba(0, 0, 0, 0.30);
  --shadow-lg: 0 8px 32px rgba(0, 0, 0, 0.40);
  --shadow-glow: 0 0 32px var(--mode-glow, rgba(232, 185, 90, 0.12));
}
```

### 6.2 SwiftUI

```swift
enum PrometheusShadow {
    struct Level { let color: Color; let radius: CGFloat; let x: CGFloat; let y: CGFloat }
    static let xs = Level(color: .black.opacity(0.06), radius: 1, x: 0, y: 1)
    static let sm = Level(color: .black.opacity(0.08), radius: 4, x: 0, y: 2)
    static let md = Level(color: .black.opacity(0.10), radius: 8, x: 0, y: 4)
    static let lg = Level(color: .black.opacity(0.14), radius: 16, x: 0, y: 8)
}

extension View {
    func prometheusShadow(_ level: PrometheusShadow.Level) -> some View {
        self.shadow(color: level.color, radius: level.radius, x: level.x, y: level.y)
    }
}
```

### 6.3 Jetpack Compose

```kotlin
object PrometheusShadow {
    val xs = 1.dp
    val sm = 4.dp
    val md = 8.dp
    val lg = 16.dp
}
```

---

## 七、圆角 Token

小组件用小圆角，大容器用大圆角，保持视觉比例协调。

### 7.1 CSS

```css
:root {
  --radius-xs: 4px;      /* 标签、徽章 */
  --radius-sm: 8px;      /* 按钮、输入框 */
  --radius-md: 12px;     /* 卡片、下拉菜单 */
  --radius-lg: 16px;     /* 对话框、面板 */
  --radius-xl: 24px;     /* 大卡片、浮层 */
  --radius-full: 9999px; /* 药丸形、头像 */
}
```

### 7.2 SwiftUI

```swift
enum PrometheusRadius {
    static let xs: CGFloat   = 4
    static let sm: CGFloat   = 8
    static let md: CGFloat   = 12
    static let lg: CGFloat   = 16
    static let xl: CGFloat   = 24
    static let full: CGFloat = 9999
}

extension View {
    func prometheusCorners(_ radius: CGFloat = PrometheusRadius.md) -> some View {
        self.clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
    }
}
```

### 7.3 Jetpack Compose

```kotlin
import androidx.compose.foundation.shape.RoundedCornerShape

object PrometheusRadius {
    val xs   = 4.dp
    val sm   = 8.dp
    val md   = 12.dp
    val lg   = 16.dp
    val xl   = 24.dp
    val full = 9999.dp
}

object PrometheusShapes {
    val xs   = RoundedCornerShape(PrometheusRadius.xs)
    val sm   = RoundedCornerShape(PrometheusRadius.sm)
    val md   = RoundedCornerShape(PrometheusRadius.md)
    val lg   = RoundedCornerShape(PrometheusRadius.lg)
    val xl   = RoundedCornerShape(PrometheusRadius.xl)
    val full = RoundedCornerShape(PrometheusRadius.full)
}
```

---

## 7.5 图标 Token

Prometheus 使用 **Phosphor Icons** 图标库的 **duotone**（双色调）风格。

### 图标库信息

```
库名称:         Phosphor Icons
风格:           Duotone（主色 + 辅色半透明层）
官网:           https://phosphoricons.com
CDN (Web):      https://cdn.jsdelivr.net/npm/@phosphor-icons/web@2/src/duotone/style.css
Swift Package:  phosphor-icons/swift (SPM)
Compose:        com.phosphoricons:phosphor-android
```

### 7.5.1 Web (CSS)

```css
/* CDN 引入 */
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@phosphor-icons/web@2/src/duotone/style.css">

/* 使用方式 */
<i class="ph-duotone ph-robot" style="font-size: 24px; color: var(--ambient-gold);"></i>

/* Duotone 辅色透明度 */
.ph-duotone {
  --ph-duotone-opacity: 0.3;
}
```

### 7.5.2 尺寸规范

```
icon-xs:    16px    /* 状态栏指示器、内联标记 */
icon-sm:    18px    /* 列表项图标、返回按钮 */
icon-md:    20px    /* 默认尺寸、设备图标、轨道 App 图标 */
icon-lg:    24px    /* Dock 栏、媒体控制、强调图标 */
icon-xl:    28px    /* 语音按钮、大触控目标 */
```

### 7.5.3 核心 App 图标映射

```
Agent 集群:    ph-robot
语音交互:      ph-microphone
普罗万物 IoT:  ph-house
记忆系统:      ph-brain
安全隐私:      ph-shield-check
插件系统:      ph-puzzle-piece
工具链:        ph-toolbox
智能路由:      ph-signpost
```

### 7.5.4 颜色规则

```
默认:     color: var(--ambient-walnut)       /* 中性暖灰 */
选中:     color: var(--ambient-gold)         /* 金色高亮 */
模式跟随: color: var(--mode-accent)          /* 跟随当前模式强调色 */
禁用:     color: var(--ambient-stone)        /* opacity: 0.4 */
```

---

## 八、使用指南

### 8.1 CSS -- 场景模式卡片

```css
.mode-card {
  background: var(--ambient-warm-white);
  color: var(--ambient-charcoal);
  font: var(--type-body);
  padding: var(--space-md) var(--space-lg);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
  transition: box-shadow var(--transition-normal), transform var(--transition-normal);
}
.mode-card:hover {
  box-shadow: var(--shadow-md);
  transform: translateY(-2px);
}
.mode-card__title {
  font: var(--type-subheading);
  color: var(--mode-accent);
}
```

### 8.2 SwiftUI -- 环境温度显示

```swift
struct AmbientTemperatureView: View {
    let temperature: String

    var body: some View {
        VStack(spacing: PrometheusSpacing.xs) {
            Text(temperature)
                .prometheusAmbientStyle()
                .foregroundColor(.ambientCharcoal)
            Text("室内温度")
                .font(.prometheusCaption)
                .foregroundColor(.ambientStone)
        }
        .padding(PrometheusSpacing.lg)
        .background(Color.ambientWarmWhite)
        .prometheusCorners(PrometheusRadius.lg)
        .prometheusShadow(PrometheusShadow.sm)
    }
}
```

### 8.3 Compose -- 模式切换按钮

```kotlin
@Composable
fun ModeButton(mode: PrometheusMode, label: String, isActive: Boolean, onClick: () -> Unit) {
    val bg = if (isActive) mode.accent().copy(alpha = 0.12f) else PrometheusColors.ambientWarmWhite
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier
            .shadow(if (isActive) PrometheusShadow.md else PrometheusShadow.xs, PrometheusShapes.md)
            .background(bg, PrometheusShapes.md)
            .clickable(onClick = onClick)
            .padding(horizontal = PrometheusSpacing.lg, vertical = PrometheusSpacing.md)
    ) {
        Text(label, style = PrometheusTypography.subheading, color = if (isActive) mode.accent() else PrometheusColors.ambientWalnut)
        Text(mode.name, style = PrometheusTypography.caption, color = PrometheusColors.ambientStone)
    }
}
```

### 8.4 Token 命名规则

| 前缀 | 含义 | 示例 |
|------|------|------|
| `ambient-` | 颜色 | `--ambient-gold` |
| `type-` | 字体 | `--type-body` |
| `space-` | 间距 | `--space-md` |
| `duration-` / `ease-` | 动效 | `--duration-fast` |
| `shadow-` | 阴影 | `--shadow-md` |
| `radius-` | 圆角 | `--radius-lg` |
| `mode-` | 场景模式 | `--mode-accent` |

### 8.5 平台映射速查

| CSS | SwiftUI | Compose |
|-----|---------|---------|
| `--ambient-gold` | `Color.ambientGold` | `PrometheusColors.ambientGold` |
| `--type-body` | `.prometheusBody` | `PrometheusTypography.body` |
| `--space-md` | `PrometheusSpacing.md` | `PrometheusSpacing.md` |
| `--duration-normal` | `PrometheusAnimation.durationNormal` | `PrometheusMotion.DURATION_NORMAL` |
| `--shadow-md` | `PrometheusShadow.md` | `PrometheusShadow.md` |
| `--radius-md` | `PrometheusRadius.md` | `PrometheusRadius.md` |

---

> 本文档由 Prometheus AI 设计系统维护。Token 变更须同步更新三平台定义，经设计评审后方可合入主分支。