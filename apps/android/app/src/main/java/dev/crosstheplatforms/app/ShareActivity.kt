package dev.crosstheplatforms.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import uniffi.bridge_core.FfiPeer

/** Share sheet target: files go to the paired PC (asks which, if there are several); shared text becomes its clipboard. */
class ShareActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val app = App.get(this)
        val uris = sharedUris(intent)
        val text = intent.getStringExtra(Intent.EXTRA_TEXT)
        if (uris.isEmpty() && text.isNullOrEmpty()) { app.toast("Nothing to send"); finish(); return }
        val send = { pc: FfiPeer ->
            if (uris.isNotEmpty()) app.sendInBackground(pc, uris)
            else app.scope.launch {
                try { app.node.value?.sendClip(pc.id, text!!); app.toast("Text sent to ${pc.name}") } catch (e: Exception) { app.toast("Send failed: ${e.message}") }
            }
            finish()
        }
        app.scope.launch {
            app.awaitNode(this@ShareActivity)
            val pcs = App.pcs(app.peers.value)
            runOnUiThread {
                when (pcs.size) {
                    0 -> { app.toast("Pair with your PC first (open Bridge)"); finish() }
                    1 -> send(pcs[0])
                    else -> setContent {
                        MaterialTheme {
                            val peers by app.peers.collectAsState()
                            Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("Send to", style = MaterialTheme.typography.titleMedium)
                                App.pcs(peers).forEach { pc -> Button({ send(pc) }, Modifier.fillMaxWidth()) { Text(pc.name) } }
                            }
                        }
                    }
                }
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun sharedUris(i: Intent): List<Uri> = when (i.action) {
        Intent.ACTION_SEND -> listOfNotNull(i.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java))
        Intent.ACTION_SEND_MULTIPLE -> i.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
        else -> emptyList()
    }
}
