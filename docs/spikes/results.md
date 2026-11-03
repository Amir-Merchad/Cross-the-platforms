# Spike results

## Summary (Task 8 — fill in after the device checks)

| Spike | What it proves | Status | Path if it fails |
|---|---|---|---|
| 1 iPad clipboard via `idevice` | no-app clipboard Windows ↔ iPad | **PASS over USB** (text, instant); image/lock/Wi-Fi PENDING | clipboard via iPad app only |
| 2 Rust BLE HID → iPad | own Bluetooth keyboard/mouse | **PASS** pairing, move, click, scroll, typing | Universal Bridge as helper |
| 3 iPad + S24+ on one BLE link | one link, per-device input | **PASS**: one Windows peripheral serves both, per-device targeting works | phone input over LAN |
| 4 Return-edge feel | seamless switch back | **PASS**: deliberate-push guard, no stuck keys, instant return on disconnect | hotkey-only return |
| 5 iroh transfer vs LocalSend | fast files on all three | 1 GiB loopback ✔ 64 MB/s; Wi-Fi PENDING (owner) | tune QUIC buffers |
| 6 Shared core in Kotlin + Swift | three-app structure, no-Mac iOS | APK builds ✔, IPA builds on CI ✔; on-device PENDING (owner) | — |
| 7 Phone → PC clipboard tile | Android clipboard workaround | PENDING (owner) | share-sheet / Phone Link |

Fill in as each check runs on the real devices. **PENDING (owner)** = needs you with the device.

| | Value |
|---|---|
| iPadOS version | iPadOS 27.2 (iPad Air 13-inch, M2) |
| One UI / Android version | One UI 9, Android 17 (Galaxy S24+) |
| Windows build | 10.0.26300 |

## Spike 1 — iPad clipboard via `idevice` (Task 3)

Setup: Apple Devices app (installed), iPad on USB, "Trust this computer", Developer Mode on
(Settings → Privacy & Security → Developer Mode; reboot when asked).

Reference CLI: `idevice-tools` (built from source in `C:\dev\_ref\idevice\tools`).
If it can't see the iPad: `set USBMUXD_SOCKET_ADDRESS=127.0.0.1:27015`.

| Check | Command | Result |
|---|---|---|
| Mount developer disk image | Developer Mode on (`amfi show` reveals the switch), DDI from `doronz88/DeveloperDiskImage` (Xcode files, 16.5 MB), `mounter mount --image … --manifest … --trustcache …` | **PASS** 2026-11-03: personalized via Apple TSS. Without DDI the pasteboard service is `ServiceNotFound` |
| iPad → PC text | `idevice-tools pasteboard paste` | **PASS** (plain text + RTF items present) |
| PC → iPad, emoji/non-Latin | `pasteboard copy "héllo 👋 مرحبا from Windows"` | **PASS**, identical in Notes |
| Change notifications | `idevice-tools pasteboard watch`, copy 3× on iPad | PENDING (owner) |
| Image | copy screenshot, `pasteboard items --policy promised`, `pasteboard resolve 0 public.png --out shot.png` | **PASS** 2026-11-03: valid 2732×2048 PNG (544 KB); HEIC/JPEG also offered (items are *promised*, resolved on demand) |
| After 5 min locked | repeat `paste` | PENDING (owner) |
| Two-way sync spike | `cargo run --release -p ipad-clip` | **PASS** 2026-11-03: both directions, "instant" |
| Over Wi-Fi | `pair`, then `--host <ip> --pairing-file <file>` (file stays out of git) | PENDING (owner) |

Verdict: PENDING

## Spikes 2–4 — BLE HID from Windows, two hosts, edge switching (Task 4)

Binary: `cargo run -p ble-hid -- <mode>`. **Quit Universal Bridge first** (only one app can publish the HID service).

| Check | How | Result |
|---|---|---|
| Rust publishes HID service + advertises | `ble-hid check` | **PASS** 2026-11-03 after fix: MT7921 reports `Aborted` first, then `Started`; tool now waits 12 s like UB |
| Clean shutdown | stop with Ctrl+C / `check` exits by itself | Required: force-killing a publisher wedged the MT7921 Bluetooth function once (fixed by Shift+Shutdown + 20 s power-button drain) |
| iPad pairs, square + `hello` | iPad Settings → Bluetooth → this PC; `ble-hid demo` | **PASS** 2026-11-03: paired, pointer square + typing work |
| Click, scroll | `ble-hid demo --clicks` | **PASS** 2026-11-03 |
| S24+ pairs too; only target moves | `ble-hid hosts`, then `demo --target <id>` per id | **PASS** 2026-11-03: iPad `…5b:95:30:4d:1c:f5`, S24+ `…4e:5d:31:c1:13:6e`; each moved alone |
| Both connected 10 min | leave `hosts` running | **PASS** 2026-11-03 |
| Edge crossings + both escapes | `ble-hid edge --target <id> --remote-at right --width 1366 --fast` | **PASS** 2026-11-03: entry, push-return, Ctrl+Alt+Q, Esc ×3 |
| Latency | notify avg 3 ms, max ~29 ms; iPad interval 15 ms (fast request accepted, iPad keeps 15 ms) | **PASS**: "smoother and more responsive" after pacing; remaining feel = iPad 60 Hz + 15 ms slot + iPad pointer animations |
| No accidental returns | deliberate-push guard (1 s grace, 300 ms dwell, 96 counts) | **PASS** 2026-11-03 |
| No stuck keys across switch | hold Shift while crossing either way | **PASS** 2026-11-03 (5a, 5b) |
| Disconnect while Shift held | turn off iPad Bluetooth mid-press | **PASS** 2026-11-03 after fix (watch connection status): instant return, no stuck Shift |
| Tuned values | defaults `--gain 1.0 --min-push 96 --dwell 300` felt right | **PASS** |

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
