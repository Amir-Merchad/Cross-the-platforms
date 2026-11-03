package dev.crosstheplatforms.app

import android.Manifest
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.Surface
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.journeyapps.barcodescanner.ScanContract
import com.journeyapps.barcodescanner.ScanOptions
import kotlinx.coroutines.launch
import uniffi.bridge_core.FfiPeer

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 0)
        NodeService.start(this)
        setContent { MaterialTheme(colorScheme = if (isSystemInDarkTheme()) darkColorScheme() else lightColorScheme()) { Surface { Screen() } } }
    }

    @Composable
    private fun Screen() {
        val app = App.get(this)
        val peers by app.peers.collectAsState()
        val status by app.status.collectAsState()
        var message by remember { mutableStateOf<String?>(null) }
        var sendTo by remember { mutableStateOf<FfiPeer?>(null) }

        val scan = rememberLauncherForActivityResult(ScanContract()) { r ->
            val text = r.contents ?: return@rememberLauncherForActivityResult
            message = "Pairing…"
            app.scope.launch {
                message = try { "Paired with ${app.awaitNode(this@MainActivity).join(text).name}".also { app.refreshPeers() } }
                catch (e: Exception) { "Pairing failed: ${e.message}" }
            }
        }
        val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
            val pc = sendTo ?: return@rememberLauncherForActivityResult
            if (uris.isNotEmpty()) { message = "Sending ${uris.size} file(s) to ${pc.name}…"; app.sendInBackground(pc, uris) }
        }

        Column(Modifier.padding(20.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Bridge", style = MaterialTheme.typography.headlineMedium)
            Text(status, style = MaterialTheme.typography.bodySmall)
            if (peers.isEmpty()) Text("Not paired yet. On the PC open Cross the Platforms → Transfer → Show pairing code, then scan it.")
            peers.forEach { p ->
                Card(Modifier.fillMaxWidth()) {
                    Row(Modifier.padding(16.dp).fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                        Text(p.name, style = MaterialTheme.typography.titleMedium)
                        Row {
                            TextButton({ app.node.value?.forget(p.id); app.refreshPeers() }) { Text("Forget") }
                            Button({ sendTo = p; pick.launch(arrayOf("*/*")) }) { Text("Send files") }
                        }
                    }
                }
            }
            OutlinedButton({
                scan.launch(ScanOptions().setDesiredBarcodeFormats(ScanOptions.QR_CODE).setPrompt("Scan the code on your PC").setBeepEnabled(false).setOrientationLocked(false))
            }) { Text("Pair with PC") }
            Text("Files from the PC land in Download/Bridge. To send your clipboard, add the \"Send clipboard to PC\" Quick Settings tile.", style = MaterialTheme.typography.bodySmall)
            message?.let { Text(it) }
        }
    }
}
