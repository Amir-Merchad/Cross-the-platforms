package dev.crosstheplatforms.spike

import android.app.Activity
import android.content.ClipboardManager
import android.widget.Toast
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import uniffi.bridge_core.sendFileFfi
import java.io.File

/** Reads the clipboard once focused (Android 10+ only allows that in the foreground) and sends it as clip.txt. */
class ClipActivity : Activity() {
    private var done = false

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        val text = getSystemService(ClipboardManager::class.java).primaryClip?.getItemAt(0)?.coerceToText(this)?.toString()
        val ticket = Shared.sendTicket(this)
        if (ticket.isBlank()) { toast("Set a ticket in the app first"); finish(); return }
        if (text.isNullOrEmpty()) { toast("Clipboard is empty"); finish(); return }
        val file = File(File(cacheDir, "clip").apply { mkdirs() }, "clip.txt").apply { writeText(text) }
        CoroutineScope(Dispatchers.Main).launch {
            try {
                sendFileFfi(ticket, file.path)
                toast("Clipboard sent")
            } catch (e: Exception) {
                toast("Send failed: ${e.message}")
            }
            finish()
        }
    }

    private fun toast(s: String) = Toast.makeText(applicationContext, s, Toast.LENGTH_SHORT).show()
}
