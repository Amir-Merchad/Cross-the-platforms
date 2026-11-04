import Foundation

// In-memory transfer history for the session, folded from core events. No FFI types, so `swift test` runs it.
// Mirrors the Android Activity.kt, except that a failed send to the PC stays queued (OutgoingQueue) and retries by itself.

/// Core events the history cares about. `Model` maps FfiEvent to these. Never carries paths or clipboard text.
public enum TransferEvent: Equatable {
    case started(id: UInt64, peer: String, outgoing: Bool, name: String, files: Int, total: UInt64)
    case progress(id: UInt64, done: UInt64, total: UInt64)
    case received(from: String)
    case requeued(id: UInt64)
    case failed(id: UInt64, reason: String)
}

public enum RowState { case moving, done, failed, waiting }

public struct TransferRow: Equatable, Identifiable {
    public var id: UInt64
    public var peer: String
    public var outgoing: Bool
    public var name: String
    public var files: Int
    public var total: UInt64
    public var done: UInt64
    public var startedAt: Int
    public var now: Int
    public var state: RowState
    public var reason: String?
    public var received = 0   // files received so far, to end incoming rows

    public init(id: UInt64, peer: String, outgoing: Bool, name: String, files: Int, total: UInt64, done: UInt64,
                startedAt: Int, now: Int, state: RowState, reason: String? = nil) {
        self.id = id; self.peer = peer; self.outgoing = outgoing; self.name = name; self.files = files
        self.total = total; self.done = done; self.startedAt = startedAt; self.now = now; self.state = state; self.reason = reason
    }
}

public func nowMs() -> Int { Int(Date().timeIntervalSince1970 * 1000) }

public struct ActivityLog {
    public private(set) var rows: [TransferRow] = []
    public init() {}

    private mutating func patch(_ id: UInt64, _ f: (inout TransferRow) -> Void) {
        if let i = rows.firstIndex(where: { $0.id == id }) { f(&rows[i]) }
    }

    public mutating func apply(event e: TransferEvent, now: Int = nowMs()) {
        switch e {
        case let .started(id, peer, outgoing, name, files, total):
            rows.removeAll { $0.id == id || ($0.state == .waiting && $0.peer == peer) }
            rows.append(TransferRow(id: id, peer: peer, outgoing: outgoing, name: name, files: files, total: total, done: 0, startedAt: now, now: now, state: .moving))
        case let .progress(id, done, total):
            patch(id) { r in
                r.done = done; r.total = total; r.now = now
                if r.outgoing && done >= total { r.state = .done }
            }
        case let .received(from):
            guard let row = rows.first(where: { !$0.outgoing && $0.peer == from && $0.state == .moving }) else { return }
            patch(row.id) { r in
                r.received += 1; r.now = now
                if r.received >= r.files { r.done = r.total; r.state = .done }
            }
        case let .requeued(id):
            patch(id) { $0.state = .waiting; $0.now = now }
        case let .failed(id, reason):
            // A send the queue will try again reads as waiting. One it drops (a permanent error), or a receive, reads as failed.
            if let row = rows.first(where: { $0.id == id }) {
                guard row.state == .moving else { return }
                patch(id) { r in
                    r.state = r.outgoing && shouldKeepBatch(afterError: reason) ? .waiting : .failed
                    r.reason = reason; r.now = now
                }
            } else {
                // No Started came first: a send that failed before it began.
                rows.append(TransferRow(id: id, peer: "", outgoing: true, name: "A file", files: 1, total: 0, done: 0, startedAt: now, now: now,
                                    state: shouldKeepBatch(afterError: reason) ? .waiting : .failed, reason: reason))
            }
        }
    }
}

/// A sentence for the user. Never the raw reason, which can carry paths or internals.
public func plainError(_ reason: String) -> String {
    guard shouldKeepBatch(afterError: reason) else { return "That did not finish. Send it again from the app you started in." }
    let queued = "It stays queued and sends when the PC is back."
    return reason.range(of: "not reachable", options: .caseInsensitive) != nil ? "The PC went away. \(queued)" : "That did not finish. \(queued)"
}

public func displayName(_ name: String?) -> String {
    let t = name?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return t.isEmpty ? "Unnamed device" : t
}

public func formatRate(bytes: UInt64, ms: Int) -> String {
    guard ms > 0 else { return "—" }
    let perS = Double(bytes) / Double(ms) * 1000
    return perS >= 1e6 ? String(format: "%.1f MB/s", perS / 1e6) : "\(Int((perS / 1e3).rounded())) KB/s"
}

// The latest send of ours, if it is not running or done. A row with no peer is a send that failed before it started.
private func lastSend(_ rows: [TransferRow]) -> TransferRow? { rows.last { $0.outgoing || $0.peer.isEmpty } }
private func sendHeldBack(_ rows: [TransferRow]) -> TransferRow? {
    lastSend(rows).flatMap { $0.state == .failed || $0.state == .waiting ? $0 : nil }
}

/// Reachability is not known to the iPad (the hub derives it from Hello), so this reads "Paired" at rest
/// and "PC not reachable" only while the latest send of ours has not got through. Never "Connected".
public func statusLine(paired: Bool, rows: [TransferRow]) -> String {
    !paired ? "Not paired" : sendHeldBack(rows) != nil ? "PC not reachable" : "Paired"
}

/// The warning on Home: the latest send has not got through, so say what happens next. Nil when nothing is wrong.
public func alertLine(_ rows: [TransferRow]) -> String? {
    sendHeldBack(rows).map { "\($0.name) did not send. \(plainError($0.reason ?? ""))" }
}

/// Finished rows for the Recent list, newest first. Running ones show in the progress card instead.
public func recent(_ rows: [TransferRow], _ n: Int = 3) -> [TransferRow] {
    Array(rows.filter { $0.state != .moving }.suffix(n).reversed())
}

public func shortResult(_ r: TransferRow) -> String {
    switch r.state {
    case .failed: return "Failed"
    case .waiting: return "Waiting"
    default: return "\(r.outgoing ? "Sent" : "Received"), \(formatBytes(r.total))"
    }
}

public func percent(_ r: TransferRow) -> Int { r.total == 0 ? 0 : Int(min(r.done * 100 / r.total, 100)) }

public func progressLine(_ r: TransferRow) -> String {
    "\(formatBytes(r.done)) of \(formatBytes(r.total))" + (r.outgoing ? ". Keep Bridge open until this finishes." : "")
}

/// The PC card's second line.
public func pcSub(paired: Bool, rows: [TransferRow]) -> String {
    if !paired { return "Pair once, then everything is one tap." }
    if sendHeldBack(rows) != nil { return "Looking for your PC" }
    guard let moving = rows.last(where: { $0.state == .moving }) else { return "Ready" }
    return moving.outgoing ? "Receiving your files" : "Sending to you"   // the PC's side of it
}

public func ringName(_ r: TransferRow) -> String { r.files > 1 ? "\(r.name) and \(r.files - 1) more" : r.name }

/// Which PC Home shows: the chosen one if it is still paired, else the first.
public func chooseId(_ ids: [String], chosen: String?) -> String? {
    ids.first { $0 == chosen } ?? ids.first
}
