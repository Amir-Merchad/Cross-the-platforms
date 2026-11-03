// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "BridgeKit",
    platforms: [.iOS(.v17), .macOS(.v13)],
    products: [.library(name: "BridgeKit", targets: ["BridgeKit"])],
    targets: [
        .target(name: "BridgeKit"),
        .testTarget(name: "BridgeKitTests", dependencies: ["BridgeKit"]),
    ]
)
