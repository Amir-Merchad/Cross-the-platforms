import SwiftUI
import UIKit

/// Spike UI: receive into Documents (visible in Files → On My iPad → CTP Spike), send a picked file.
struct ContentView: View {
    @AppStorage("ticket") private var sendTicket = ""
    @State private var myTicket: String?
    @State private var log = ""
    @State private var picking = false

    private var documents: URL { FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0] }

    var body: some View {
        Form {
            Section("Send to (ticket from bridge-recv)") {
                TextField("ticket", text: $sendTicket, axis: .vertical).lineLimit(3...6)
                Button("Pick file and send") { picking = true }.disabled(sendTicket.isEmpty)
            }
            Section("Receive") {
                Button("Start receiving") { startReceiving() }.disabled(myTicket != nil)
                if let t = myTicket {
                    Text(t).font(.caption).textSelection(.enabled)
                    Button("Copy my ticket") { UIPasteboard.general.string = t }
                }
            }
            Section { Text(log) }
        }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.item]) { result in
            guard case .success(let url) = result else { return }
            send(url)
        }
    }

    private func startReceiving() {
        Task {
            do {
                let rx = try await startReceiver(dir: documents.path)
                myTicket = rx.ticket()
                while true {
                    let t0 = Date()
                    do {
                        let r = try await rx.next()
                        log = String(format: "received %llu bytes (%.1f MB/s incl. wait)", r.size, Double(r.size) / 1e6 / Date().timeIntervalSince(t0))
                    } catch { log = "receive failed: \(error)" }
                }
            } catch { log = "receiver failed to start: \(error)" }
        }
    }

    private func send(_ url: URL) {
        Task {
            log = "sending…"
            do {
                // Copy into tmp so Rust gets a plain path that outlives the security scope.
                let scoped = url.startAccessingSecurityScopedResource()
                let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(url.lastPathComponent)
                try? FileManager.default.removeItem(at: tmp)
                try FileManager.default.copyItem(at: url, to: tmp)
                if scoped { url.stopAccessingSecurityScopedResource() }
                let t0 = Date()
                let n = try await sendFileFfi(ticket: sendTicket.trimmingCharacters(in: .whitespacesAndNewlines), path: tmp.path)
                let s = Date().timeIntervalSince(t0)
                log = String(format: "sent %llu bytes in %.2fs = %.1f MB/s", n, s, Double(n) / 1e6 / s)
            } catch { log = "send failed: \(error)" }
        }
    }
}
