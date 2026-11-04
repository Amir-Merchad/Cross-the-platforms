import SwiftUI
import UIKit
import VisionKit
import BridgeKit

struct PairSheet: View {
    @ObservedObject var m: Model
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 16) {
            if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
                ScanView { code in join(code) }
            } else {
                Text("Camera unavailable. Copy the code on your PC, then paste it.").padding()
            }
            Button("Paste code") { join(UIPasteboard.general.string ?? "") }.padding(.bottom)
        }
    }

    private func join(_ code: String) {
        guard isPairInvite(code) else { m.status = "That is not a pairing code"; return }
        Task { await m.join(code); dismiss() }
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
