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

/// A failed send is dropped only if it would fail the same way again (mirrors core's flush_outbox);
/// a stall, a cut stream or an unreachable PC keeps the batch for the next flush.
/// ponytail: an unknown permanent error retries on every flush; a retry cap is the upgrade path.
public func shouldKeepBatch(afterError reason: String) -> Bool {
    !["can't read a file", "only files can be sent", "declined"].contains { reason.contains($0) }
}

/// A `.part` is a receive the OS killed midway; call at cold launch, before any node starts.
public func sweepPartFiles(in dir: URL) {
    let fm = FileManager.default
    for f in (try? fm.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)) ?? [] where f.pathExtension == "part" {
        try? fm.removeItem(at: f)
    }
}
