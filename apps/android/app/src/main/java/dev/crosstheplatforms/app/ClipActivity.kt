package dev.crosstheplatforms.app

import android.app.Activity
import android.content.ClipboardManager
import android.util.Log
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
            val t0 = System.nanoTime()
            try {
                val node = app.node.filterNotNull().first()
                val pcs = App.pcs(node.peers())
                if (pcs.isEmpty()) app.toast("Pair with your PC first")
                pcs.forEach { node.sendClip(it.id, text) }
                Log.i(App.TAG, "clip sent to ${pcs.size} PC(s), ${text.length} chars, ${(System.nanoTime() - t0) / 1_000_000} ms")
                if (pcs.isNotEmpty()) app.toast("Clipboard sent")
            } catch (e: Exception) {
                Log.w(App.TAG, "clip send failed after ${(System.nanoTime() - t0) / 1_000_000} ms: ${e.message}")
                app.toast("Send failed: ${e.message}")
            }
        }
        finish()
    }
}
