package dev.crosstheplatforms.app

import android.app.PendingIntent
import android.content.Intent
import android.service.quicksettings.TileService

/** Quick Settings tile: the clipboard text goes to the PC's notes inbox, not its clipboard. */
class NoteTile : TileService() {
    override fun onClick() {
        val intent = Intent(this, ClipActivity::class.java).putExtra(ClipActivity.TO_NOTES, true).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivityAndCollapse(PendingIntent.getActivity(this, 1, intent, PendingIntent.FLAG_IMMUTABLE))
    }
}
