package dev.crosstheplatforms.app

import android.app.Application
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.OpenableColumns
import android.provider.Settings
import android.widget.Toast
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import uniffi.bridge_core.FfiEvent
import uniffi.bridge_core.FfiNode
import uniffi.bridge_core.FfiPeer
import uniffi.bridge_core.NodeEvents
import uniffi.bridge_core.PeerKind
import uniffi.bridge_core.startNode
import java.io.File

/** Holds the one node for the whole process (activities, service, tile). Never logs names or text. */
class App : Application(), NodeEvents {
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    val node = MutableStateFlow<FfiNode?>(null)
    val peers = MutableStateFlow<List<FfiPeer>>(emptyList())
    val status = MutableStateFlow("Starting…")
    private val starting = Mutex()

    override fun onCreate() {
        super.onCreate()
        // Copies left by a send the process didn't live to finish.
        // Synchronous so it can't race a share that starts in this fresh process (deletes are quick).
        File(cacheDir, OUTGOING).deleteRecursively()
        getSystemService(NotificationManager::class.java).apply {
            createNotificationChannel(NotificationChannel(CH_SERVICE, "Running", NotificationManager.IMPORTANCE_MIN))
            createNotificationChannel(NotificationChannel(CH_FILES, "Files", NotificationManager.IMPORTANCE_DEFAULT))
        }
    }

    val receiveDir: File
        get() = File(Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS), "Bridge")
            .takeIf { it.mkdirs() || it.isDirectory } ?: getExternalFilesDir(null)!!

    /** Called by the service; idempotent. */
    suspend fun start() = starting.withLock {
        if (node.value != null) return@withLock
        try {
            val name = Settings.Global.getString(contentResolver, "device_name") ?: Build.MODEL
            val n = startNode(File(filesDir, "node").path, receiveDir.path, name, PeerKind.ANDROID, this)
            peers.value = n.peers()
            node.value = n
            status.value = "Ready"
        } catch (e: Exception) {
            status.value = "Couldn't start: ${e.message}"
        }
    }

    /** Starts the service if needed and waits for the node. Call from a foreground activity. */
    suspend fun awaitNode(ctx: Context): FfiNode {
        ctx.startForegroundService(Intent(ctx, NodeService::class.java))
        return node.filterNotNull().first()
    }

    fun refreshPeers() { node.value?.let { peers.value = it.peers() } }

    override fun onEvent(e: FfiEvent) {
        when (e) {
            is FfiEvent.Paired -> refreshPeers()
            is FfiEvent.Incoming -> notifyFiles("Receiving ${e.files.size} file(s)…", ongoing = true)
            is FfiEvent.Received -> notifyFiles("Received ${formatBytes(e.size.toLong())} — Download/Bridge", ongoing = false)
            is FfiEvent.Failed -> notifyFiles("Transfer failed: ${e.reason}", ongoing = false)
            is FfiEvent.Progress -> {}
            // Writing the clipboard is allowed in the background (reading isn't).
            is FfiEvent.Clip -> scope.launch(Dispatchers.Main) {
                getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Bridge", e.text))
            }
        }
    }

    private fun notifyFiles(text: String, ongoing: Boolean) {
        val n = Notification.Builder(this, CH_FILES).setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle("Bridge").setContentText(text).setOngoing(ongoing).build()
        getSystemService(NotificationManager::class.java).notify(NOTE_FILES, n)
    }

    /** Copies shared/picked documents into `root` under their display names, off the main thread. */
    suspend fun copyToCache(root: File, uris: List<Uri>): List<File> = withContext(Dispatchers.IO) {
        uris.mapIndexed { i, uri ->
            val name = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
                if (it.moveToFirst()) it.getString(0) else null
            } ?: "file"
            // One folder per file: two shared files may have the same name.
            val out = File(File(root, "$i").apply { mkdirs() }, File(name).name)
            contentResolver.openInputStream(uri)!!.use { input -> out.outputStream().use { input.copyTo(it) } }
            out
        }
    }

    /**
     * Sends in the app scope (survives the activity finishing) and toasts the outcome. `onCopied`
     * runs once the files are read: share-sheet URI grants end with the activity, so it finishes only then.
     * Each send has its own cache folder, removed when the send ends.
     */
    fun sendInBackground(peer: FfiPeer, uris: List<Uri>, onCopied: () -> Unit = {}) = scope.launch {
        val root = File(File(cacheDir, OUTGOING), "${System.nanoTime()}")
        try {
            val files = try { copyToCache(root, uris) } finally { launch(Dispatchers.Main) { onCopied() } }
            val n = node.filterNotNull().first()
            val t0 = System.nanoTime()
            val bytes = n.sendFiles(peer.id, files.map { it.path }).toLong()
            val s = (System.nanoTime() - t0) / 1e9
            toast("Sent ${formatBytes(bytes)} to ${peer.name} (%.0f MB/s)".format(bytes / 1e6 / s))
        } catch (e: Exception) {
            android.util.Log.w(TAG, "send files failed: ${e.message}")
            toast("Send failed: ${e.message}")
        } finally {
            withContext(Dispatchers.IO) { root.deleteRecursively() }
        }
    }

    fun toast(s: String) = scope.launch(Dispatchers.Main) { Toast.makeText(this@App, s, Toast.LENGTH_SHORT).show() }

    companion object {
        const val TAG = "Bridge"
        const val OUTGOING = "outgoing"
        const val CH_SERVICE = "service"
        const val CH_FILES = "files"
        const val NOTE_SERVICE = 1
        const val NOTE_FILES = 2
        fun get(ctx: Context) = ctx.applicationContext as App
        fun formatBytes(n: Long): String = when {
            n >= 1_000_000_000 -> "%.2f GB".format(n / 1e9)
            n >= 1_000_000 -> "%.1f MB".format(n / 1e6)
            n >= 1_000 -> "${n / 1000} KB"
            else -> "$n B"
        }
        fun pcs(peers: List<FfiPeer>) = peers.filter { it.kind == PeerKind.WINDOWS }
    }
}
