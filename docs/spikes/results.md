# Spike results

## Summary (Task 8 — fill in after the device checks)

| Spike | What it proves | Status | Path if it fails |
|---|---|---|---|
| 1 iPad clipboard via `idevice` | no-app clipboard Windows ↔ iPad | code ready, PENDING (owner) | clipboard via iPad app only |
| 2 Rust BLE HID → iPad | own Bluetooth keyboard/mouse | service publishes ✔, pairing PENDING (owner) | Universal Bridge as helper |
| 3 iPad + S24+ on one BLE link | one link, per-device input | PENDING (owner) | phone input over LAN |
| 4 Return-edge feel | seamless switch back | PENDING (owner) | hotkey-only return |
| 5 iroh transfer vs LocalSend | fast files on all three | 1 GiB loopback ✔ 64 MB/s; Wi-Fi PENDING (owner) | tune QUIC buffers |
| 6 Shared core in Kotlin + Swift | three-app structure, no-Mac iOS | APK builds ✔, IPA builds on CI ✔; on-device PENDING (owner) | — |
| 7 Phone → PC clipboard tile | Android clipboard workaround | PENDING (owner) | share-sheet / Phone Link |

Fill in as each check runs on the real devices. **PENDING (owner)** = needs you with the device.

| | Value |
|---|---|
| iPadOS version | PENDING (owner) |
| One UI / Android version | PENDING (owner) |
| Windows build | 10.0.26300 |

## Spike 1 — iPad clipboard via `idevice` (Task 3)

Setup: Apple Devices app (installed), iPad on USB, "Trust this computer", Developer Mode on
(Settings → Privacy & Security → Developer Mode; reboot when asked).

Reference CLI: `idevice-tools` (built from source in `C:\dev\_ref\idevice\tools`).
If it can't see the iPad: `set USBMUXD_SOCKET_ADDRESS=127.0.0.1:27015`.

| Check | Command | Result |
|---|---|---|
| Mount developer disk image | `idevice-tools mounter -h` → mount | PENDING (owner) |
| iPad → PC text | copy in Notes, `idevice-tools pasteboard paste` | PENDING (owner) |
| PC → iPad, emoji/non-Latin | `idevice-tools pasteboard copy "héllo 👋 مرحبا"`, paste in Notes | PENDING (owner) |
| Change notifications | `idevice-tools pasteboard watch`, copy 3× on iPad | PENDING (owner) |
| Image | copy screenshot, `pasteboard items`, `pasteboard resolve <i> public.png --out shot.png` | PENDING (owner) |
| After 5 min locked | repeat `paste` | PENDING (owner) |
| Two-way sync spike | `cargo run -p ipad-clip`, 10× each way incl. emoji | PENDING (owner) |
| Over Wi-Fi | `pair`, then `--host <ip> --pairing-file <file>` (file stays out of git) | PENDING (owner) |

Verdict: PENDING

## Spikes 2–4 — BLE HID from Windows, two hosts, edge switching (Task 4)

Binary: `cargo run -p ble-hid -- <mode>`. **Quit Universal Bridge first** (only one app can publish the HID service).

| Check | How | Result |
|---|---|---|
| Rust publishes HID service + advertises | `ble-hid check` | **PASS** 2026-11-03 after fix: MT7921 reports `Aborted` first, then `Started`; tool now waits 12 s like UB |
| Clean shutdown | stop with Ctrl+C / `check` exits by itself | Required: force-killing a publisher wedged the MT7921 Bluetooth function once (fixed by Shift+Shutdown + 20 s power-button drain) |
| iPad pairs, square + `hello` | iPad Settings → Bluetooth → this PC; `ble-hid demo` | PENDING (owner) |
| Click, scroll | `ble-hid demo --clicks` | PENDING (owner) |
| S24+ pairs too; only target moves | `ble-hid hosts`, then `demo --target <id>` per id | PENDING (owner) |
| Both connected 10 min | leave `hosts` running | PENDING (owner) |
| Edge: 20 crossings each way | `ble-hid edge --target <id> --remote-at right --width 1180` | PENDING (owner) |
| No stuck keys across switch | hold Ctrl/Shift while switching | PENDING (owner) |
| Disconnect while Shift held | turn off iPad Bluetooth mid-press | PENDING (owner) |
| Tuned values | `--gain` / `--threshold` that felt right | PENDING (owner) |

Escape hatches always active in `edge`: Ctrl+Alt+Q, Esc ×3 in 1 s.

Verdict: PENDING

## Spike 5 — file transfer over iroh (Tasks 5–7)

Binaries: `bridge-recv <dir>` (prints ticket), `bridge-send <ticket> <file>`.

| Check | Result |
|---|---|
| Unit/integration tests (round trip, hostile name, cut-off leaves no file) | **PASS** 2026-11-03 |
| Windows → Windows loopback, 1 GiB, release build | **PASS**: 64.3 MB/s, bytes identical (protocol ceiling on this laptop, not Wi-Fi) |
| Windows ↔ S24+ (5 MB PDF, 1 GB video) vs LocalSend | PENDING (owner, Task 6) |
| Windows ↔ iPad (same files) vs LocalSend | PENDING (owner, Task 7) |

### Android spike app (Task 6)

Build (from repo root, PowerShell):
```
powershell -File spikes/android-app/build-rust.ps1
cd spikes/android-app
$env:JAVA_HOME="C:\Program Files\Android\Android Studio\jbr"; $env:TMP="C:\dev\_ref\tmp"; $env:TEMP=$env:TMP   # short TMP: JDK socket path limit
.\gradlew :app:assembleDebug
```
APK: `spikes/android-app/app/build/outputs/apk/debug/app-debug.apk` — **builds 2026-11-03**.
Install: plug in the S24+ with USB debugging on, `adb install -r <apk>` (adb is in `%LOCALAPPDATA%\Android\Sdk\platform-tools`).

| Check | How | Result |
|---|---|---|
| Phone receives 5 MB PDF / 1 GB video | app → Start receiving → Copy my ticket (arrives on PC via Phone Link) → `bridge-send "<ticket>" file` | PENDING (owner) |
| Phone sends both | `bridge-recv C:\tmp\in` → paste ticket in app → Pick file and send | PENDING (owner) |
| Same files with LocalSend, same Wi-Fi | time it | PENDING (owner) |
| Clipboard tile (emoji text) | add "Send clipboard" tile, copy text, tap tile → `clip.txt` on PC matches | PENDING (owner) |
| Tile flow acceptable? | owner's call | PENDING (owner) |

### iPad spike app (Task 7)

Built by `.github/workflows/ios-spike.yml` on a GitHub macOS runner (unsigned). Get it:
`gh run download --name Spike-ipa` (artifact kept 3 days; re-run the workflow for a new one).

| Check | How | Result |
|---|---|---|
| CI builds unsigned `Spike.ipa` | push to `core/**` or `spikes/ios-app/**` | **PASS** 2026-11-03 (run 41549281512); copy in `spikes/ios-app/dist/` |
| Sideload | SideStore or AltStore on Windows, free Apple account, install `Spike.ipa` | PENDING (owner) |
| iPad receives PDF/video | app → Start receiving → Copy my ticket → `bridge-send` | PENDING (owner) |
| PDF opens from Files → On My iPad → CTP Spike | | PENDING (owner) |
| iPad sends both | `bridge-recv` + paste ticket | PENDING (owner) |
| Same files with LocalSend | time it | PENDING (owner) |
