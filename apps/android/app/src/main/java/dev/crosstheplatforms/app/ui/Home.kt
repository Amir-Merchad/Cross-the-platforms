package dev.crosstheplatforms.app.ui

import android.Manifest
import android.content.pm.PackageManager
import android.view.View
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import com.google.zxing.BarcodeFormat
import com.journeyapps.barcodescanner.BarcodeCallback
import com.journeyapps.barcodescanner.DecoratedBarcodeView
import com.journeyapps.barcodescanner.DefaultDecoderFactory
import dev.crosstheplatforms.app.Row as TransferRow
import dev.crosstheplatforms.app.RowState
import dev.crosstheplatforms.app.alertLine
import dev.crosstheplatforms.app.displayName
import dev.crosstheplatforms.app.formatRate
import dev.crosstheplatforms.app.pcSub
import dev.crosstheplatforms.app.percent
import dev.crosstheplatforms.app.progressLine
import dev.crosstheplatforms.app.recent
import dev.crosstheplatforms.app.ringName
import dev.crosstheplatforms.app.shortResult
import dev.crosstheplatforms.app.statusLine
import kotlinx.coroutines.withTimeoutOrNull

/** How long Forget must be held. Letting go earlier cancels. */
const val HOLD_TO_FORGET_MS = 900L

private val Control = RoundedCornerShape(14.dp)

/** Primary (amber, 3 px lip) or secondary (tray) button. Sinks 2 px while pressed. */
@Composable
fun LipButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, primary: Boolean = true, height: Dp = 56.dp) {
    val p = LocalPalette.current
    val source = remember { MutableInteractionSource() }
    val pressed by source.collectIsPressedAsState()
    val fill = if (primary) p.action else p.tray
    val lip = if (primary) p.actionLip else p.line
    val sink = if (pressed) 2.dp else 0.dp
    Box(modifier.padding(bottom = 3.dp)) {
        Box(
            Modifier.fillMaxWidth().height(height).offset(y = sink)
                .drawBehind { drawRoundRect(lip, Offset(0f, (3.dp - sink).toPx()), size, CornerRadius(14.dp.toPx())) }
                .clip(Control).background(fill)
                .clickable(source, null, role = Role.Button, onClick = onClick),
            contentAlignment = Alignment.Center,
        ) { Text(text, style = Body.copy(fontWeight = FontWeight.SemiBold), color = if (primary) p.onAction else p.ink) }
    }
}

@Composable
fun TextAction(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    Box(modifier.height(44.dp).clip(Control).clickable(role = Role.Button, onClick = onClick).padding(horizontal = 16.dp), contentAlignment = Alignment.Center) {
        Text(text, style = Body.copy(fontWeight = FontWeight.Medium), color = LocalPalette.current.muted)
    }
}

/** Fires `onForget` once the finger has stayed down for HOLD_TO_FORGET_MS. A shorter press does nothing. */
@Composable
fun HoldToForget(label: String, onForget: () -> Unit, modifier: Modifier = Modifier) {
    val p = LocalPalette.current
    var held by remember { mutableStateOf(false) }
    val fill by animateFloatAsState(if (held) 1f else 0f, tween(if (held) HOLD_TO_FORGET_MS.toInt() else 150, easing = LinearEasing), label = "hold")
    Box(
        modifier.fillMaxWidth().height(44.dp).clip(RoundedCornerShape(12.dp)).border(2.dp, p.line, RoundedCornerShape(12.dp))
            .drawBehind { drawRect(p.danger.copy(alpha = 0.25f), size = size.copy(width = size.width * fill)) }
            .semantics {
                role = Role.Button
                // TalkBack users cannot hold, so the action is offered directly.
                customActions = listOf(CustomAccessibilityAction("Forget this PC") { onForget(); true })
            }
            .pointerInput(onForget) {
                awaitEachGesture {
                    awaitFirstDown()
                    held = true
                    val up = withTimeoutOrNull(HOLD_TO_FORGET_MS) { waitForUpOrCancellation() }
                    held = false
                    if (up == null) { // the timeout won: still down after the full hold
                        onForget()
                        waitForUpOrCancellation()
                    }
                }
            },
        contentAlignment = Alignment.Center,
    ) { Text(label, style = Body.copy(fontWeight = FontWeight.SemiBold), color = p.danger) }
}

@Composable
private fun Section(content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(LocalPalette.current.surface).padding(horizontal = 16.dp, vertical = 6.dp)) { content() }
}

@Composable
private fun Line(left: String, right: String, rightColor: androidx.compose.ui.graphics.Color, border: Boolean = true) {
    val p = LocalPalette.current
    Row(
        Modifier.fillMaxWidth().then(if (border) Modifier.drawBehind { drawRect(p.line, size = size.copy(height = 1.dp.toPx())) } else Modifier).padding(vertical = 10.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(left, style = Body, color = p.ink, modifier = Modifier.weight(1f, fill = false))
        Text(right, style = Caption, color = rightColor, modifier = Modifier.padding(start = 12.dp))
    }
}

/** The phone's home screen. States home, sending, receiving, notPaired and failed are this one function over `rows`. */
@Composable
fun HomeScreen(
    pcName: String?, // null when no PC is paired
    rows: List<TransferRow>,
    startFailed: Boolean,
    onSendFiles: () -> Unit,
    onSendClipboard: () -> Unit,
    onPair: () -> Unit,
    onForget: () -> Unit,
) {
    val p = LocalPalette.current
    val paired = pcName != null
    val moving = rows.lastOrNull { it.state == RowState.MOVING }
    val alert = if (startFailed) "Bridge could not start. Close it and open it again." else alertLine(rows)
    Column(
        Modifier.fillMaxSize().background(p.bg).systemBarsPadding().verticalScroll(rememberScrollState()).padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(18.dp),
    ) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text("Bridge", style = Title, color = p.ink)
            Text(statusLine(paired, rows), style = Caption, color = p.muted)
        }
        Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Column(
                Modifier.fillMaxWidth().height(155.dp).padding(bottom = 5.dp)
                    .drawBehind { drawRoundRect(p.screenLip, Offset(0f, 5.dp.toPx()), size, CornerRadius(14.dp.toPx())) }
                    .clip(Control).background(p.screen).padding(horizontal = 18.dp, vertical = 14.dp),
                verticalArrangement = Arrangement.Bottom,
            ) {
                Text(if (pcName != null) displayName(pcName) else "No PC yet", style = Body.copy(fontWeight = FontWeight.SemiBold), color = p.onScreen)
                Text(pcSub(paired, rows), style = Caption, color = p.onScreen.copy(alpha = 0.85f))
            }
            if (moving != null) Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(p.surface).padding(horizontal = 16.dp, vertical = 14.dp)) {
                val rate = formatRate(moving.done, moving.now - moving.startedAt)
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                    Text(ringName(moving), style = Body.copy(fontWeight = FontWeight.Medium), color = p.ink, modifier = Modifier.weight(1f, fill = false))
                    if (rate != "—") Text(rate, style = Body.copy(fontWeight = FontWeight.Medium), color = p.ink, modifier = Modifier.padding(start = 12.dp))
                }
                Box(Modifier.padding(top = 8.dp, bottom = 4.dp).fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(p.line)) {
                    Box(Modifier.fillMaxWidth(percent(moving) / 100f).height(6.dp).background(p.action))
                }
                Text(progressLine(moving), style = Caption, color = p.muted)
            }
            if (alert != null) Text(
                alert, style = Body.copy(fontWeight = FontWeight.Medium), color = p.warnInk,
                modifier = Modifier.fillMaxWidth().clip(Control).background(p.warnBg).padding(horizontal = 14.dp, vertical = 12.dp),
            )
            LipButton(if (paired) "Send files" else "Pair with PC", if (paired) onSendFiles else onPair)
            if (paired) LipButton("Send clipboard", onSendClipboard, primary = false)
        }
        if (paired) {
            Section {
                Text("Recent", style = Body.copy(fontWeight = FontWeight.SemiBold), color = p.ink, modifier = Modifier.padding(vertical = 8.dp))
                val shown = recent(rows)
                if (shown.isEmpty()) Line("Nothing yet", "", p.muted)
                shown.forEach { r -> Line(r.name, shortResult(r), if (r.state == RowState.FAILED) p.danger else p.muted) }
            }
            Section {
                Line("Received files", "Download/Bridge", p.muted, border = false)
                Line("Sharing from other apps", "Ready", p.muted)
            }
            HoldToForget("Hold to forget this PC", onForget)
        } else {
            Text("On the PC, open Bridge, choose Add a device, and show the code. Then scan it here.", style = Body, color = p.muted)
        }
    }
}

/** States pair and pairNoCamera. `message` is the progress, or why the last code did not work. */
@Composable
fun PairScreen(message: String?, attempt: Int, onCode: (String) -> Unit, onPaste: () -> Unit, onCancel: () -> Unit) {
    val p = LocalPalette.current
    val ctx = LocalContext.current
    var camera by remember { mutableStateOf(ContextCompat.checkSelfPermission(ctx, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { camera = it }
    DisposableEffect(Unit) { if (!camera) ask.launch(Manifest.permission.CAMERA); onDispose { } }
    Column(
        Modifier.fillMaxSize().background(p.bg).systemBarsPadding().padding(20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp, Alignment.CenterVertically), horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (camera) {
            Scanner(attempt, onCode, Modifier.size(280.dp).clip(RoundedCornerShape(22.dp)).border(3.dp, p.action, RoundedCornerShape(22.dp)))
            Text("Point the camera at the code on your PC", style = Heading, color = p.ink, textAlign = TextAlign.Center)
        } else {
            Text("The camera is off for Bridge", style = Heading, color = p.ink, textAlign = TextAlign.Center)
            Text("On the PC, press Copy code and get it to this device. Your clipboard sync can carry it. Then paste it here.", style = Body, color = p.muted, textAlign = TextAlign.Center)
        }
        message?.let { Text(it, style = Body.copy(fontWeight = FontWeight.Medium), color = p.warnInk, textAlign = TextAlign.Center) }
        LipButton("Paste code", onPaste, Modifier.padding(horizontal = 28.dp))
        TextAction("Cancel", onCancel)
    }
}

@Composable
private fun Scanner(attempt: Int, onCode: (String) -> Unit, modifier: Modifier) {
    val owner = LocalLifecycleOwner.current
    var view by remember { mutableStateOf<DecoratedBarcodeView?>(null) }
    DisposableEffect(owner, view) {
        val v = view
        val obs = LifecycleEventObserver { _, e -> if (e == Lifecycle.Event.ON_RESUME) v?.resume() else if (e == Lifecycle.Event.ON_PAUSE) v?.pause() }
        owner.lifecycle.addObserver(obs)
        if (owner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) v?.resume()
        onDispose { owner.lifecycle.removeObserver(obs); v?.pause() }
    }
    AndroidView(modifier = modifier, factory = { c ->
        DecoratedBarcodeView(c).also {
            it.barcodeView.decoderFactory = DefaultDecoderFactory(listOf(BarcodeFormat.QR_CODE))
            it.statusView.visibility = View.GONE
            it.viewFinder.visibility = View.GONE
            view = it
        }
    }, update = { v ->
        // `attempt` changes after a code that did not work, which arms the scanner again (decodeSingle stops after one hit).
        if (v.tag != attempt) { v.tag = attempt; v.decodeSingle(BarcodeCallback { r -> r.text?.let(onCode) }) }
    })
}

/** State sharePick: which PC gets the shared files (or text). */
@Composable
fun SharePick(what: String, pcNames: List<String>, onPick: (Int) -> Unit, onCancel: () -> Unit) {
    val p = LocalPalette.current
    Column(Modifier.fillMaxSize().background(p.bg).systemBarsPadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp, Alignment.Bottom)) {
        Text("Send $what to", style = Heading, color = p.ink)
        pcNames.forEachIndexed { i, n -> LipButton(displayName(n), { onPick(i) }, primary = i == 0) }
        TextAction("Cancel", onCancel, Modifier.fillMaxWidth())
    }
}

/** State shareWaiting: the shared files are being copied, so this stays open until they are. */
@Composable
fun SharePreparing(files: Int, pcName: String) {
    val p = LocalPalette.current
    Column(Modifier.fillMaxSize().background(p.bg).systemBarsPadding().padding(20.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        Text("Bridge", style = Title, color = p.ink)
        Text(
            "${if (files == 1) "1 file is" else "$files files are"} ready. Sending to ${displayName(pcName)} now.",
            style = Body.copy(fontWeight = FontWeight.Medium), color = p.warnInk,
            modifier = Modifier.fillMaxWidth().clip(Control).background(p.warnBg).padding(horizontal = 14.dp, vertical = 12.dp),
        )
    }
}
