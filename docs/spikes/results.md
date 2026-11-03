# Spike results

## Summary (Task 8 — fill in after the device checks)

| Spike | What it proves | Status | Path if it fails |
|---|---|---|---|
| 1 iPad clipboard via `idevice` | no-app clipboard Windows ↔ iPad | **PASS**: USB and Wi-Fi, text instant, images readable, auto-reconnect. Needs Developer Mode + DDI mount (redo after iPad reboot) | clipboard via iPad app only |
| 2 Rust BLE HID → iPad | own Bluetooth keyboard/mouse | **PASS** pairing, move, click, scroll, typing | Universal Bridge as helper |
| 3 iPad + S24+ on one BLE link | one link, per-device input | **PASS**: one Windows peripheral serves both, per-device targeting works | phone input over LAN |
| 4 Return-edge feel | seamless switch back | **PASS**: deliberate-push guard, no stuck keys, instant return on disconnect | hotkey-only return |
| 5 iroh transfer vs LocalSend | fast files on all three | **PARTIAL**: works both ways phone↔PC (~40–44 MB/s, hash-verified) but LocalSend is ~2× faster (90 MB/s); window tuning doesn't help | LAN bulk over TCP+TLS, iroh for pairing/internet |
| 6 Shared core in Kotlin + Swift | three-app structure, no-Mac iOS | APK builds ✔, IPA builds on CI ✔; on-device PENDING (owner) | — |
| 7 Phone → PC clipboard tile | Android clipboard workaround | **PASS** (one tap via Quick Settings tile) | share-sheet / Phone Link |

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
| After lock | lock iPad with sync running | **Finding** 2026-11-03: iPad closes the pasteboard connection on lock; a fresh connection works immediately and the DDI stays mounted (only a reboot unmounts). `ipad-clip` now reconnects every 2 s; retest PENDING |
| Two-way sync spike | `cargo run --release -p ipad-clip` | **PASS** 2026-11-03: both directions, "instant" |
| Over Wi-Fi | one-time `ipad-clip --rp-pair <file>` over USB (RemotePairing, no tap needed), then `ipad-clip --wifi <file>` | **PASS** 2026-11-03: mDNS finds `_remotepairing._tcp` (192.168.1.3:49152), pair-verify → TLS-PSK tunnel → pasteboard; two-way sync works. Plain lockdown-over-Wi-Fi refuses CoreDeviceProxy; Apple's Windows usbmux doesn't list Wi-Fi devices |

Verdict: **GO** for no-app iPad clipboard over Wi-Fi. Costs: Developer Mode on, DDI re-mount after each iPad reboot (automate in v1), one-time USB pairing.

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
| Phone receives | app → Start receiving | **PASS** 2026-11-04: PC→phone 200 MB in 4.72 s = **44.4 MB/s**, hash verified; files land in public `Download/Bridge` |
| Phone sends to PC | `bridge-recv`, ticket injected via `adb run-as` into the app prefs | **PASS** 2026-11-03: 198 MB video in 5.0 s = **39.7 MB/s** over Wi-Fi, bytes intact |
| Same files with LocalSend, same Wi-Fi | 198.5 MB video phone→PC | **LocalSend 90.0 MB/s vs ours 39.7 MB/s (PC→phone 44.4)**. Loopback ours ≈ 42–64 MB/s regardless of QUIC windows (1–64 MB tried) → per-packet QUIC/UDP cost on Windows, not flow control. v1: keep iroh for discovery/pairing/internet, move LAN bulk data to a direct TLS-over-TCP stream |
| Clipboard tile | Quick Settings "Send clipboard" tile → `clip.txt` on PC | **PASS** 2026-11-04: text arrived byte-identical (incl. non-ASCII `’`); "Clipboard sent" toast. Spike saves a file; v1 writes it to the Windows clipboard |
| Tile flow acceptable? | owner's call | PENDING (owner) |

### iPad spike app (Task 7)

Built by `.github/workflows/ios-spike.yml` on a GitHub macOS runner (unsigned). Get it:
`gh run download --name Spike-ipa` (artifact kept 3 days; re-run the workflow for a new one).

| Check | How | Result |
|---|---|---|
| CI builds unsigned `Spike.ipa` | push to `core/**` or `spikes/ios-app/**` | **PASS** 2026-11-03 (run 41549281512); copy in `spikes/ios-app/dist/` |
| Sideload | iloader v2.4.1 (Windows, uses Apple Devices' usbmux) → SideStore **0.7.0** (0.6.4 and older: sign-in broken) → iloader "Import IPA" | **PASS** 2026-11-04. AltStore PAL (EU) doesn't help: needs Apple-notarized apps (paid account) |
| iPad receives | ticket read from the iPad clipboard via `idevice` | **PASS** 2026-11-04: PC→iPad 200 MB in 2.64 s = **79.3 MB/s**, hash verified |
| PDF opens from Files → On My iPad → CTP Spike | | PENDING (owner) |
| iPad sends | ticket put on the iPad clipboard via `idevice` | **PASS** 2026-11-04: 46.5 MB video arrived intact (iPad-side speed: see note) |
| Same files with LocalSend | time it | PENDING (owner) |
