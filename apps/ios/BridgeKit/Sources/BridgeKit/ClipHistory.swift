import Foundation

// Clips seen while Bridge is open, in memory only: nothing about them is written to disk or logged.
// Same rules as the PC's history: 50 unpinned, pins never expire, a repeat moves to the top, huge text is not kept.

public struct ClipItem: Equatable, Identifiable {
    public let id: Int
    public var text: String
    public var from: String
    public var at: Int
    public var pinned = false
}

public struct ClipHistory {
    public static let maxUnpinned = 50
    public static let maxTextBytes = 1_048_576
    public private(set) var items: [ClipItem] = []
    private var nextId = 1
    public init() {}

    public mutating func add(_ text: String, from: String, now: Int = nowMs()) {
        guard !text.isEmpty, text.utf8.count <= Self.maxTextBytes else { return }
        if let i = items.firstIndex(where: { $0.text == text }) {
            var item = items.remove(at: i)
            item.from = from; item.at = now
            items.insert(item, at: 0)
            return
        }
        items.insert(ClipItem(id: nextId, text: text, from: from, at: now), at: 0)
        nextId += 1
        while items.filter({ !$0.pinned }).count > Self.maxUnpinned, let i = items.lastIndex(where: { !$0.pinned }) { items.remove(at: i) }
    }

    public mutating func setPinned(_ id: Int, _ pinned: Bool) {
        guard let i = items.firstIndex(where: { $0.id == id }) else { return }
        items[i].pinned = pinned
    }

    public mutating func delete(_ id: Int) { items.removeAll { $0.id == id } }
    public mutating func clear() { items.removeAll() }

    /// Text containing `q`, ignoring case. An empty query lists everything.
    public func search(_ q: String) -> [ClipItem] {
        q.isEmpty ? items : items.filter { $0.text.range(of: q, options: .caseInsensitive) != nil }
    }
}

/// First non-blank line, for a list row.
public func clipTitle(_ text: String) -> String {
    text.split(whereSeparator: \.isNewline).map { $0.trimmingCharacters(in: .whitespaces) }.first { !$0.isEmpty } ?? "Blank text"
}
