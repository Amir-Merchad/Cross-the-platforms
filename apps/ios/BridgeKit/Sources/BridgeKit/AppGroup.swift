import Foundation

private let baseId = "dev.crosstheplatforms.bridge"

/// App group ids to try, best first. SideStore re-signs with `group.<id>.<TEAMID>` and also suffixes
/// bundle ids (order for the extension unconfirmed), so strip ".share" and treat any remainder as the suffix.
public func appGroupCandidates(bundleId: String) -> [String] {
    let plain = "group." + baseId
    guard bundleId.hasPrefix(baseId) else { return [plain] }
    let rest = bundleId.dropFirst(baseId.count).split(separator: ".").filter { $0 != "share" }
    return rest.isEmpty ? [plain] : [plain + "." + rest.joined(separator: "."), plain]
}

/// Shared container of the first candidate group this process was granted, or nil.
public func appGroupContainer(bundleId: String? = Bundle.main.bundleIdentifier) -> URL? {
    appGroupCandidates(bundleId: bundleId ?? baseId)
        .lazy.compactMap { FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: $0) }.first
}
