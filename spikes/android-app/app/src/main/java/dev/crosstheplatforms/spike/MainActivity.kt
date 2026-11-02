package dev.crosstheplatforms.spike

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.bridge_core.sendFileFfi

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), 0)
        setContent { MaterialTheme { Screen() } }
    }

    @Composable
    private fun Screen() {
        val scope = rememberCoroutineScope()
        var ticket by remember { mutableStateOf(Shared.sendTicket(this)) }
        val recvTicket by Shared.receiveTicket.collectAsState()
        val log by Shared.log.collectAsState()
        val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri == null) return@rememberLauncherForActivityResult
            scope.launch {
                Shared.log.value = "sending…"
                try {
                    val file = Shared.copyToCache(this@MainActivity, uri)
                    val t0 = System.nanoTime()
                    val n = sendFileFfi(ticket, file.path)
                    val s = (System.nanoTime() - t0) / 1e9
                    Shared.log.value = "sent %d bytes in %.2fs = %.1f MB/s".format(n.toLong(), s, n.toDouble() / 1e6 / s)
                } catch (e: Exception) {
                    Shared.log.value = "send failed: ${e.message}"
                }
            }
        }
        Column(Modifier.padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text("Send to (ticket from bridge-recv)", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(ticket, { ticket = it; Shared.saveSendTicket(this@MainActivity, it) }, Modifier.fillMaxWidth(), minLines = 3)
            Button({ pick.launch(arrayOf("*/*")) }, enabled = ticket.isNotBlank()) { Text("Pick file and send") }
            HorizontalDivider()
            Button({ startForegroundService(Intent(this@MainActivity, ReceiveService::class.java)) }) { Text("Start receiving") }
            recvTicket?.let { t ->
                Text("My ticket (paste into bridge-send):", style = MaterialTheme.typography.titleMedium)
                Text(t, style = MaterialTheme.typography.bodySmall)
                Button({ getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("ticket", t)) }) { Text("Copy my ticket") }
            }
            HorizontalDivider()
            Text(log)
        }
    }
}
