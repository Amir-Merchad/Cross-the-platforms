import XCTest
@testable import BridgeKit

final class FormatTests: XCTestCase {
    func testBytes() {
        XCTAssertEqual(formatBytes(2_550_000_000), "2.55 GB")
        XCTAssertEqual(formatBytes(1_500_000), "1.5 MB")
        XCTAssertEqual(formatBytes(999), "999 B")
    }
    func testInvite() {
        XCTAssertTrue(isPairInvite(" ctp-pair:{}\n"))
        XCTAssertFalse(isPairInvite("hello"))
    }
}

final class OutgoingQueueTests: XCTestCase {
    var tmp: URL!, dirA: URL!, dirB: URL!

    override func setUpWithError() throws {
        let fm = FileManager.default
        tmp = fm.temporaryDirectory.appendingPathComponent("bk-" + UUID().uuidString)
        dirA = tmp.appendingPathComponent("srcA")
        dirB = tmp.appendingPathComponent("srcB")
        for d in [tmp!, dirA!, dirB!] { try fm.createDirectory(at: d, withIntermediateDirectories: true) }
        for d in [dirA!, dirB!] { try Data(d.lastPathComponent.utf8).write(to: d.appendingPathComponent("a.pdf")) }
        // queue root is a separate folder so the source dirs are not mistaken for batches
        tmp = tmp.appendingPathComponent("queue")
        try fm.createDirectory(at: tmp, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: tmp.deletingLastPathComponent())
    }

    func testSameNamesBothSurvive() throws {
        let q = OutgoingQueue(root: tmp)
        try q.enqueue([dirA.appendingPathComponent("a.pdf"), dirB.appendingPathComponent("a.pdf")])
        let b = q.batches()
        XCTAssertEqual(b.count, 1)
        XCTAssertEqual(b[0].map(\.lastPathComponent), ["a.pdf", "a.pdf"])
        XCTAssertEqual(Set(b[0].map(\.path)).count, 2)
    }

    func testHalfCopiedBatchIsNeverSent() throws {
        try FileManager.default.createDirectory(at: tmp.appendingPathComponent(".incoming-1"), withIntermediateDirectories: true)
        let q = OutgoingQueue(root: tmp)
        XCTAssertTrue(q.batches().isEmpty)
        q.sweepIncomplete()
        XCTAssertFalse(FileManager.default.fileExists(atPath: tmp.appendingPathComponent(".incoming-1").path))
    }

    func testRemoveDropsTheWholeBatch() throws {
        let q = OutgoingQueue(root: tmp)
        try q.enqueue([dirA.appendingPathComponent("a.pdf"), dirB.appendingPathComponent("a.pdf")])
        let first = try XCTUnwrap(q.batches().first?.first)
        try q.remove(batchOf: first)
        XCTAssertTrue(q.batches().isEmpty)
    }
}

final class NodeLifecycleTests: XCTestCase {
    func testForegroundStartsOnce() {
        var l = NodeLifecycle()
        XCTAssertEqual(l.foreground(), [.start])
        XCTAssertEqual(l.foreground(), [])
    }
    func testIdleBackgroundStops() {
        var l = NodeLifecycle(); _ = l.foreground()
        XCTAssertEqual(l.background(), [.stop])
    }
    func testBackgroundWaitsForTransfer() {
        var l = NodeLifecycle(); _ = l.foreground(); l.transferStarted(files: 2)
        XCTAssertEqual(l.background(), [.beginBackgroundTime])
        XCTAssertEqual(l.fileFinished(), [])
        XCTAssertEqual(l.fileFinished(), [.stop, .endBackgroundTime])
    }
    func testBackgroundTimeRunsOut() {
        var l = NodeLifecycle(); _ = l.foreground(); l.transferStarted(files: 1); _ = l.background()
        XCTAssertEqual(l.backgroundTimeExpired(), [.stop, .endBackgroundTime])
        XCTAssertEqual(l.foreground(), [.start])
    }
    func testReturningInTimeKeepsTheNode() {
        var l = NodeLifecycle(); _ = l.foreground(); l.transferStarted(files: 1); _ = l.background()
        XCTAssertEqual(l.foreground(), [.endBackgroundTime])
    }
}
