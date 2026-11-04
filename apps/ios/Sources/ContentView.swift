import SwiftUI

struct ContentView: View {
    @ObservedObject var m: Model
    @Environment(\.scenePhase) private var phase
    @State private var picking = false
    @State private var pairing = false

    private var pcs: [FfiPeer] { m.peers.filter { $0.kind == .windows } }

    var body: some View {
        Form {
            Section("PCs") {
                ForEach(pcs, id: \.id) { p in
                    HStack {
                        Text(p.name)
                        Spacer()
                        Button("Forget", role: .destructive) { m.forget(p) }.buttonStyle(.borderless)
                    }
                }
                Button("Pair with PC") { pairing = true }
            }
            Section {
                Button("Send files…") { picking = true }
                Button("Send clipboard to PC") { m.sendClipboard() }
            }
            Section { Text("Share folder: \(m.shareFolder)").font(.footnote).foregroundStyle(.secondary) }
            if !m.status.isEmpty { Section { Text(m.status) } }
        }
        .onChange(of: phase, initial: true) { _, p in m.phase(p) }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.item], allowsMultipleSelection: true) { r in
            if case .success(let urls) = r { m.send(urls) }
        }
        .sheet(isPresented: $pairing, onDismiss: { m.flushOutgoing() }) { PairSheet(m: m) }
        .confirmationDialog("Send to which PC?", isPresented: $m.choosing) {
            ForEach(pcs, id: \.id) { p in Button(p.name) { m.choose(p) } }
        }
    }
}
