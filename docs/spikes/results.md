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
