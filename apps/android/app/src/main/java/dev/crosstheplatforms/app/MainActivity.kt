package dev.crosstheplatforms.app

import android.Manifest
import android.content.ClipboardManager
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import dev.crosstheplatforms.app.ui.BridgeTheme
import dev.crosstheplatforms.app.ui.HomeScreen
import dev.crosstheplatforms.app.ui.PairScreen
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 0)
        NodeService.start(this)
        setContent { BridgeTheme { Screen() } }
    }

    @Composable
    private fun Screen() {
        val app = App.get(this)
        val peers by app.peers.collectAsState()
        val rows by app.transfers.collectAsState()
        val startFailed by app.startFailed.collectAsState()
        var chosen by remember { mutableStateOf<String?>(null) }
        val pcs = App.pcs(peers)
        val pc = pcs.firstOrNull { it.id == chooseId(pcs.map { p -> p.id }, chosen) } // Send files, clipboard and Forget follow the PC shown
        var pairing by remember { mutableStateOf(false) }
        var pairMessage by remember { mutableStateOf<String?>(null) }
        var attempt by remember { mutableIntStateOf(0) }

        val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
            if (pc != null && uris.isNotEmpty()) app.sendInBackground(pc, uris)
        }
        fun join(code: String) {
            pairMessage = "Pairing…"
            app.scope.launch {
                try {
                    app.awaitNode(this@MainActivity).join(code)
                    app.refreshPeers()
                    pairing = false
                    pairMessage = null
                } catch (e: Exception) {
                    pairMessage = "That code did not work. Show a new one on the PC and try again."
                    attempt++
                }
            }
        }

        BackHandler(pairing) { pairing = false; pairMessage = null }
        if (pairing) PairScreen(
            pairMessage, attempt, onCode = ::join,
            onPaste = {
                val code = getSystemService(ClipboardManager::class.java).primaryClip?.getItemAt(0)?.coerceToText(this)?.toString()
                if (code.isNullOrBlank()) pairMessage = "Copy the code on the PC first, then press Paste code." else join(code.trim())
            },
            onCancel = { pairing = false; pairMessage = null },
        ) else HomeScreen(
            pcName = pc?.name,
            pcNames = pcs.map { it.name },
            shown = pcs.indexOf(pc),
            onChoose = { chosen = pcs[it].id },
            rows = rows,
            startFailed = startFailed,
            onSendFiles = { pick.launch(arrayOf("*/*")) },
            onSendClipboard = { app.sendClipboard(this, pc) },
            onPair = { pairing = true },
            onForget = { pc?.let { app.node.value?.forget(it.id); app.refreshPeers() } },
        )
    }
}
