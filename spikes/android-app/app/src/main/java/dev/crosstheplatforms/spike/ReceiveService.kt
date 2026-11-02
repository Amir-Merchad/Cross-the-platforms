package dev.crosstheplatforms.spike

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Environment
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.bridge_core.startReceiver
import java.io.File

/** Foreground service that keeps the phone reachable and saves files to Download/Bridge. */
class ReceiveService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var running = false

    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("recv", "Receiving", NotificationManager.IMPORTANCE_LOW))
        startForeground(1, note("Waiting for files"), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        if (running) return START_STICKY
        running = true
        scope.launch {
            val dir = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Bridge")
                .takeIf { it.mkdirs() || it.isDirectory } ?: getExternalFilesDir(null)!!
            try {
                val rx = startReceiver(dir.path)
                Shared.receiveTicket.value = rx.ticket()
                while (isActive) {
                    val t0 = System.nanoTime()
                    try {
                        val r = rx.next()
                        val secs = (System.nanoTime() - t0) / 1e9
                        val msg = "received %d bytes (%.1f MB/s incl. wait)".format(r.size.toLong(), r.size.toDouble() / 1e6 / secs)
                        Shared.log.value = msg
                        nm.notify(1, note(msg))
                    } catch (e: Exception) {
                        Shared.log.value = "receive failed: ${e.message}"
                    }
                }
            } catch (e: Exception) {
                Shared.log.value = "receiver failed to start: ${e.message}"
            }
        }
        return START_STICKY
    }

    private fun note(text: String) = Notification.Builder(this, "recv")
        .setSmallIcon(android.R.drawable.stat_sys_download_done).setContentTitle("CTP Spike").setContentText(text).build()

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
