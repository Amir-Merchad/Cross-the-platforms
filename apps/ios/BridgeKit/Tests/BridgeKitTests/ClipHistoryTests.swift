import XCTest
@testable import BridgeKit

final class ClipHistoryTests: XCTestCase {
    func testOldestUnpinnedGoesAt51() {
        var h = ClipHistory()
        for i in 0..<51 { h.add("clip \(i)", from: "PC", now: i) }
        XCTAssertEqual(h.items.count, 50)
        XCTAssertEqual(h.items.first?.text, "clip 50")
        XCTAssertEqual(h.items.last?.text, "clip 1")
    }

    func testPinsSurviveTrimmingAndDoNotCount() {
        var h = ClipHistory()
        h.add("keep me", from: "PC", now: 0)
        h.setPinned(h.items[0].id, true)
        for i in 0..<60 { h.add("n\(i)", from: "PC", now: i + 1) }
        XCTAssertEqual(h.items.filter { !$0.pinned }.count, 50)
        XCTAssertTrue(h.items.contains { $0.pinned && $0.text == "keep me" })
    }

    func testDuplicateMovesToTop() {
        var h = ClipHistory()
        h.add("a", from: "PC", now: 1)
        h.add("b", from: "PC", now: 2)
        h.add("a", from: "Phone", now: 3)
        XCTAssertEqual(h.items.map(\.text), ["a", "b"])
        XCTAssertEqual(h.items[0].from, "Phone")
    }

    func testSearchIgnoresCase() {
        var h = ClipHistory()
        h.add("Hello World", from: "PC", now: 1)
        h.add("other", from: "PC", now: 2)
        XCTAssertEqual(h.search("wORLD").count, 1)
        XCTAssertEqual(h.search("").count, 2)
        XCTAssertTrue(h.search("zzz").isEmpty)
    }

    func testEmptyAndHugeTextAreNotKept() {
        var h = ClipHistory()
        h.add("", from: "PC")
        h.add(String(repeating: "x", count: ClipHistory.maxTextBytes + 1), from: "PC")
        XCTAssertTrue(h.items.isEmpty)
    }

    func testDeleteAndClear() {
        var h = ClipHistory()
        h.add("a", from: "PC", now: 1)
        h.add("b", from: "PC", now: 2)
        h.delete(h.items[0].id)
        XCTAssertEqual(h.items.map(\.text), ["a"])
        h.clear()
        XCTAssertTrue(h.items.isEmpty)
    }

    func testTitleIsTheFirstNonBlankLine() {
        XCTAssertEqual(clipTitle("\n  first \nsecond"), "first")
        XCTAssertEqual(clipTitle("  \n "), "Blank text")
    }
}
