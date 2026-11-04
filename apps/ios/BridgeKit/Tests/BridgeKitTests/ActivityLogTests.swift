import XCTest
@testable import BridgeKit

/// Mirrors the Android ActivityTest, except that a failed iPad to PC send stays queued (OutgoingQueue) and so reads as waiting.
final class ActivityLogTests: XCTestCase {
    private func run(_ evs: TransferEvent...) -> [TransferRow] {
        var log = ActivityLog()
        for (i, e) in evs.enumerated() { log.apply(event: e, now: 1000 + i) }
        return log.rows
    }
    private func started(_ outgoing: Bool, files: Int = 2, id: UInt64 = 1, peer: String = "p") -> TransferEvent {
        .started(id: id, peer: peer, outgoing: outgoing, name: "a.txt", files: files, total: 100)
    }
    private let recv = TransferEvent.received(from: "p")
    private func row(_ state: RowState = .done, outgoing: Bool = true) -> TransferRow {
        TransferRow(id: 1, peer: "p", outgoing: outgoing, name: "a.txt", files: 1, total: 2_000_000, done: 2_000_000, startedAt: 0, now: 1000, state: state)
    }

    func testIncomingEndsDoneOnceAllFilesAreReceived() {
        XCTAssertEqual(run(started(false), .progress(id: 1, done: 100, total: 100), recv)[0].state, .moving)
        let end = run(started(false), .progress(id: 1, done: 100, total: 100), recv, recv)[0]
        XCTAssertEqual(end.state, .done)
        XCTAssertEqual(end.done, 100)
    }

    func testOutgoingEndsDoneOnFinalProgress() {
        let r = run(started(true), .progress(id: 1, done: 40, total: 100))
        XCTAssertEqual(r[0].state, .moving)
        XCTAssertEqual(r[0].done, 40)
        XCTAssertEqual(run(started(true), .progress(id: 1, done: 100, total: 100))[0].state, .done)
    }

    func testRequeuedEndsWaiting() {
        XCTAssertEqual(run(started(true), .requeued(id: 1))[0].state, .waiting)
    }

    func testFailedSendToPCStaysQueuedSoItIsWaitingNotFailed() {
        let r = run(started(true), .failed(id: 1, reason: "device not reachable: timed out"))[0]
        XCTAssertEqual(r.state, .waiting)
        XCTAssertEqual(run(started(true), .failed(id: 1, reason: "connection stalled"))[0].state, .waiting)
    }

    func testFailedSendThatWouldFailAgainIsDroppedSoItReadsFailed() {
        let r = run(started(true), .failed(id: 1, reason: "declined: no"))[0]
        XCTAssertEqual(r.state, .failed)
        XCTAssertEqual(r.reason, "declined: no")
    }

    func testFailedReceiveIsFailed() {
        XCTAssertEqual(run(started(false), .failed(id: 1, reason: "connection stalled"))[0].state, .failed)
    }

    func testFailedWithUnknownIdAddsARowNamedAFile() {
        let r = run(.failed(id: 9, reason: "not reachable"))
        XCTAssertEqual(r.count, 1)
        XCTAssertEqual(r[0].id, 9)
        XCTAssertEqual(r[0].name, "A file")
        XCTAssertEqual(r[0].state, .waiting)
    }

    func testAReceiverFailingAfterTheLastByteIsNeverDone() {
        // Core sends the final progress only after the receiver confirms, so a failure always finds the row moving.
        XCTAssertEqual(run(started(true), .progress(id: 1, done: 90, total: 100), .failed(id: 1, reason: "receiver reported a checksum mismatch"))[0].state, .waiting)
    }

    func testOutgoingWithTotalZeroEndsDoneOnProgress() {
        XCTAssertEqual(run(.started(id: 1, peer: "p", outgoing: true, name: "e", files: 1, total: 0), .progress(id: 1, done: 0, total: 0))[0].state, .done)
    }

    func testStartedForAPeerDropsThatPeersWaitingRows() {
        XCTAssertEqual(run(started(true), .requeued(id: 1), started(true, id: 2)).map(\.id), [2])
    }

    func testStartedKeepsWaitingRowsOfOtherPeersAndFailedRows() {
        XCTAssertEqual(run(started(true), .requeued(id: 1), started(true, id: 2, peer: "q")).map(\.id), [1, 2])
        XCTAssertEqual(run(started(true), .failed(id: 1, reason: "declined"), started(true, id: 2)).map(\.id), [1, 2])
    }

    func testPlainErrorHidesTheRawStringAndSaysWhatHappensNext() {
        let raw = "peer is not reachable: timeout 0x5"
        XCTAssertFalse(plainError(raw).contains("0x5"))
        XCTAssertTrue(plainError(raw).contains("It stays queued and sends when the PC is back."))
        XCTAssertFalse(plainError("declined: weird internal").contains("weird"))
        XCTAssertTrue(plainError("declined: weird internal").contains("Send it again from the app you started in."))
        XCTAssertFalse(plainError("connection stalled").contains("Send it again"))
    }

    func testStatusSaysPairedAtRestAndNeverConnected() {
        XCTAssertEqual(statusLine(paired: true, rows: []), "Paired")
        XCTAssertEqual(statusLine(paired: true, rows: run(started(true), .progress(id: 1, done: 100, total: 100))), "Paired")
        XCTAssertEqual(statusLine(paired: false, rows: []), "Not paired")
        for paired in [true, false] { XCTAssertFalse(statusLine(paired: paired, rows: run(started(true))).contains("Connected")) }
    }

    func testPCNotReachableFollowsAFailedSendUntilTheNextOneWorks() {
        XCTAssertEqual(statusLine(paired: true, rows: run(started(true), .failed(id: 1, reason: "not reachable"))), "PC not reachable")
        XCTAssertEqual(statusLine(paired: true, rows: run(.failed(id: 9, reason: "x"))), "PC not reachable")
        XCTAssertEqual(statusLine(paired: true, rows: run(started(true), .failed(id: 1, reason: "declined"), started(true, id: 2), .progress(id: 2, done: 100, total: 100))), "Paired")
        XCTAssertEqual(statusLine(paired: true, rows: run(started(false), .failed(id: 1, reason: "x"))), "Paired")
    }

    func testAlertSaysItStaysQueuedAndNeverOffersSendAgain() {
        XCTAssertNil(alertLine(run(started(true), .progress(id: 1, done: 100, total: 100))))
        let a = alertLine(run(started(true), .failed(id: 1, reason: "peer not reachable")))!
        XCTAssertEqual(a, "a.txt did not send. The PC went away. It stays queued and sends when the PC is back.")
        XCTAssertNil(alertLine(run(started(true), .failed(id: 1, reason: "x"), started(true, id: 2))))
    }

    func testRecentSkipsRunningRowsNewestFirstAndSaysSentReceivedOrWaiting() {
        let rows = run(started(true, id: 1), .progress(id: 1, done: 100, total: 100), started(false, id: 2), .failed(id: 2, reason: "x"), started(true, id: 3))
        XCTAssertEqual(recent(rows).map(\.id), [2, 1])
        XCTAssertEqual(shortResult(rows[0]), "Sent, 100 B")
        XCTAssertEqual(shortResult(rows[1]), "Failed")
        XCTAssertEqual(shortResult(row(.waiting)), "Waiting")
        XCTAssertEqual(shortResult(row(outgoing: false)), "Received, 2.0 MB")
    }

    func testProgressLineTellsTheiPadToStayOpenOnlyWhenSending() {
        var s = row(.moving)
        s.done = 620_000_000; s.total = 1_000_000_000
        XCTAssertEqual(progressLine(s), "620.0 MB of 1.00 GB. Keep Bridge open until this finishes.")
        XCTAssertEqual(percent(s), 62)
        s.outgoing = false
        XCTAssertEqual(progressLine(s), "620.0 MB of 1.00 GB")
        s.total = 0
        XCTAssertEqual(percent(s), 0)
    }

    func testFormatRate() {
        XCTAssertEqual(formatRate(bytes: 41_000_000, ms: 1000), "41.0 MB/s")
        XCTAssertEqual(formatRate(bytes: 500_000, ms: 1000), "500 KB/s")
        XCTAssertEqual(formatRate(bytes: 5, ms: 0), "—")
    }

    func testPCCardSubSaysWhatTheTransferIsDoing() {
        XCTAssertEqual(pcSub(paired: false, rows: []), "Pair once, then everything is one tap.")
        XCTAssertEqual(pcSub(paired: true, rows: []), "Ready")
        XCTAssertEqual(pcSub(paired: true, rows: run(started(true))), "Receiving your files")
        XCTAssertEqual(pcSub(paired: true, rows: run(started(false))), "Sending to you")
        XCTAssertEqual(pcSub(paired: true, rows: run(started(true), .failed(id: 1, reason: "not reachable"))), "Looking for your PC")
    }

    func testRingNameCountsTheOtherFiles() {
        XCTAssertEqual(ringName(row(.moving)), "a.txt")
        var r = row(.moving)
        r.files = 5
        XCTAssertEqual(ringName(r), "a.txt and 4 more")
    }

    func testDisplayNameFallsBackWhenBlank() {
        XCTAssertEqual(displayName(" "), "Unnamed device")
        XCTAssertEqual(displayName(nil), "Unnamed device")
        XCTAssertEqual(displayName(" Office PC "), "Office PC")
    }

    func testShownPCIsTheChosenOneElseTheFirst() {
        XCTAssertEqual(chooseId(["a", "b"], chosen: "b"), "b")
        XCTAssertEqual(chooseId(["a", "b"], chosen: "gone"), "a")
        XCTAssertEqual(chooseId(["a", "b"], chosen: nil), "a")
        XCTAssertNil(chooseId([], chosen: "a"))
    }
}
