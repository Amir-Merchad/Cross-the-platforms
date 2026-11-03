package dev.crosstheplatforms.app

import android.app.Notification
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import kotlinx.coroutines.launch

/** Keeps the node alive so the PC can send files and clipboard text at any time. */
class NodeService : Service() {
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n = Notification.Builder(this, App.CH_SERVICE).setSmallIcon(android.R.drawable.ic_menu_share)
            .setContentTitle("Bridge is ready").setContentText("Receives files and clipboard from your PC").setContentIntent(open).build()
        startForeground(App.NOTE_SERVICE, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        val app = App.get(this)
        app.scope.launch { app.start() }
        return START_STICKY
    }

    companion object {
        fun start(ctx: Context) = ctx.startForegroundService(Intent(ctx, NodeService::class.java))
    }
}
