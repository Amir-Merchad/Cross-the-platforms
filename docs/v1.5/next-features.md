# Next features (owner's list, 2026-11-05 morning)

To be done in a **local** Claude Code session on the owner's laptop, not in the cloud. Start by reading `docs/v1.5/morning.md` (state of the branch, tests in order, calls to check) and running its device tests with the owner.

Branch: `v1.5/input-27ztiw`. Standing rules: push only to `v1.5/*`, no merge/PR/release, never log file names, keys or clipboard text, never read pairing records, never commit `.idea/`, `.red.txt` or `apps/tauri/vite.mock.config.ts`. For each feature: design in `docs/superpowers/specs/`, plan in `docs/superpowers/plans/`, build with tests, review, commit, push; mark every design choice as a call for the owner.

## The list

1. **Clipboard history with search**, on the PC and the iPad: the last 50 copies (text and images) from any device, pinned items for formulas or citations reused often. Stored locally, never logged.
2. **Keep formatting when copying**: rich text and HTML alongside plain text, so a copy from a PDF or web page into notes keeps bold, lists and links.
3. **Send to the device under the pointer**: a hotkey that pastes the clipboard on whichever device is being controlled, or drops the selected file there.
4. **Drop a file onto a device on the Desk** to send it.
5. **Screenshot to clipboard everywhere**: a PC hotkey that takes a screenshot of the iPad (the spike's screen capture already works over the tunnel: `ScreenCaptureServiceClient`, see `docs/spikes/ipad-pointer.md`) and puts it on the PC clipboard. For lecture slides on the iPad.
6. **Focus mode**: one switch that silences notifications on the phone while studying on the PC and iPad (needs the phone's Do Not Disturb permission).
7. **Quick notes inbox**: text sent from the phone lands in one running note on the PC instead of only the clipboard, so nothing is lost when the next copy overwrites it.
8. **Latency number**: a camera test (slow-motion video of a keypress and the iPad screen) to measure real input lag and tune the 8 ms pacing.
9. **Smoother scroll**: touchpad sub-notch scrolling mapped to smaller iPad scroll steps (report 7, about 2 px per unit, 6 px floor) for reading long pages.
10. **Per-device keyboard layout**: German on the PC, translated for the iPad and phone automatically (the iPad treats the virtual keyboard as US; `core/src/androidkeys.rs` exists for Android).
11. **Battery and link on the Desk**: the iPad's and phone's battery and link quality next to each device.
12. **Bluetooth fallback**: once the adapter is stable again, use Bluetooth automatically when the iPad's Wi-Fi route fails (an iPadOS update can break the private interface).
13. **Gestures between all devices**: two-, three- and four-finger gestures (pinch, scroll, swipes) carried across devices. Note from the proofs: `dtuhidd` links gestures for absolute pointer, scroll, dock swipe, navigation swipe, translation, fluid touch, scale, rotation and zoom toggle, and several report IDs (03, 04, 0B to 13, 81) are still unknown, so the iPad side likely needs a report-ID spike first.

## Suggested order (call)

Without devices first: 1, 2, 7, 4, 3, 9, 10. Then with devices: 5, 11, 12, 13, 6, 8.
