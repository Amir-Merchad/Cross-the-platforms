import XCTest
@testable import BridgeKit

final class AppGroupTests: XCTestCase {
    let base = "group.dev.crosstheplatforms.bridge"

    func testApp() {
        XCTAssertEqual(appGroupCandidates(bundleId: "dev.crosstheplatforms.bridge"), [base])
    }
    func testExtension() {
        XCTAssertEqual(appGroupCandidates(bundleId: "dev.crosstheplatforms.bridge.share"), [base])
    }
    func testSideStoreSuffixedApp() {
        XCTAssertEqual(appGroupCandidates(bundleId: "dev.crosstheplatforms.bridge.ABC123"), [base + ".ABC123", base])
    }
    func testSideStoreSuffixedExtensionBothOrders() {
        let want = [base + ".ABC123", base]
        XCTAssertEqual(appGroupCandidates(bundleId: "dev.crosstheplatforms.bridge.ABC123.share"), want)
        XCTAssertEqual(appGroupCandidates(bundleId: "dev.crosstheplatforms.bridge.share.ABC123"), want)
    }
}
