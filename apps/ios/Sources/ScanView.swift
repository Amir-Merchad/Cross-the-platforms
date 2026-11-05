import SwiftUI
import UIKit
import AVFoundation
import VisionKit
import CoreImage.CIFilterBuiltins
import BridgeKit

struct PairSheet: View {
    @ObservedObject var m: Model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.palette) private var p
    @State private var available = false
    @State private var busy = false
    @State private var showCode = false

    var body: some View {
        VStack(spacing: 14) {
            if showCode {
                if let qr = m.invite(), let img = qrImage(qr) {
                    Image(uiImage: img).interpolation(.none).resizable().aspectRatio(1, contentMode: .fit).frame(maxWidth: 320)
                        .padding(16).background(RoundedRectangle(cornerRadius: 22).fill(.white))
                    Text("Scan this on your phone or another iPad, in Bridge, Pair. Works once, for 5 minutes.")
                        .font(.rHeading).foregroundStyle(p.ink).multilineTextAlignment(.center).frame(maxWidth: 440)
                } else {
                    Text("Bridge is still starting. Try again in a moment.").foregroundStyle(p.muted)
                }
            } else if available {
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
            if !showCode {
                Button("Paste code") { join(UIPasteboard.general.string ?? "") }
                    .buttonStyle(LipButtonStyle()).frame(maxWidth: 280)
            }
            Button(showCode ? "Scan a code instead" : "Show my code for another device") { showCode.toggle() }
                .buttonStyle(LipButtonStyle(primary: false)).frame(maxWidth: 280)
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

/// A crisp QR code for a pairing code, drawn by Core Image.
func qrImage(_ text: String) -> UIImage? {
    let f = CIFilter.qrCodeGenerator()
    f.message = Data(text.utf8)
    f.correctionLevel = "L"
    guard let out = f.outputImage?.transformed(by: CGAffineTransform(scaleX: 8, y: 8)),
          let cg = CIContext().createCGImage(out, from: out.extent) else { return nil }
    return UIImage(cgImage: cg)
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
