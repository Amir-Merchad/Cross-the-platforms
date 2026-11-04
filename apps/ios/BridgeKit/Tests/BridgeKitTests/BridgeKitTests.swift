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

    private func age(_ url: URL, by seconds: TimeInterval) throws {
        try FileManager.default.setAttributes([.creationDate: Date(timeIntervalSinceNow: -seconds)], ofItemAtPath: url.path)
    }

    func testHalfCopiedBatchIsNeverSent() throws {
        let half = tmp.appendingPathComponent(".incoming-1")
        try FileManager.default.createDirectory(at: half, withIntermediateDirectories: true)
        try age(half, by: 2 * 3600)
        let q = OutgoingQueue(root: tmp)
        XCTAssertTrue(q.batches().isEmpty)
        q.sweepIncomplete()
        XCTAssertFalse(FileManager.default.fileExists(atPath: half.path))
    }

    func testFreshIncomingFolderSurvivesTheSweep() throws {
        let live = tmp.appendingPathComponent(".incoming-live")
        try FileManager.default.createDirectory(at: live, withIntermediateDirectories: true)
        OutgoingQueue(root: tmp).sweepIncomplete()
        XCTAssertTrue(FileManager.default.fileExists(atPath: live.path))
    }

    func testRemoveIgnoresFilesOutsideTheQueue() throws {
        let other = tmp.deletingLastPathComponent().appendingPathComponent("other")
        let file = other.appendingPathComponent("x/f.txt")
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("x".utf8).write(to: file)
        try OutgoingQueue(root: tmp).remove(batchOf: file)
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: other.path))
    }

    func testBatchesAreOldestFirst() throws {
        let q = OutgoingQueue(root: tmp)
        let first = try q.enqueue([dirA.appendingPathComponent("a.pdf")])
        let second = try q.enqueue([dirB.appendingPathComponent("a.pdf")])
        try age(first, by: 200)
        try age(second, by: 100)
        let b = q.batches()
        XCTAssertEqual(b.count, 2)
        func batchName(_ f: URL) -> String { f.deletingLastPathComponent().deletingLastPathComponent().lastPathComponent }
        XCTAssertEqual(batchName(b[0][0]), first.lastPathComponent)
        XCTAssertEqual(batchName(b[1][0]), second.lastPathComponent)
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
    func testOneFailureDoesNotEndTheOtherTransfer() {
        var l = NodeLifecycle(); _ = l.foreground()
        l.transferStarted(files: 1); l.transferStarted(files: 1)
        XCTAssertEqual(l.transferFailed(), [])
        XCTAssertEqual(l.background(), [.beginBackgroundTime])
        XCTAssertEqual(l.fileFinished(), [.stop, .endBackgroundTime])
    }
    func testReturningInTimeKeepsTheNode() {
        var l = NodeLifecycle(); _ = l.foreground(); l.transferStarted(files: 1); _ = l.background()
        XCTAssertEqual(l.foreground(), [.endBackgroundTime])
    }
}
