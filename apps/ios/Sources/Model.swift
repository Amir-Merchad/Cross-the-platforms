import SwiftUI
import UIKit
import BridgeKit

@MainActor final class Model: ObservableObject, NodeEvents {
    @Published var peers: [FfiPeer] = []
    @Published private(set) var rows: [TransferRow] = []
    @Published private(set) var startFailed = false
    @Published var notice: String?          // one-off messages that are not a transfer (empty clipboard, bad code)
    @Published private(set) var success: String?   // a one-off good result; clears by itself
    @Published var chosen: String?          // the PC Home shows; see `shown`
    @Published private(set) var shareFolder = ""

    private var node: FfiNode?
    private var life = NodeLifecycle()
    private var chain: Task<Void, Never>?   // start/stop run one at a time
    private var bgTask = UIBackgroundTaskIdentifier.invalid
    private var log = ActivityLog()
    private var flushing = false, flushAgain = false
    private var successClear: Task<Void, Never>?
    private let queue: OutgoingQueue          // app's own root; send() enqueues here
    private let queues: [OutgoingQueue]        // + app group root (share extension), nil group = local only
    private let nodeDir: URL
    private let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]

    init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        nodeDir = support.appendingPathComponent("node")
        let out = support.appendingPathComponent("outgoing")
        for d in [nodeDir, out] { try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true) }
        queue = OutgoingQueue(root: out)
        var all = [queue]
        if let g = appGroupContainer() {
            let shared = g.appendingPathComponent("outgoing")
            try? FileManager.default.createDirectory(at: shared, withIntermediateDirectories: true)
            all.append(OutgoingQueue(root: shared))
        }
        queues = all
        shareFolder = all.count > 1 ? "Ready" : "unavailable (\(Bundle.main.bundleIdentifier ?? "?"))"   // the bundle id is for diagnosis
        sweepPartFiles(in: documents)   // receives the OS killed midway; no node is running yet
        queues.forEach { $0.sweepIncomplete() }   // default 1h: the extension may be mid-copy
    }

    // MARK: lifecycle

    func phase(_ p: ScenePhase) {
        switch p {
        case .active:
            run(life.foreground())
            if node != nil { flushOutgoing() }   // node survives backgrounding with files pending; else start() flushes
        case .background: run(life.background())
        default: break
        }
    }

    private func run(_ actions: [LifecycleAction]) {
        for a in actions {
            switch a {
            case .none: break
            case .start: enqueue { await self.start() }
            case .stop: enqueue { await self.stop() }
            case .beginBackgroundTime:
                bgTask = UIApplication.shared.beginBackgroundTask { [weak self] in
                    MainActor.assumeIsolated { self?.bgExpired() }
                }
            case .endBackgroundTime: enqueue { self.endBg() }
            }
        }
    }

    private func enqueue(_ op: @escaping @MainActor () async -> Void) {
        let prev = chain
        chain = Task { await prev?.value; await op() }
    }

    private func start() async {
        guard node == nil else { return }
        do {
            node = try await startNode(dataDir: nodeDir.path, receiveDir: documents.path, name: UIDevice.current.name, kind: .ipad, events: self)
            startFailed = false
            refreshPeers()
            flushOutgoing()
        } catch { startFailed = true }
    }

    private func stop() async {
        let n = node
        node = nil
        await n?.shutdown()
    }

    private func bgExpired() {
        run(life.backgroundTimeExpired())
        endBg()   // iOS requires this before the handler returns
    }

    private func endBg() {
        guard bgTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(bgTask)
        bgTask = .invalid
    }

    // MARK: events (background thread)

    nonisolated func onEvent(e: FfiEvent) { Task { @MainActor in self.handle(e) } }

    private func handle(_ e: FfiEvent) {
        if let t = transferEvent(e) { log.apply(event: t); rows = log.rows }
        switch e {
        case .paired: refreshPeers()
        case .incoming(_, let files): life.transferStarted(files: files.count)
        case .received: run(life.fileFinished())
        case .failed: run(life.transferFailed())
        case .clip(let from, let text):
            // Straight from another of the owner's devices (the node already dropped repeats and echoes).
            // The PC's own sync writes the pasteboard by itself; this is for phones and other iPads.
            UIPasteboard.general.string = text
            say(success: "Clipboard from \(displayName(peers.first { $0.id == from }?.name))")
        case .progress, .queued, .started, .requeued: break
        }
    }

    /// Never carries paths or clipboard text into the log.
    private func transferEvent(_ e: FfiEvent) -> TransferEvent? {
        switch e {
        case .started(let id, let peer, let outgoing, let name, let files, let total):
            return .started(id: id, peer: peer, outgoing: outgoing, name: name, files: Int(files), total: total)
        case .progress(let id, let done, let total): return .progress(id: id, done: done, total: total)
        case .received(let from, _, _): return .received(from: from)
        case .requeued(let id): return .requeued(id: id)
        case .failed(let id, let reason): return .failed(id: id, reason: reason)
        default: return nil
        }
    }

    // MARK: peers

    private func refreshPeers() { peers = node?.peers() ?? [] }

    func forget(_ p: FfiPeer) { node?.forget(id: p.id); refreshPeers() }

    var pcs: [FfiPeer] { peers.filter { $0.kind == .windows } }
    /// Which PC Home shows, and where Send files and Send clipboard go.
    var shown: FfiPeer? { chooseId(pcs.map(\.id), chosen: chosen).flatMap { id in pcs.first { $0.id == id } } }

    @discardableResult func join(_ qr: String) async -> Bool {
        guard let node else { notice = "Bridge is still starting. Try again in a moment."; return false }
        do { chosen = try await node.join(qrText: qr).id; notice = nil; refreshPeers(); return true }
        catch { notice = "That code did not work. Show a new one on the PC and try again."; return false }
    }

    // MARK: sending

    private func withPC(_ go: @escaping (FfiPeer) -> Void) {
        if let pc = shown { go(pc) } else { notice = "Pair with a PC to send" }
    }

    func send(_ urls: [URL]) {
        let q = queue
        notice = nil
        Task {
            do {
                let queued = try await Task.detached { () -> Bool in
                    let scoped = urls.map { ($0, $0.startAccessingSecurityScopedResource()) }
                    defer { for (u, ok) in scoped where ok { u.stopAccessingSecurityScopedResource() } }
                    guard urls.allSatisfy({ (try? $0.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true }) else { return false }
                    try q.enqueue(urls)
                    return true
                }.value
                if queued { flushOutgoing() } else { notice = "Only files can be sent, not folders or packages" }
            } catch { notice = "Could not copy: \(error.localizedDescription)" }
        }
    }

    func flushOutgoing() {
        guard node != nil else { return }   // start() flushes again
        if flushing { flushAgain = true; return }
        guard !pending().isEmpty else { return }
        withPC { pc in self.deliver(to: pc) }
    }

    private func pending() -> [[URL]] { queues.flatMap { $0.batches() } }
    private func drop(_ b: [URL]) { queues.forEach { try? $0.remove(batchOf: b[0]) } }   // remove ignores foreign roots

    private func deliver(to pc: FfiPeer) {
        guard let node else { return }
        flushing = true
        Task {
            var ok = true
            for b in pending() {
                life.transferStarted(files: 1)
                do {
                    _ = try await node.sendFiles(peerId: pc.id, paths: b.map(\.path))
                    drop(b)
                    run(life.fileFinished())
                } catch {
                    // core already emitted .failed, which the log shows as waiting (kept, retried by itself) or failed (dropped)
                    if shouldKeepBatch(afterError: "\(error)") {
                        ok = false
                        break
                    }
                    drop(b)   // would fail the same way again
                }
            }
            flushing = false
            let again = flushAgain && ok
            flushAgain = false
            if again { flushOutgoing() }
        }
    }

    private func say(success text: String) {
        success = text
        successClear?.cancel()
        successClear = Task {
            try? await Task.sleep(for: .seconds(4))
            if !Task.isCancelled { success = nil }
        }
    }

    func sendClipboard() {
        notice = nil
        success = nil
        guard let text = UIPasteboard.general.string, !text.isEmpty else { notice = "Clipboard is empty"; return }
        guard let node, !peers.isEmpty else { notice = "Pair with a device to send"; return }
        // To every paired device at once: the PC, phones, other iPads.
        Task {
            let n = await node.shareClip(text: text)
            if n > 0 { self.say(success: n == 1 ? "Clipboard sent" : "Clipboard sent to \(n) devices") }
            else { self.notice = "Nothing new to send, or no device answered. Open Bridge on the other device and try again." }
        }
    }

    /// A one-time pairing code for another phone or iPad to scan. Nil while Bridge starts.
    func invite() -> String? { node?.invite().qr }
}
