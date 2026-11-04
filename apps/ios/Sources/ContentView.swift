import SwiftUI
import BridgeKit

/// How long Forget must be held. Letting go earlier, or a scroll taking the touch, cancels.
let holdToForgetSeconds = 0.9

struct ContentView: View {
    @ObservedObject var m: Model
    @Environment(\.scenePhase) private var phase
    @Environment(\.palette) private var p
    @Environment(\.horizontalSizeClass) private var width
    @State private var picking = false
    @State private var pairing = false

    var body: some View {
        let pc = m.shown
        let paired = pc != nil
        let moving = m.rows.last { $0.state == .moving }
        let alert: String? = m.startFailed ? "Bridge could not start. Close it and open it again." : alertLine(m.rows) ?? m.notice
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                HStack {
                    Text("Bridge").font(.rTitle).foregroundStyle(p.ink)
                    Spacer()
                    Text(statusLine(paired: paired, rows: m.rows)).font(.rCaption).foregroundStyle(p.muted)
                }
                // Two columns on the iPad, one when the window is narrow (Split View, Slide Over).
                let layout = width == .regular ? AnyLayout(HStackLayout(alignment: .top, spacing: 18)) : AnyLayout(VStackLayout(alignment: .leading, spacing: 18))
                layout {
                    left(pc: pc, moving: moving, alert: alert)
                    right(pc: pc)
                }
            }
            .padding(width == .regular ? 40 : 20)
        }
        .background(p.bg.ignoresSafeArea())
        .font(.rBody)
        .onChange(of: phase, initial: true) { _, ph in m.phase(ph) }
        .fileImporter(isPresented: $picking, allowedContentTypes: [.item], allowsMultipleSelection: true) { r in
            if case .success(let urls) = r { m.send(urls) }
        }
        .sheet(isPresented: $pairing, onDismiss: { m.flushOutgoing() }) { PairSheet(m: m) }
    }

    private func left(pc: FfiPeer?, moving: TransferRow?, alert: String?) -> some View {
        let paired = pc != nil
        return VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 0) {
                Spacer(minLength: 0)
                Text(pc.map { displayName($0.name) } ?? "No PC yet").font(.rBodyStrong)
                Text(pcSub(paired: paired, rows: m.rows)).font(.rCaption).opacity(0.85)
            }
            .foregroundStyle(p.onScreen)
            .frame(maxWidth: .infinity, minHeight: width == .regular ? 320 : 150, alignment: .leading)
            .padding(.horizontal, 18).padding(.vertical, 14)
            .background(RoundedRectangle(cornerRadius: 14).fill(p.screen))
            .background(RoundedRectangle(cornerRadius: 14).fill(p.screenLip).offset(y: 5))
            .padding(.bottom, 5)

            if m.pcs.count > 1 {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(m.pcs, id: \.id) { c in
                            let on = c.id == pc?.id
                            Button { m.chosen = c.id } label: {
                                Text(displayName(c.name)).font(.rBodyStrong)
                                    .foregroundStyle(on ? p.onAction : p.ink)
                                    .padding(.horizontal, 16).frame(minHeight: 44)
                                    .background(RoundedRectangle(cornerRadius: 12).fill(on ? p.action : p.tray))
                            }
                            .buttonStyle(.plain)
                            .accessibilityAddTraits(on ? .isSelected : [])
                        }
                    }
                }
            }
            if let r = moving { progressCard(r) }
            if let alert {
                Text(alert).font(.rBodyStrong).foregroundStyle(p.warnInk)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14).padding(.vertical, 12)
                    .background(RoundedRectangle(cornerRadius: 14).fill(p.warnBg))
            }
            if let done = m.success {
                Text(done).font(.rBody).foregroundStyle(p.ink)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14).padding(.vertical, 12)
                    .background(RoundedRectangle(cornerRadius: 14).fill(p.surface))
            }
            Button(paired ? "Send files" : "Pair with PC") { if paired { picking = true } else { pairing = true } }
                .buttonStyle(LipButtonStyle())
            if paired { Button("Send clipboard") { m.sendClipboard() }.buttonStyle(LipButtonStyle(primary: false)) }
        }
        .frame(maxWidth: .infinity)
    }

    private func progressCard(_ r: TransferRow) -> some View {
        let rate = formatRate(bytes: r.done, ms: r.now - r.startedAt)
        return VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(ringName(r)).font(.rBodyStrong.weight(.medium)).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 12)
                if rate != "—" { Text(rate).font(.rBodyStrong.weight(.medium)) }
            }
            ProgressView(value: Double(percent(r)), total: 100).tint(p.action).padding(.vertical, 4)
            Text(progressLine(r)).font(.rCaption).foregroundStyle(p.muted)
        }
        .foregroundStyle(p.ink)
        .padding(.horizontal, 16).padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(RoundedRectangle(cornerRadius: 18).fill(p.surface))
    }

    @ViewBuilder private func right(pc: FfiPeer?) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            if let pc {
                section {
                    Text("Recent").font(.rBodyStrong).padding(.vertical, 8)
                    let shown = recent(m.rows)
                    if shown.isEmpty { line("Nothing yet", "", p.muted) }
                    ForEach(shown) { r in
                        line(r.name, shortResult(r), r.state == .failed || r.state == .waiting ? p.danger : p.muted)
                    }
                }
                section {
                    line("Received files", "Files, under Bridge", p.muted, divider: false)
                    // The bundle id stays when the app group is unavailable, so a failed share can be diagnosed.
                    line("Sharing from other apps", m.shareFolder, p.muted)
                }
                HoldToForget(label: "Hold to forget this PC") { m.forget(pc) }
            } else {
                Text("On the PC, open Bridge, choose Add a device, and show the code. Then scan it here.").foregroundStyle(p.muted)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func section<C: View>(@ViewBuilder _ content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 0, content: content)
            .foregroundStyle(p.ink)
            .padding(.horizontal, 16).padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: 18).fill(p.surface))
    }

    private func line(_ left: String, _ right: String, _ color: Color, divider: Bool = true) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(left).lineLimit(1).truncationMode(.middle)
            Spacer(minLength: 12)
            Text(right).font(.rCaption).foregroundStyle(color)
        }
        .padding(.vertical, 10)
        .overlay(alignment: .top) { if divider { Rectangle().fill(p.line).frame(height: 1) } }
    }
}

/// Forgets only once the finger has stayed down for `holdToForgetSeconds`. Lifting early, or a scroll taking the touch, does nothing.
struct HoldToForget: View {
    let label: String
    let onForget: () -> Void
    @Environment(\.palette) private var p
    @State private var held = false

    var body: some View {
        Text(label).font(.rBodyStrong).foregroundStyle(p.danger)
            .frame(maxWidth: .infinity, minHeight: 44)
            .background(alignment: .leading) {
                GeometryReader { g in Rectangle().fill(p.danger.opacity(0.25)).frame(width: held ? g.size.width : 0) }
            }
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(p.line, lineWidth: 2))
            .animation(held ? .linear(duration: holdToForgetSeconds) : .easeOut(duration: 0.15), value: held)
            .contentShape(Rectangle())
            // perform runs only when the full duration passes with the finger still down; a cancel only resets `held`.
            .onLongPressGesture(minimumDuration: holdToForgetSeconds, maximumDistance: 10, perform: onForget) { held = $0 }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label)
            .accessibilityAddTraits(.isButton)
            // VoiceOver users cannot hold, so the action is offered directly.
            .accessibilityAction(named: "Forget this PC", onForget)
    }
}
