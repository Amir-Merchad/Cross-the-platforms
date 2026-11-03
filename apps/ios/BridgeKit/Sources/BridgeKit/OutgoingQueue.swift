import Foundation

/// Files picked to send, copied under `root` so they survive until delivered.
/// Layout: `<uuid>/<i>/<name>`; one folder per file keeps equal names apart.
/// A batch is built in `.incoming-<uuid>` and renamed only when fully copied.
public struct OutgoingQueue {
    let root: URL

    public init(root: URL) { self.root = root }

    @discardableResult
    public func enqueue(_ files: [URL]) throws -> URL {
        let fm = FileManager.default
        let id = UUID().uuidString
        let tmp = root.appendingPathComponent(".incoming-" + id)
        let done = root.appendingPathComponent(id)
        do {
            for (i, f) in files.enumerated() {
                let dir = tmp.appendingPathComponent(String(i))
                try fm.createDirectory(at: dir, withIntermediateDirectories: true)
                try fm.copyItem(at: f, to: dir.appendingPathComponent(f.lastPathComponent))
            }
            try fm.moveItem(at: tmp, to: done)
        } catch {
            try? fm.removeItem(at: tmp)
            throw error
        }
        return done
    }

    /// Complete batches, oldest first; each is its files in enqueue order.
    public func batches() -> [[URL]] {
        let fm = FileManager.default
        let keys: [URLResourceKey] = [.creationDateKey]
        let dirs = ((try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles])) ?? [])
            .map { ($0, (try? $0.resourceValues(forKeys: Set(keys)).creationDate) ?? .distantPast) }
            .sorted { ($0.1, $0.0.lastPathComponent) < ($1.1, $1.0.lastPathComponent) }
            .map { $0.0 }
        return dirs.compactMap { batch in
            let subs = ((try? fm.contentsOfDirectory(at: batch, includingPropertiesForKeys: nil)) ?? [])
                .sorted { (Int($0.lastPathComponent) ?? 0) < (Int($1.lastPathComponent) ?? 0) }
            let files = subs.compactMap { (try? fm.contentsOfDirectory(at: $0, includingPropertiesForKeys: nil))?.first }
            return files.isEmpty ? nil : files
        }
    }

    /// `file` is one of the URLs from `batches()`; drops its whole batch.
    public func remove(batchOf file: URL) throws {
        try FileManager.default.removeItem(at: file.deletingLastPathComponent().deletingLastPathComponent())
    }

    /// Deletes batches left half-copied by a crash.
    public func sweepIncomplete() {
        let fm = FileManager.default
        for u in (try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil, options: [])) ?? []
        where u.lastPathComponent.hasPrefix(".incoming-") {
            try? fm.removeItem(at: u)
        }
    }
}
