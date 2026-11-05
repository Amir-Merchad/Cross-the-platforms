package dev.crosstheplatforms.app

import java.util.Locale
import kotlin.math.roundToLong

// In-memory transfer history for the session, folded from core events. Pure Kotlin (no Android) so unit tests run it.
// Mirrors apps/tauri/src/lib/activity.ts, minus Send again: a phone send is retried from the app it started in.

/** Core events the history cares about. `App` maps FfiEvent to these. Never carries paths or clipboard text. */
sealed interface Ev {
    data class Started(val id: Long, val peer: String, val outgoing: Boolean, val name: String, val files: Int, val total: Long) : Ev
    data class Progress(val id: Long, val done: Long, val total: Long) : Ev
    data class Received(val from: String) : Ev
    data class Requeued(val id: Long) : Ev
    data class Failed(val id: Long, val reason: String) : Ev
}

enum class RowState { MOVING, DONE, FAILED, REQUEUED }

data class Row(
    val id: Long, val peer: String, val outgoing: Boolean, val name: String, val files: Int, val total: Long, val done: Long,
    val startedAt: Long, val now: Long, val state: RowState, val reason: String? = null,
    val received: Int = 0, // files received so far, to end incoming rows
)

private fun List<Row>.patch(id: Long, f: (Row) -> Row) = map { if (it.id == id) f(it) else it }

fun applyEvent(rows: List<Row>, e: Ev, now: Long): List<Row> = when (e) {
    is Ev.Started -> rows.filter { it.id != e.id && !(it.state == RowState.REQUEUED && it.peer == e.peer) } +
        Row(e.id, e.peer, e.outgoing, e.name, e.files, e.total, 0, now, now, RowState.MOVING)
    is Ev.Progress -> rows.patch(e.id) {
        it.copy(done = e.done, total = e.total, now = now, state = if (it.outgoing && e.done >= e.total) RowState.DONE else it.state)
    }
    is Ev.Received -> {
        val row = rows.find { !it.outgoing && it.peer == e.from && it.state == RowState.MOVING }
        if (row == null) rows else {
            val received = row.received + 1
            rows.patch(row.id) { if (received >= it.files) it.copy(received = received, now = now, done = it.total, state = RowState.DONE) else it.copy(received = received, now = now) }
        }
    }
    is Ev.Requeued -> rows.patch(e.id) { it.copy(state = RowState.REQUEUED, now = now) }
    is Ev.Failed ->
        if (rows.any { it.id == e.id }) rows.patch(e.id) { if (it.state == RowState.MOVING) it.copy(state = RowState.FAILED, reason = e.reason, now = now) else it }
        else rows + Row(e.id, "", false, "A file", 1, 0, 0, now, now, RowState.FAILED, e.reason)
}

private const val SEND_AGAIN = "send it again from the app you started in"

/** A sentence for the user. Never the raw reason, which can carry paths or internals. */
fun plainError(reason: String): String =
    if (Regex("not reachable", RegexOption.IGNORE_CASE).containsMatchIn(reason)) "Not reachable right now. Open Bridge on the PC, then $SEND_AGAIN."
    else "That did not finish. ${SEND_AGAIN.replaceFirstChar { it.uppercase() }}."

/** The share-sheet alias "Send to notes" (AndroidManifest `.ShareNoteActivity`); the plain ShareActivity is not it. */
fun isNotesAlias(className: String?): Boolean = className == "dev.crosstheplatforms.app.ShareNoteActivity"

fun displayName(name: String?): String = name?.trim().takeUnless { it.isNullOrEmpty() } ?: "Unnamed device"

fun formatBytes(n: Long): String = when {
    n >= 1_000_000_000 -> String.format(Locale.ROOT, "%.2f GB", n / 1e9)
    n >= 1_000_000 -> String.format(Locale.ROOT, "%.1f MB", n / 1e6)
    n >= 1_000 -> "${(n / 1e3).roundToLong()} KB"
    else -> "$n B"
}

fun formatRate(bytes: Long, ms: Long): String {
    if (ms <= 0) return "—"
    val perS = bytes.toDouble() / ms * 1000
    return if (perS >= 1e6) String.format(Locale.ROOT, "%.1f MB/s", perS / 1e6) else "${(perS / 1e3).roundToLong()} KB/s"
}


/**
 * The PC row's status. Reachability is not known to the phone (the hub derives it from Hello), so it reads
 * "Paired" at rest and "PC not reachable" only while the latest send of ours has failed. Never "Connected".
 */
fun statusLine(paired: Boolean, rows: List<Row>): String =
    if (!paired) "Not paired" else if (failedSend(rows) != null) "PC not reachable" else "Paired"

// A failure with no peer is a send that failed before it started.
private fun failedSend(rows: List<Row>) = rows.lastOrNull { it.outgoing || it.peer.isEmpty() }?.takeIf { it.state == RowState.FAILED }

/** The warning on Home: the latest send failed, so say what to do. Null when nothing is wrong. */
fun alertLine(rows: List<Row>): String? = failedSend(rows)?.let { "${it.name} did not send. ${plainError(it.reason ?: "")}" }

/** Finished rows for the Recent list, newest first. Running ones show in the progress card instead. */
fun recent(rows: List<Row>, n: Int = 3): List<Row> = rows.filter { it.state != RowState.MOVING }.takeLast(n).reversed()

fun shortResult(r: Row): String = when (r.state) {
    RowState.FAILED -> "Failed"
    RowState.REQUEUED -> "Waiting"
    else -> "${if (r.outgoing) "Sent" else "Received"}, ${formatBytes(r.total)}"
}

fun percent(r: Row): Int = if (r.total <= 0) 0 else (r.done * 100 / r.total).toInt().coerceIn(0, 100)

fun progressLine(r: Row): String =
    "${formatBytes(r.done)} of ${formatBytes(r.total)}" + if (r.outgoing) ". You can leave Bridge, sending keeps going." else ""

/** The PC card's second line. */
fun pcSub(paired: Boolean, rows: List<Row>): String {
    if (!paired) return "Pair once, then everything is one tap."
    if (failedSend(rows) != null) return "Looking for your PC"
    val moving = rows.lastOrNull { it.state == RowState.MOVING } ?: return "Ready"
    return if (moving.outgoing) "Receiving your files" else "Sending to you" // the PC's side of it
}

fun ringName(r: Row): String = if (r.files > 1) "${r.name} and ${r.files - 1} more" else r.name

/** Which PC Home shows: the chosen one if it is still paired, else the first. */
fun chooseId(ids: List<String>, chosen: String?): String? = ids.firstOrNull { it == chosen } ?: ids.firstOrNull()
