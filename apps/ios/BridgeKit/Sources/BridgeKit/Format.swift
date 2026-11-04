import Foundation

/// Same steps as Android `formatBytes`.
public func formatBytes(_ n: UInt64) -> String {
    switch n {
    case 1_000_000_000...: return String(format: "%.2f GB", Double(n) / 1e9)
    case 1_000_000...: return String(format: "%.1f MB", Double(n) / 1e6)
    case 1_000...: return "\(n / 1000) KB"
    default: return "\(n) B"
    }
}

public func isPairInvite(_ s: String) -> Bool {
    s.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("ctp-pair:")
}

/// A failed send stays queued only if the PC was unreachable; any other failure would repeat forever.
public func shouldKeepBatch(afterError reason: String) -> Bool {
    reason.contains("device not reachable")
}
