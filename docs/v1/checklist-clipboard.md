# v1 clipboard acceptance checklist (Windows ↔ iPad)

Run with `target\debug\ctp-app.exe`. Quit from the **tray → Quit** before rebuilding; never kill it.
One-time setup: `%APPDATA%\CrossThePlatforms\ipad\rp-pairing.plist` (or Clipboard tab → Pair iPad with USB)
and `%APPDATA%\CrossThePlatforms\ipad\ddi\{Image.dmg, Image.dmg.trustcache, BuildManifest.plist}`.
iPad: Developer Mode on, same Wi-Fi as the laptop.

| Check | Result |
|---|---|
| Clipboard tab shows "Connected over Wi-Fi" within ~10 s of start | PENDING |
| Windows text → iPad Notes (incl. `héllo 👋 مرحبا`), < 1 s | PENDING |
| iPad text → Windows Notepad, < 1 s | PENDING |
| Windows screenshot (Win+Shift+S) → paste in iPad Notes | PENDING |
| iPad screenshot copy → paste in Windows Paint | PENDING |
| Copy the same thing twice: no flicker or loop; "Synced" goes up once per real copy | PENDING |
| Explorer file copy on Windows → iPad clipboard unchanged | PENDING |
| Toggle off (tray or tab) → nothing syncs either way; on → next copy syncs, nothing replayed | PENDING |
| Tray check mark and Clipboard tab switch always agree | PENDING |
| iPad locked → unlock → sync works within ~5 s | PENDING |
| iPad off Wi-Fi → status says so → back on → reconnects by itself | PENDING |
| iPad reboot → unlock once → disk image remounted, sync back (note if USB was needed) | PENDING |
| Laptop reboot → sync works without opening the app | PENDING |
| Keyboard/mouse sharing still works while clipboard syncs | PENDING |
| `engine.log` has no clipboard text | PENDING |
