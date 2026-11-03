package dev.crosstheplatforms.app

import android.app.Activity
import android.content.ClipboardManager
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/** Reads the clipboard once focused (Android only allows that in the foreground) and sends it to the paired PCs. */
class ClipActivity : Activity() {
    private var done = false

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        val app = App.get(this)
        val text = getSystemService(ClipboardManager::class.java).primaryClip?.getItemAt(0)?.coerceToText(this)?.toString()
        if (text.isNullOrEmpty()) { app.toast("Clipboard is empty"); finish(); return }
        NodeService.start(this) // while still in the foreground
        app.scope.launch {
            try {
                val node = app.node.filterNotNull().first()
                val pcs = App.pcs(node.peers())
                if (pcs.isEmpty()) app.toast("Pair with your PC first")
                pcs.forEach { node.sendClip(it.id, text) }
                if (pcs.isNotEmpty()) app.toast("Clipboard sent")
            } catch (e: Exception) {
                app.toast("Send failed: ${e.message}")
            }
        }
        finish()
    }
}
