package dev.crosstheplatforms.app

import android.app.PendingIntent
import android.content.Intent
import android.service.quicksettings.TileService

/** Quick Settings tile: opens a transparent activity, which may read the clipboard in the foreground. */
class ClipTile : TileService() {
    override fun onClick() {
        val intent = Intent(this, ClipActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
    }
}
