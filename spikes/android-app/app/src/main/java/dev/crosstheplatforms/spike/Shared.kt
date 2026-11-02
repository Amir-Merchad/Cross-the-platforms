package dev.crosstheplatforms.spike

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import kotlinx.coroutines.flow.MutableStateFlow
import java.io.File

/** Spike-wide state shared by the activity, service and tile. */
object Shared {
    val receiveTicket = MutableStateFlow<String?>(null)
    val log = MutableStateFlow("")

    fun sendTicket(ctx: Context): String = ctx.getSharedPreferences("spike", 0).getString("ticket", "") ?: ""
    fun saveSendTicket(ctx: Context, t: String) = ctx.getSharedPreferences("spike", 0).edit().putString("ticket", t.trim()).apply()

    /** Copies a picked document into cache under its display name so Rust can read a real path. */
    fun copyToCache(ctx: Context, uri: Uri): File {
        val name = ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        } ?: "file"
        val dir = File(ctx.cacheDir, "outgoing").apply { deleteRecursively(); mkdirs() }
        val out = File(dir, File(name).name)
        ctx.contentResolver.openInputStream(uri)!!.use { input -> out.outputStream().use { input.copyTo(it) } }
        return out
    }
}
