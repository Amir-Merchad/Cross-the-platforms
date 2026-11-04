import XCTest
@testable import BridgeKit

final class RetryTests: XCTestCase {
    func testKeepsBatchWhenPCUnreachable() {
        XCTAssertTrue(shouldKeepBatch(afterError: "device not reachable: timed out"))
    }
    func testKeepsBatchWhenStreamStalls() {
        XCTAssertTrue(shouldKeepBatch(afterError: "connection stalled"))
    }
    func testDropsBatchOnlyOnPermanentErrors() {
        for e in ["can't read a file: gone", "only files can be sent", "declined: no"] {
            XCTAssertFalse(shouldKeepBatch(afterError: e), e)
        }
    }
}
