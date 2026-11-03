package dev.crosstheplatforms.app

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** Brings the node back after a reboot, without opening the app. */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        if (intent.action == Intent.ACTION_BOOT_COMPLETED) NodeService.start(ctx)
    }
}
