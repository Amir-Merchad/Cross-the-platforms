package dev.crosstheplatforms.app

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import dev.crosstheplatforms.app.ui.BridgeTheme
import dev.crosstheplatforms.app.ui.SharePick
import dev.crosstheplatforms.app.ui.SharePreparing
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
            if (uris.isNotEmpty()) {
                // Stay open until the files are copied: the share grant ends when this activity does.
                setContent { BridgeTheme { SharePreparing(uris.size, pc.name) } }
                app.sendInBackground(pc, uris) { finish() }
            } else {
                app.scope.launch {
                    try { app.node.value?.sendClip(pc.id, text!!); app.toast("Text sent to ${pc.name}") } catch (e: Exception) { app.toast("Text did not send. Open Bridge on the PC and try again.") }
                }
                finish()
            }
        }
        app.scope.launch {
            app.awaitNode(this@ShareActivity)
            val pcs = App.pcs(app.peers.value)
            runOnUiThread {
                when (pcs.size) {
                    0 -> { app.toast("Pair with your PC first (open Bridge)"); finish() }
                    1 -> send(pcs[0])
                    else -> setContent {
                        BridgeTheme {
                            SharePick(
                                if (uris.isEmpty()) "text" else if (uris.size == 1) "1 file" else "${uris.size} files",
                                pcs.map { it.name }, { send(pcs[it]) }, { finish() },
                            )
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
