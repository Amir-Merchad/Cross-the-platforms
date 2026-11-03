# v1 clipboard acceptance checklist (Windows ↔ iPad)

Run with `target\debug\ctp-app.exe`. Quit from the **tray → Quit** before rebuilding; never kill it.
One-time setup: `%APPDATA%\CrossThePlatforms\ipad\rp-pairing.plist` (or Clipboard tab → Pair iPad with USB)
and `%APPDATA%\CrossThePlatforms\ipad\ddi\{Image.dmg, Image.dmg.trustcache, BuildManifest.plist}`.
iPad: Developer Mode on, same Wi-Fi as the laptop.

| Check | Result |
|---|---|
| Clipboard tab shows "Connected over Wi-Fi" within ~10 s of start | **PASS** 2026-11-03 (build 0d45c13): connected over Wi-Fi 0.6 s after start (earlier run showed "Not paired"; not seen again) |
| Windows text → iPad Notes (incl. `héllo 👋 مرحبا`), < 1 s | **PASS** 2026-11-03 (USB) |
| iPad text → Windows Notepad, < 1 s | **PASS** 2026-11-03 (USB) |
| Windows screenshot (Win+Shift+S) → paste in iPad Notes | **PASS** 2026-11-03 (USB) |
| iPad screenshot copy → paste in Windows Paint | **PASS** 2026-11-03 (build 0d45c13): pasted at full quality; Windows write verified by test (CF_DIBV5 + PNG, readable by other apps). Earlier failure not reproduced |
| Copy the same thing twice: no flicker or loop; "Synced" goes up once per real copy | **PASS** 2026-11-03 |
| Explorer file copy on Windows → iPad clipboard unchanged | **PASS** 2026-11-03 |
| Toggle off (tray or tab) → nothing syncs either way; on → next copy syncs, nothing replayed | **PASS** 2026-11-03 |
| Tray check mark and Clipboard tab switch always agree | **PASS** 2026-11-04 (owner-checked) |
| iPad locked → unlock → sync works within ~5 s | **PASS** 2026-11-04 (owner-checked) |
| iPad off Wi-Fi → status says so → back on → reconnects by itself | **PASS** 2026-11-04 (owner-checked) |
| iPad reboot → unlock once → disk image remounted, sync back (note if USB was needed) | **PASS** 2026-11-04 (owner-checked) |
| Laptop reboot → sync works without opening the app | **PASS** 2026-11-04 (owner-checked) |
| Keyboard/mouse sharing still works while clipboard syncs | **PASS** 2026-11-03 (build 0d45c13): "mouse, keyboard and copy-paste run smoothly" |
| `engine.log` has no clipboard text | **PASS** 2026-11-03: only sizes (`clip image -> Windows (22380544 bytes)`) |
