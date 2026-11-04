package dev.crosstheplatforms.app

import android.app.Activity

/** Opened by the Quick Settings tile. Reads the clipboard once focused (Android only allows that in the foreground), then goes away. */
class ClipActivity : Activity() {
    private var done = false

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus || done) return
        done = true
        App.get(this).sendClipboard(this)
        finish()
    }
}
