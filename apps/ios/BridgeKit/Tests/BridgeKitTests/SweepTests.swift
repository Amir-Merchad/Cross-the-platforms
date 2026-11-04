import XCTest
@testable import BridgeKit

final class SweepTests: XCTestCase {
    func testSweepPartFilesDeletesOnlyParts() throws {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: d) }
        let part = d.appendingPathComponent("a.pdf.part"), keep = d.appendingPathComponent("a.pdf")
        try Data("x".utf8).write(to: part); try Data("y".utf8).write(to: keep)
        sweepPartFiles(in: d)
        XCTAssertFalse(FileManager.default.fileExists(atPath: part.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: keep.path))
    }
}
