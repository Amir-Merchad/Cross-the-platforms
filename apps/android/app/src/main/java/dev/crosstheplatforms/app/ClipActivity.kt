package dev.crosstheplatforms.app

import android.app.Activity

/** Opened by the Quick Settings tile. Reads the clipboard once focused (Android only allows that in the foreground), then goes away. */
class ClipActivity : Activity() {
    private var done = false

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        if (intent.getBooleanExtra(TO_NOTES, false)) App.get(this).sendClipboardToNotes(this) else App.get(this).sendClipboard(this)
        finish()
    }

    companion object { const val TO_NOTES = "to_notes" }
}
