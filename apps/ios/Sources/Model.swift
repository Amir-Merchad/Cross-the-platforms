import SwiftUI
import UIKit
import BridgeKit

@MainActor final class Model: ObservableObject, NodeEvents {
    @Published var peers: [FfiPeer] = []
    @Published var status = ""
    @Published var choosing = false

    private var node: FfiNode?
    private var life = NodeLifecycle()
    private var chain: Task<Void, Never>?   // start/stop run one at a time
    private var bgTask = UIBackgroundTaskIdentifier.invalid
    private var afterChoice: ((FfiPeer) -> Void)?
    private var flushing = false, flushAgain = false
    private let queue: OutgoingQueue
    private let nodeDir: URL
    private let documents = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]

    init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        nodeDir = support.appendingPathComponent("node")
        let out = support.appendingPathComponent("outgoing")
        for d in [nodeDir, out] { try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true) }
        queue = OutgoingQueue(root: out)
        queue.sweepIncomplete()
    }

    // MARK: lifecycle

    func phase(_ p: ScenePhase) {
        switch p {
        case .active: run(life.foreground())
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
            refreshPeers()
            flushOutgoing()
        } catch { status = "Could not start: \(error)" }
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
        switch e {
        case .paired: refreshPeers(); status = "Paired"
        case .incoming(_, let files):
            life.transferStarted(files: files.count)
            status = "Receiving \(files.count) file(s)…"
        case .received(_, _, let size):
            run(life.fileFinished())
            status = "Received \(formatBytes(size))"
        case .failed(_, let reason):
            run(life.transferFailed())
            status = "Failed: \(reason)"
        case .progress, .clip, .queued: break
        }
    }

    // MARK: peers

    private func refreshPeers() { peers = node?.peers() ?? [] }

    func forget(_ p: FfiPeer) { node?.forget(id: p.id); refreshPeers() }

    func join(_ qr: String) async {
        guard let node else { status = "Not running"; return }
        do { status = "Paired with \(try await node.join(qrText: qr).name)"; refreshPeers() }
        catch { status = "Pairing failed: \(error)" }
    }

    // MARK: sending

    private func withPC(_ go: @escaping (FfiPeer) -> Void) {
        let pcs = peers.filter { $0.kind == .windows }
        switch pcs.count {
        case 0: status = "Pair with a PC to send"
        case 1: go(pcs[0])
        default: afterChoice = go; choosing = true
        }
    }

    func choose(_ p: FfiPeer) { afterChoice?(p); afterChoice = nil }

    func send(_ urls: [URL]) {
        let q = queue
        status = "Preparing…"
        Task {
            do {
                try await Task.detached {
                    let scoped = urls.map { ($0, $0.startAccessingSecurityScopedResource()) }
                    defer { for (u, ok) in scoped where ok { u.stopAccessingSecurityScopedResource() } }
                    try q.enqueue(urls)
                }.value
                flushOutgoing()
            } catch { status = "Could not copy: \(error.localizedDescription)" }
        }
    }

    func flushOutgoing() {
        guard node != nil else { status = "Starting…"; return }   // start() flushes again
        if flushing { flushAgain = true; return }
        guard !queue.batches().isEmpty else { return }
        withPC { pc in self.deliver(to: pc) }
    }

    private func deliver(to pc: FfiPeer) {
        guard let node else { return }
        flushing = true
        Task {
            var ok = true
            for b in queue.batches() {
                life.transferStarted(files: 1)
                status = "Sending…"
                do {
                    _ = try await node.sendFiles(peerId: pc.id, paths: b.map(\.path))
                    try? queue.remove(batchOf: b[0])
                    run(life.fileFinished())
                    status = "Sent to \(pc.name)"
                } catch {
                    run(life.transferFailed())
                    status = "Send failed, will retry: \(error)"
                    ok = false
                    break
                }
            }
            flushing = false
            let again = flushAgain && ok
            flushAgain = false
            if again { flushOutgoing() }
        }
    }

    func sendClipboard() {
        guard let text = UIPasteboard.general.string, !text.isEmpty else { status = "Clipboard is empty"; return }
        withPC { pc in
            guard let node = self.node else { return }
            Task {
                do { try await node.sendClip(peerId: pc.id, text: text); self.status = "Clipboard sent" }
                catch { self.status = "Send failed: \(error)" }
            }
        }
    }
}
