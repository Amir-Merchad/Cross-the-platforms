import SwiftUI
import UIKit
import AVFoundation
import VisionKit
import BridgeKit

struct PairSheet: View {
    @ObservedObject var m: Model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var p
    @State private var available = false
    @State private var busy = false

    var body: some View {
        VStack(spacing: 14) {
            if available {
                ScanView { code in join(code) }
                    .frame(maxWidth: 440).aspectRatio(1, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 22))
                    .overlay(RoundedRectangle(cornerRadius: 22).stroke(p.action, lineWidth: 3))
                Text("Point the camera at the code on your PC").font(.rHeading).foregroundStyle(p.ink).multilineTextAlignment(.center)
            } else {
                Text("The camera is off for Bridge").font(.rHeading).foregroundStyle(p.ink).multilineTextAlignment(.center)
                Text("On the PC, press Copy code and get it to this device. Your clipboard sync can carry it. Then paste it here.")
                    .foregroundStyle(p.muted).multilineTextAlignment(.center).frame(maxWidth: 440)
            }
            if let n = m.notice {
                Text(n).font(.rBodyStrong).foregroundStyle(p.warnInk).multilineTextAlignment(.center)
            }
            Button("Paste code") { join(UIPasteboard.general.string ?? "") }
                .buttonStyle(LipButtonStyle()).frame(maxWidth: 280)
            Button("Cancel") { dismiss() }.font(.rBodyStrong.weight(.medium)).foregroundStyle(p.muted).frame(minHeight: 44)
        }
        .padding(40)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(p.bg.ignoresSafeArea())
        .task {
            m.notice = nil
            _ = await AVCaptureDevice.requestAccess(for: .video)
            available = DataScannerViewController.isSupported && DataScannerViewController.isAvailable
        }
    }

    private func join(_ code: String) {
        guard !busy else { return }
        guard isPairInvite(code) else { m.notice = "That is not a pairing code"; return }
        busy = true
        Task {
            if await m.join(code) { dismiss() }
            busy = false
        }
    }
}

struct ScanView: UIViewControllerRepresentable {
    let onCode: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onCode) }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let v = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])])
        v.delegate = context.coordinator
        return v
    }

    func updateUIViewController(_ v: DataScannerViewController, context: Context) {
        if !v.isScanning { try? v.startScanning() }
    }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onCode: (String) -> Void
        private var done = false
        init(_ onCode: @escaping (String) -> Void) { self.onCode = onCode }

        func dataScanner(_ s: DataScannerViewController, didAdd added: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !done else { return }
            for case .barcode(let b) in added {
                if let t = b.payloadStringValue, isPairInvite(t) { done = true; onCode(t); return }
            }
        }
    }
}
