import XCTest
@testable import BridgeKit

final class RetryTests: XCTestCase {
    func testKeepsBatchWhenPCUnreachable() {
        XCTAssertTrue(shouldKeepBatch(afterError: "device not reachable: timed out"))
    }
    func testDropsBatchOnOtherErrors() {
        XCTAssertFalse(shouldKeepBatch(afterError: "only files can be sent"))
    }
}
