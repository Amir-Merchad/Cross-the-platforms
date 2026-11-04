import UIKit
import UniformTypeIdentifiers
import BridgeKit

final class ShareViewController: UIViewController {
    private let label = UILabel()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        label.text = "Saving…"
        label.numberOfLines = 0
        label.textAlignment = .center
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            label.leadingAnchor.constraint(equalTo: view.layoutMarginsGuide.leadingAnchor),
            label.trailingAnchor.constraint(equalTo: view.layoutMarginsGuide.trailingAnchor),
        ])
        save()
    }

    private func finish(_ msg: String, after: TimeInterval) {
        DispatchQueue.main.async {
            self.label.text = msg
            DispatchQueue.main.asyncAfter(deadline: .now() + after) {
                self.extensionContext?.completeRequest(returningItems: nil)
            }
        }
    }

    private func save() {
        guard let group = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.dev.crosstheplatforms.bridge") else {
            finish("Could not save: app group unavailable", after: 2.5)
            return
        }
        let root = group.appendingPathComponent("outgoing")
        let providers = (extensionContext?.inputItems as? [NSExtensionItem] ?? []).flatMap { $0.attachments ?? [] }
        guard !providers.isEmpty else { finish("Nothing to save", after: 1.5); return }

        let fm = FileManager.default
        let scratch = fm.temporaryDirectory.appendingPathComponent("share-" + UUID().uuidString)
        let lock = NSLock()
        var copied: [Int: URL] = [:]
        let g = DispatchGroup()
        for (i, p) in providers.enumerated() {
            g.enter()
            let type = [UTType.item, .data].first { p.hasItemConformingToTypeIdentifier($0.identifier) }
            guard let type else { g.leave(); continue }
            // the provided URL is only valid inside this callback: copy now, one folder per item keeps the name
            p.loadFileRepresentation(forTypeIdentifier: type.identifier) { url, _ in
                defer { g.leave() }
                guard let url else { return }
                let dir = scratch.appendingPathComponent(String(i))
                let dest = dir.appendingPathComponent(url.lastPathComponent)
                do {
                    try fm.createDirectory(at: dir, withIntermediateDirectories: true)
                    try fm.copyItem(at: url, to: dest)
                    lock.lock(); copied[i] = dest; lock.unlock()
                } catch {}
            }
        }
        g.notify(queue: .global()) {
            defer { try? fm.removeItem(at: scratch) }
            let files = copied.sorted { $0.key < $1.key }.map(\.value)
            guard !files.isEmpty else { self.finish("Could not read the shared items", after: 2.5); return }
            do {
                try fm.createDirectory(at: root, withIntermediateDirectories: true)
                try OutgoingQueue(root: root).enqueue(files)
                let note = files.count < providers.count ? "Saved \(files.count) of \(providers.count) — open Bridge to send" : "Saved — open Bridge to send"
                self.finish(note, after: 1.2)
            } catch {
                self.finish("Could not save", after: 2.5)
            }
        }
    }
}
