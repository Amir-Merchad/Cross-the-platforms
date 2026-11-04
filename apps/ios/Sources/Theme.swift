import SwiftUI
import UIKit

/// Tokens from the Foundations board (dark / light). The action colours are the same in both.
struct Palette {
    let bg, surface, ink, muted, line, tray, screen, danger, warnBg, warnInk: Color
    let action = Color(red: 1, green: 0.773, blue: 0.192)          // #FFC531
    let actionLip = Color(red: 0.843, green: 0.604, blue: 0)        // #D79A00
    let onAction = Color(red: 0.063, green: 0.125, blue: 0.180)     // #10202E, text on the primary colour
    let onScreen = Color(red: 0.933, green: 0.953, blue: 0.973)     // #EEF3F8, text on the PC card (dark in both themes)
    let screenLip = Color(red: 0.051, green: 0.078, blue: 0.110)    // #0D141C

    static let dark = Palette(bg: Color(hex: 0x11171E), surface: Color(hex: 0x1B232D), ink: Color(hex: 0xEEF3F8), muted: Color(hex: 0xA9B7C7),
                              line: Color(hex: 0x2E3A47), tray: Color(hex: 0x18202A), screen: Color(hex: 0x2A3644), danger: Color(hex: 0xFF9A8F),
                              warnBg: Color(hex: 0x3B2F14), warnInk: Color(hex: 0xFFD978))
    static let light = Palette(bg: Color(hex: 0xE9EFF7), surface: Color(hex: 0xFFFFFF), ink: Color(hex: 0x14202E), muted: Color(hex: 0x526274),
                               line: Color(hex: 0xD3DCE8), tray: Color(hex: 0xDCE4EF), screen: Color(hex: 0x1C2836), danger: Color(hex: 0xC4281C),
                               warnBg: Color(hex: 0xFFF1CC), warnInk: Color(hex: 0x6B4A00))
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double(hex >> 16 & 0xFF) / 255, green: Double(hex >> 8 & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }
}

extension EnvironmentValues {
    /// The palette for the system light or dark setting.
    var palette: Palette { colorScheme == .dark ? .dark : .light }
}

/// Rubik is one variable file (UIAppFonts). Its name table calls the family "Rubik Light" and the typographic family "Rubik";
/// ask UIKit which one it registered and use that.
private let rubikFamily: String = UIFont.familyNames.contains("Rubik") ? "Rubik" : "Rubik Light"

// Type scale from Foundations: title 30 bold, heading 22 semibold, body 17 (tablet), caption 14.
extension Font {
    static func rubik(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font { .custom(rubikFamily, size: size).weight(weight) }
    static let rTitle = rubik(30, .bold)
    static let rHeading = rubik(22, .semibold)
    static let rBody = rubik(17)
    static let rBodyStrong = rubik(17, .semibold)
    static let rCaption = rubik(14)
}

/// Primary (amber, 3 pt lip) or secondary (tray) button. Sinks 2 pt while pressed.
struct LipButtonStyle: ButtonStyle {
    var primary = true
    @Environment(\.palette) private var p

    func makeBody(configuration c: Configuration) -> some View {
        let sink: CGFloat = c.isPressed ? 2 : 0
        c.label
            .font(.rBodyStrong)
            .foregroundStyle(primary ? p.onAction : p.ink)
            .frame(maxWidth: .infinity, minHeight: 56)
            .background(RoundedRectangle(cornerRadius: 14).fill(primary ? p.action : p.tray))
            .offset(y: sink)
            .background(RoundedRectangle(cornerRadius: 14).fill(primary ? p.actionLip : p.line).offset(y: 3))
            .padding(.bottom, 3)
    }
}
