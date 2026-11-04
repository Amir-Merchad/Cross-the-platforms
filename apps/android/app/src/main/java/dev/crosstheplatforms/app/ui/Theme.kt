package dev.crosstheplatforms.app.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import dev.crosstheplatforms.app.R

/** Tokens from the Foundations board (dark / light). The action colours are the same in both. */
data class Palette(
    val bg: Color, val surface: Color, val ink: Color, val muted: Color, val line: Color, val tray: Color,
    val screen: Color, val danger: Color, val ok: Color, val warnBg: Color, val warnInk: Color,
) {
    val action = Color(0xFFFFC531)
    val actionLip = Color(0xFFD79A00)
    val onAction = Color(0xFF10202E) // text on the primary colour
    val onScreen = Color(0xFFEEF3F8) // text on the PC card, which is dark in both themes
    val screenLip = Color(0xFF0D141C)
}

val DarkPalette = Palette(Color(0xFF11171E), Color(0xFF1B232D), Color(0xFFEEF3F8), Color(0xFFA9B7C7), Color(0xFF2E3A47), Color(0xFF18202A),
    Color(0xFF2A3644), Color(0xFFFF9A8F), Color(0xFF5FD3A8), Color(0xFF3B2F14), Color(0xFFFFD978))
val LightPalette = Palette(Color(0xFFE9EFF7), Color(0xFFFFFFFF), Color(0xFF14202E), Color(0xFF526274), Color(0xFFD3DCE8), Color(0xFFDCE4EF),
    Color(0xFF1C2836), Color(0xFFC4281C), Color(0xFF0B7A55), Color(0xFFFFF1CC), Color(0xFF6B4A00))

val LocalPalette = staticCompositionLocalOf { DarkPalette }

@OptIn(ExperimentalTextApi::class)
private fun rubik(w: Int) = Font(R.font.rubik, FontWeight(w), variationSettings = FontVariation.Settings(FontVariation.weight(w)))

/** Rubik, one variable file, at the four weights the design uses. */
val Rubik = FontFamily(rubik(400), rubik(500), rubik(600), rubik(700))

/** Follows the system light or dark setting. Every Material Text picks up Rubik through the typography. */
@Composable
fun BridgeTheme(content: @Composable () -> Unit) {
    val p = if (isSystemInDarkTheme()) DarkPalette else LightPalette
    val base = Typography()
    val rubikStyles = Typography(
        displayLarge = base.displayLarge.rubik(), displayMedium = base.displayMedium.rubik(), displaySmall = base.displaySmall.rubik(),
        headlineLarge = base.headlineLarge.rubik(), headlineMedium = base.headlineMedium.rubik(), headlineSmall = base.headlineSmall.rubik(),
        titleLarge = base.titleLarge.rubik(), titleMedium = base.titleMedium.rubik(), titleSmall = base.titleSmall.rubik(),
        bodyLarge = base.bodyLarge.rubik(), bodyMedium = base.bodyMedium.rubik(), bodySmall = base.bodySmall.rubik(),
        labelLarge = base.labelLarge.rubik(), labelMedium = base.labelMedium.rubik(), labelSmall = base.labelSmall.rubik(),
    )
    val scheme = (if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme())
        .copy(background = p.bg, surface = p.surface, onBackground = p.ink, onSurface = p.ink, primary = p.action, onPrimary = p.onAction)
    CompositionLocalProvider(LocalPalette provides p) { MaterialTheme(colorScheme = scheme, typography = rubikStyles, content = content) }
}

private fun TextStyle.rubik() = copy(fontFamily = Rubik)

// Type scale from Foundations: title 30 bold, heading 22 semibold, body 15/16 regular, caption 13/14.
val Title = TextStyle(fontFamily = Rubik, fontWeight = FontWeight.Bold, fontSize = 30.sp)
val Heading = TextStyle(fontFamily = Rubik, fontWeight = FontWeight.SemiBold, fontSize = 22.sp)
val Body = TextStyle(fontFamily = Rubik, fontWeight = FontWeight.Normal, fontSize = 16.sp, lineHeight = 23.sp)
val Caption = TextStyle(fontFamily = Rubik, fontWeight = FontWeight.Normal, fontSize = 14.sp)
