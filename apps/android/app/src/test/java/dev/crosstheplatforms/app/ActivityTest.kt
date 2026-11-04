package dev.crosstheplatforms.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mirrors apps/tauri/src/lib/activity.test.ts, minus Send again (phone sends are retried from the app they started in). */
class ActivityTest {
    private fun run(vararg evs: Ev) = evs.foldIndexed(emptyList<Row>()) { i, rows, e -> applyEvent(rows, e, 1000L + i) }
    private fun started(outgoing: Boolean, files: Int = 2, id: Long = 1, peer: String = "p") = Ev.Started(id, peer, outgoing, "a.txt", files, 100)
    private val recv = Ev.Received("p")

    @Test fun incomingEndsDoneOnceAllFilesAreReceived() {
        val mid = run(started(false), Ev.Progress(1, 100, 100), recv)
        assertEquals(RowState.MOVING, mid[0].state)
        val end = run(started(false), Ev.Progress(1, 100, 100), recv, recv)[0]
        assertEquals(RowState.DONE, end.state)
        assertEquals(100L, end.done)
    }

    @Test fun outgoingEndsDoneOnFinalProgress() {
        val r = run(started(true), Ev.Progress(1, 40, 100))
        assertEquals(RowState.MOVING, r[0].state)
        assertEquals(40L, r[0].done)
        assertEquals(RowState.DONE, run(started(true), Ev.Progress(1, 100, 100))[0].state)
    }

    @Test fun requeuedEndsRequeued() {
        assertEquals(RowState.REQUEUED, run(started(true), Ev.Requeued(1))[0].state)
    }

    @Test fun failedAfterStartedMarksTheRowFailedWithReason() {
        val r = run(started(true), Ev.Failed(1, "boom"))[0]
        assertEquals(RowState.FAILED, r.state)
        assertEquals("boom", r.reason)
    }

    @Test fun failedWithUnknownIdAddsAFailedRowNamedAFile() {
        val r = run(Ev.Failed(9, "boom"))
        assertEquals(1, r.size)
        assertEquals(9L, r[0].id)
        assertEquals("A file", r[0].name)
        assertEquals(RowState.FAILED, r[0].state)
    }

    @Test fun aReceiverFailingAfterTheLastByteIsNeverDone() {
        // Core sends the final progress only after the receiver confirms, so a failure always finds the row moving.
        val r = run(started(true), Ev.Progress(1, 90, 100), Ev.Failed(1, "receiver reported a checksum mismatch"))
        assertEquals(RowState.FAILED, r[0].state)
    }

    @Test fun outgoingWithTotalZeroEndsDoneOnProgress() {
        val r = run(Ev.Started(1, "p", true, "e", 1, 0), Ev.Progress(1, 0, 0))
        assertEquals(RowState.DONE, r[0].state)
    }

    @Test fun startedForAPeerDropsThatPeersRequeuedRows() {
        assertEquals(listOf(2L), run(started(true), Ev.Requeued(1), started(true, id = 2)).map { it.id })
    }

    @Test fun startedKeepsRequeuedRowsOfOtherPeersAndFailedRows() {
        assertEquals(listOf(1L, 2L), run(started(true), Ev.Requeued(1), started(true, id = 2, peer = "q")).map { it.id })
        assertEquals(listOf(1L, 2L), run(started(true), Ev.Failed(1, "x"), started(true, id = 2)).map { it.id })
    }

    @Test fun plainErrorHidesTheRawStringAndTellsYouToSendAgain() {
        val raw = "peer is not reachable: timeout 0x5"
        assertFalse(plainError(raw).contains("0x5"))
        assertFalse(plainError(raw).lowercase().contains("queued"))
        assertTrue(plainError(raw).contains("send it again from the app you started in"))
        assertFalse(plainError("weird internal").contains("weird"))
        assertTrue(plainError("weird internal").lowercase().contains("send it again from the app you started in"))
    }

    private fun row(state: RowState = RowState.DONE, outgoing: Boolean = true, peer: String = "p", reason: String? = null) =
        Row(1, peer, outgoing, "a.txt", 1, 2_000_000, 2_000_000, 0, 1000, state, reason, 0)


    @Test fun statusSaysPairedAtRestAndNeverConnected() {
        assertEquals("Paired", statusLine(true, emptyList()))
        assertEquals("Paired", statusLine(true, run(started(true), Ev.Progress(1, 100, 100))))
        assertEquals("Not paired", statusLine(false, emptyList()))
        assertFalse(listOf(true, false).any { statusLine(it, run(started(true))).contains("Connected") })
    }

    @Test fun pcNotReachableFollowsAFailedSendUntilTheNextOneWorks() {
        val failed = run(started(true), Ev.Failed(1, "not reachable"))
        assertEquals("PC not reachable", statusLine(true, failed))
        assertEquals("PC not reachable", statusLine(true, run(Ev.Failed(9, "x")))) // failed before it started
        assertEquals("Paired", statusLine(true, run(started(true), Ev.Failed(1, "x"), started(true, id = 2), Ev.Progress(2, 100, 100))))
        assertEquals("Paired", statusLine(true, run(started(false), Ev.Failed(1, "x")))) // a failed receive is not the PC being away
    }

    @Test fun alertSaysSendItAgainForTheLatestFailedSendOnly() {
        assertEquals(null, alertLine(run(started(true), Ev.Progress(1, 100, 100))))
        val failed = alertLine(run(started(true), Ev.Failed(1, "peer not reachable")))!!
        assertTrue(failed.startsWith("a.txt did not send."))
        assertTrue(failed.contains("send it again from the app you started in"))
        assertEquals(null, alertLine(run(started(true), Ev.Failed(1, "x"), started(true, id = 2))))
    }

    @Test fun recentSkipsRunningRowsNewestFirstAndSaysSentOrReceived() {
        val rows = run(started(true, id = 1), Ev.Progress(1, 100, 100), started(false, id = 2), Ev.Failed(2, "x"), started(true, id = 3))
        assertEquals(listOf(2L, 1L), recent(rows).map { it.id })
        assertEquals("Sent, 100 B", shortResult(rows[0]))
        assertEquals("Failed", shortResult(rows[1]))
        assertEquals("Received, 2.0 MB", shortResult(row(outgoing = false)))
    }

    @Test fun progressLineSaysYouCanLeaveOnlyWhenSending() {
        val s = row(RowState.MOVING).copy(done = 620_000_000, total = 1_000_000_000)
        assertEquals("620.0 MB of 1.00 GB. You can leave Bridge, sending keeps going.", progressLine(s))
        assertEquals("620.0 MB of 1.00 GB", progressLine(s.copy(outgoing = false)))
        assertEquals(62, percent(s))
        assertEquals(0, percent(s.copy(total = 0)))
    }

    @Test fun pcCardSubSaysWhatTheTransferIsDoing() {
        assertEquals("Pair once, then everything is one tap.", pcSub(false, emptyList()))
        assertEquals("Ready", pcSub(true, emptyList()))
        assertEquals("Receiving your files", pcSub(true, run(started(true))))
        assertEquals("Sending to you", pcSub(true, run(started(false))))
        assertEquals("Looking for your PC", pcSub(true, run(started(true), Ev.Failed(1, "x"))))
    }

    @Test fun ringNameCountsTheOtherFiles() {
        assertEquals("a.txt", ringName(row(RowState.MOVING)))
        assertEquals("a.txt and 4 more", ringName(row(RowState.MOVING).copy(files = 5)))
    }

    @Test fun displayNameFallsBackWhenBlank() {
        assertEquals("Unnamed device", displayName(" "))
        assertEquals("Unnamed device", displayName(null))
        assertEquals("Office PC", displayName(" Office PC "))
    }

    @Test fun shownPcIsTheChosenOneElseTheFirst() {
        assertEquals("b", chooseId(listOf("a", "b"), "b"))
        assertEquals("a", chooseId(listOf("a", "b"), "gone")) // forgotten or not chosen yet
        assertEquals("a", chooseId(listOf("a", "b"), null))
        assertEquals(null, chooseId(emptyList(), "a"))
    }
}
