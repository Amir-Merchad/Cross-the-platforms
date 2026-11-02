# Spike results

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
| Rust publishes HID service + advertises | `ble-hid hosts` | **PASS** 2026-11-03: service 0x1812 created, advertising, no errors |
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
