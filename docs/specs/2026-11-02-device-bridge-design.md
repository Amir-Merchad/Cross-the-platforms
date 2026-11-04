# Device Bridge — Design

Date: 2026-11-02 · Status: draft for approval (rev 2) · Working name "Bridge" / repo `device-bridge` (placeholder)

## 1. Intent

The owner's own open-source app suite that makes a Windows 11 laptop, a Samsung Galaxy S24+ and an iPad Air M2 work as one setup:

- any device's input can drive any other device (laptop keyboard/mouse first)
- copy on any device, paste on any other, without opening anything
- send files (mostly uni PDFs, screenshots, text; sometimes videos) quickly between all three
- share (mirror) or extend screens between devices where the platform allows it

**Why:** today this takes several apps (LocalSend, Universal Bridge for Bluetooth input, manual clipboard tricks). The owner wants it all in one place, as seamless as possible. Open source on GitHub; licence is not a concern (GPL code may be used).

**Success:** after first setup the owner never opens another tool for these jobs, and nothing needs starting by hand after sleep or reboot.

## 2. Versions

| Version | Scope |
|---|---|
| **v1** | Windows keyboard/mouse → iPad and S24+ with edge switching; clipboard sync across all three; LAN file transfer; all three apps (§4) |
| **v1.5** | Cross-platform input (phone/iPad controlling each other and Windows, §5); screen mirroring and Windows screen extension (§6) |
| **v2** | Internet transfer and remote use, built on the same pairing/protocol |

v1.5 placement is Claude's call; owner may reorder. v2 starts only when v1 works day to day.

## 3. Goals, non-goals, anti-goals

**Goals (v1)**
- Shared keyboard/mouse from Windows to iPad and S24+, edge switching both ways, hotkey fallback.
- Automatic clipboard sync across all three: text and images first.
- File transfer across all three, optimised for small files arriving instantly, still fast for videos.
- Auto-reconnect after sleep, reboot or Wi-Fi changes.

**Non-goals**
- Any extra hardware (no ESP32, dongles, Picos). Only laptop, phone, iPad.
- A Mac or paid Apple developer account.
- Extending the iPad's or phone's own desktop onto another screen (platforms don't allow it, §6).

**Anti-goals (failure even if it "works")**
- Windows ever left without a working mouse/keyboard: stuck keys, input swallowed after a disconnect.
- Needing to start or re-pair things by hand after every reboot.
- Being slower or clunkier than LocalSend for a single PDF.
- Logging clipboard contents or file names by default.
- The three apps drifting apart: a device must never need a specific app flavour to talk to another.

## 4. Apps and architecture (owner's decision, structure is Claude's call)

One app per platform (changed 2026-11-04; it was two apps per mobile device). The Tauri mobile flavours are dropped: native apps are needed anyway for background services and share extensions, so a second UI per device only doubled the work and the testing.

1. **Tauri + shadcn/ui app** — Windows only. It is the hub.
2. **Native Kotlin app** for Android.
3. **Native Swift app** for iPad.

To keep this buildable, **all shared logic lives once in a Rust core** that every app uses:

```text
core/ (Rust)  — protocol, pairing, device keys, iroh transport, transfer,
                clipboard engine, LocalSend adapter, input event model
   ├─ used directly by  apps/tauri   (Windows; shadcn UI)
   ├─ via UniFFI   →    apps/android (Kotlin, native UI)
   └─ via UniFFI   →    apps/ios     (Swift, native UI)
platform/     — per-OS pieces that can't be shared:
   windows: Raw Input, hooks, BLE HID peripheral, SendInput, clipboard, (v1.5) capture + virtual display
   android: foreground service, BluetoothHidDevice, share target, clipboard tile, (v1.5) MediaProjection
   ios:     share extension, Files integration, (v1.5) ReplayKit broadcast extension
```

- Any flavour talks to any other: one versioned wire protocol in `core/`.
- **Build order (Claude's call):** Windows Tauri app first (it's the hub), then native Kotlin and Swift (background services and share extensions are native anyway), the Tauri mobile flavours were planned last and then dropped.

### No-Mac iPad builds
- The Swift iPad app builds on a **GitHub-hosted macOS runner**, producing an unsigned `.ipa`; install from Windows with **iloader → SideStore 0.7.0+** (older SideStore sign-in is broken since Apple's 2026 change) and a free Apple account (7-day re-signing, 3 sideloaded apps max). Plain SideStore, not LiveContainer, so share extensions and Files integration work. AltStore PAL (EU) doesn't apply: it needs Apple-notarized apps. *(Updated 2026-11-04 from spikes.)*
- Swift UI work can't be previewed on Windows; each UI check is a CI build + sideload. Keep iOS UI simple and push logic into `core/`, which is testable on Windows.
- **Make the repo public:** standard GitHub runners are free for public repos; private repos burn included minutes and macOS minutes count at a high multiplier.

## 5. Input

### 5.1 v1: Windows keyboard/mouse → iPad and S24+
- **Transport: BLE HID from Windows** (GattServiceProvider via the `windows` crate). Proven on this laptop + iPad by Universal Bridge (MIT, C#); port its HID descriptor, keymap and GATT setup to Rust.
- **Capture:** Raw Input for mouse deltas (no Windows acceleration), low-level hooks to suppress local input while a remote device is active.
- **Switching:** layout editor (which device sits on which edge of which monitor); edge crossing with hold time; hotkeys per device + "back to Windows"; emergency escape (e.g. Esc ×3).
- **Return edge** (iPad/Android pointer is relative-only and unreadable): on entry, push the pointer hard against the Windows-side edge to pin it; dead-reckon from sent deltas, clamped at that edge. Reaching the estimated edge is only a *candidate*: return needs a deliberate push (1 s entry grace, ≥300 ms dwell, ≥96 counts, ≤180 ms gaps, first flick ignored — Universal Bridge's guard). Motion is paced at the BLE connection interval (iPad: 15 ms minimum). Disconnect is detected from the device's connection status, not its GATT subscription. Calibration settings exposed. *(Updated 2026-11-04 from spikes.)*
- **S24+:** the same BLE link serves the iPad and the S24+ at once, sending reports only to the active host (**verified 2026-11-03**); the LAN-injection fallback is not needed.
- **Safety:** on switch, disconnect, crash or sleep, release all held keys/buttons and return control to Windows.
- **Keyboard layout:** HID sends key positions; iPad/phone hardware-keyboard layout must match; optional Ctrl↔Cmd remap.

### 5.2 v1.5: cross-platform input (feasibility as researched)

| From → To | Route | Status |
|---|---|---|
| Phone → iPad | Android `BluetoothHidDevice` (phone as BT keyboard/mouse), touchpad + keyboard UI in phone app | Keyboard reported working; mouse on iPad unverified — test |
| Phone → Windows | Same BT HID, or LAN → Windows `SendInput` | Feasible |
| iPad → Windows | iPad app as touchpad/keyboard over LAN → `SendInput` | Feasible, **only while iPad app is open**; iPad can't be a BT HID device (Apple blocks it) |
| iPad → phone | LAN → phone app injects (Shizuku) | Feasible with Shizuku |
| KBM plugged into iPad/phone → others | App captures it only while in foreground (GameController keyboard/mouse on iPad) | Partial; no system-wide edge crossing from iPad |

Seamless edge crossing *starting* on the iPad isn't possible: no app can see the iPad pointer outside itself. Starting on the phone needs Shizuku-level access; treat as stretch.

## 6. v1.5: screens

| Action | Route | Status |
|---|---|---|
| **Extend Windows** onto iPad or phone | Open-source IddCx virtual display driver (e.g. itsmikethetech/Virtual-Display-Driver) + Windows Graphics Capture + hardware H.264/HEVC encode → stream → decode in app | Feasible (same approach as existing second-screen tools); needs a driver install once |
| Mirror Windows → iPad/phone | Graphics Capture + same stream | Feasible |
| Mirror phone → Windows/iPad | MediaProjection (scrcpy-style), control back via injection | Feasible |
| Mirror iPad → Windows/phone | ReplayKit broadcast extension in iPad app, or CoreDevice display stream via `idevice` (no app) | Feasible; display stream also unlocks `idevice` touch injection for controlling the mirror |
| Extend iPad or phone desktop onto another device | — | **Not possible** on these platforms; mirror only |

Receivers: all three apps can show a stream full-screen; the Windows receiver is a window you can click into (clicks become input on the source).

## 7. Clipboard (v1)
- **Windows ↔ iPad, no app needed (verified 2026-11-03):** CoreDevice pasteboard service via `idevice` (MIT): read, write, change notifications, text instant, images resolvable. Needs Developer Mode and the developer disk image (personalized via Apple TSS; **re-mount automatically after every iPad reboot**). Wi-Fi works through **RemotePairing**: one-time pairing over USB, then mDNS `_remotepairing._tcp` → pair-verify → TLS-PSK tunnel → RSD. Reconnect automatically when the link drops.
- **iPad fallback:** clipboard action in the iPad app (both flavours).
- **Windows ↔ phone:** via the phone app. Android 10+ blocks background clipboard reads: Windows→phone is easy; phone→Windows uses a quick-settings tile / notification action / share target, or Shizuku. Phone Link stays as owner fallback.
- **Engine rules (core/):** origin device + ID on every item (no bouncing), last writer wins, pause toggle, image size limit, nothing logged.
- Formats: text and images first; rich text later; files on the clipboard become a file transfer.

## 8. File transfer (v1)
- **Main path:** own protocol on **iroh** (QUIC, key-addressed, encrypted) for discovery, pairing, control and (v2) internet. **On the same LAN, bulk file data moves to a direct TLS-over-TCP stream** negotiated over the iroh connection: spikes measured iroh at ~40–44 MB/s phone↔PC and 16 MB/s iPad→PC vs LocalSend's 90 MB/s, limited by per-packet QUIC/UDP cost on Windows, not flow control. No LAN-only assumptions in auth. *(Updated 2026-11-04 from spikes.)*
- **Pairing:** once per device, QR code on Windows scanned by the mobile app; device keys stored.
- **Fast path:** drag-drop onto a device tile, Explorer "Send to", mobile share sheet; optional auto-send of new screenshots to the active device. Files land in a predictable folder (iPad Files → Bridge, phone Downloads/Bridge, configurable on Windows).
- **Fallbacks:** LocalSend protocol (iPad/phone without our app); Quick Share via an rquickshare port (later, laptop appears in the phone's native share sheet); AirDrop phone ↔ iPad (Samsung interop, nothing to build).

## 9. Known platform limits
- **iPad apps can't run in the background:** files over our protocol arrive only while the app is open; Windows queues sends and the app pulls on launch. Clipboard via `idevice` doesn't need the app.
- **Android needs a foreground service** (persistent notification) and battery-optimisation exemption.
- **Shizuku** (phone input injection fallback) needs wireless debugging restarted after a phone reboot.
- **Two iPad flavours** use 2 of the 3 free sideload slots and both need weekly re-signing.

## 10. Validation order (throwaway spikes before product work)

**Done 2026-11-04 — results and verdicts in `docs/spikes/results.md`.**
Record iPadOS and One UI versions first.
1. iPad clipboard via `idevice` (USB, then Wi-Fi; text + image; survives sleep/reboot). *Fail →* clipboard via iPad app only.
2. Rust BLE HID → iPad (move, click, scroll, type). *Fail →* reuse Universal Bridge as external helper.
3. Same BLE link → iPad + S24+ together, per-host reports. *Fail →* phone input over LAN.
4. Return-edge feel with pin + clamp. *Fail →* hotkey-only return.
5. iroh transfer Windows ↔ Android and ↔ iPad (CI-built, sideloaded): 5 MB PDF and 1 GB video vs LocalSend.
6. Rust core via UniFFI in a minimal Kotlin and Swift app (proves the shared-core structure and the no-Mac iOS pipeline).
7. Phone → Windows clipboard workaround.

## 11. Left to the builder
- Monorepo layout (suggested: `core/`, `platform/`, `apps/tauri`, `apps/android`, `apps/ios`, `docs/`), crate choices beyond `windows`, `idevice`, `iroh`, UniFFI, Tauri.
- Wire format (versioned).
- Exact hotkeys, thresholds, folders, size limits, codec settings.
- Diagnostics format (never contents).

## 12. Calls Claude made (owner to check)
- Shared Rust core used by all three apps (Tauri directly, Kotlin/Swift via UniFFI).
- Build order: Windows Tauri → native Kotlin/Swift → Tauri mobile.
- Public GitHub repo (free macOS CI minutes).
- v1.5 holds cross-platform input and screens; v2 internet.
- Port Universal Bridge's HID approach to Rust; `idevice` over pymobiledevice3.
- Auto-send screenshots and "Bridge" folders as defaults.
