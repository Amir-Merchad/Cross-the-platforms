# Foundation and Validation Spikes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Rust workspace and shared core, then run the spec's validation spikes on the owner's real devices so the v1 plans are built on measured facts.

**Architecture:** A Cargo workspace with one kept crate, `core/` (pure, unit-tested logic: HID reports, return-edge tracking, file transfer over iroh, UniFFI exports), and throwaway spike binaries/apps under `spikes/`. Spikes that need hardware end in a manual check by the owner, recorded in `docs/spikes/results.md`. The v1 plans (input, clipboard, transfer, apps) are written after this plan, from those results.

**Tech Stack:** Rust 1.97 (stable, `x86_64-pc-windows-msvc`), `windows` 0.62, `idevice` 0.1.68 (+ its `tools` CLI), `iroh` 1.4, `uniffi` 0.32, `tokio`, `arboard`; Android: Kotlin + Jetpack Compose, Gradle with Android Studio's JDK 21, NDK 27.0.12077973, `cargo-ndk`; iOS: Swift/SwiftUI, XcodeGen, GitHub Actions `macos-latest`.

**Spec:** `docs/specs/2026-11-02-device-bridge-design.md` (rev 2, approved)

## Global Constraints

- No extra hardware: only the Windows 11 laptop, Galaxy S24+, iPad Air M2.
- No Mac, no paid Apple account: every iOS build runs on a GitHub-hosted macOS runner, produces an unsigned `.ipa`, and is sideloaded from Windows (SideStore/AltStore).
- Repo is public (`Amir-Merchad/Cross-the-platforms`); commit no pairing files, Apple credentials, device UDIDs or keys. Add them to `.gitignore`.
- All shared logic goes in `core/`; Kotlin and Swift call it through UniFFI, never re-implement it.
- Never log clipboard contents or file names (log sizes and counts only).
- Windows must never be left without working input: every spike that suppresses local input has `Ctrl+Alt+Q` and `Esc` pressed 3× within 1 s as instant "back to Windows", and releases suppression on exit, panic or remote disconnect.
- Android: `minSdk 31`, `targetSdk 36`, package `dev.crosstheplatforms.spike`.
- iOS: bundle id `dev.crosstheplatforms.spike`, iOS deployment target 17.0.
- Spike code under `spikes/` is throwaway: no tests beyond what's listed, no polish. Only `core/` is kept.

## Review Focus

1. **Received file name is hostile or odd** (`../../evil.txt`, `C:\Windows\x`, `con`, empty, a name that already exists) → saved inside the receive dir under its sanitized base name, duplicates become `name (1).ext`; never written outside the dir. Test in Task 5.
2. **Transfer cut off mid-file** (sender killed, Wi-Fi drop) → no complete-looking file left behind; data goes to `name.part` and is renamed only after the hash matches. Test in Task 5.
3. **Remote device disconnects while a key or button is held** → an all-zero report is sent where possible and local input is restored at once; no stuck Ctrl/Shift on either side. Test in Task 1 (`HeldInput::release_all`) and checked manually in Task 4.
4. **Fast mouse flick** (one Raw Input delta > 127 or < −128) → split into several reports that sum exactly to the delta, no i8 wrap-around. Test in Task 1.
5. **Clipboard text with emoji/non-Latin characters, or empty clipboard** → arrives byte-identical on the other side; an empty clipboard sends nothing. Checked manually in Task 3.

---

### Task 1: Workspace, CI and HID report encoding

**Files:**
- Create: `Cargo.toml` (workspace: members `core`, `spikes/*` that are Rust crates), `.gitignore` (`target/`, `*.plist`, `pairing*`, `local.properties`, `build/`, `.gradle/`, `*.ipa`), `rust-toolchain.toml` (`channel = "stable"`)
- Create: `core/Cargo.toml` (crate name `bridge-core`, lib types `["lib", "cdylib", "staticlib"]`), `core/src/lib.rs`, `core/src/hid.rs`
- Create: `.github/workflows/core.yml` (on push/PR: `windows-latest`, `cargo test -p bridge-core`, `cargo clippy -p bridge-core -- -D warnings`)
- Test: `core/src/hid.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub const REPORT_MAP: &[u8]` — HID report descriptor with Report ID 1 = boot-style keyboard (modifier byte, reserved, 6 keys) and Report ID 2 = mouse (5 buttons, X/Y relative i8, wheel i8). Take the bytes from Universal Bridge's descriptor (MIT; grep its `src/` for the report map) and credit it in `core/THIRD_PARTY_NOTICES.md`.
  - `pub struct MouseReport { pub buttons: u8, pub dx: i8, pub dy: i8, pub wheel: i8 }` with `pub fn to_bytes(&self) -> [u8; 4]`
  - `pub struct KeyboardReport { pub modifiers: u8, pub keys: [u8; 6] }` with `pub fn to_bytes(&self) -> [u8; 8]` (byte 1 reserved = 0)
  - `pub fn split_delta(dx: i32, dy: i32) -> Vec<(i8, i8)>`
  - `pub fn vk_to_hid(vk: u16) -> Option<u8>` (Windows virtual-key → HID usage; port Universal Bridge's keymap) and `pub fn vk_to_modifier(vk: u16) -> Option<u8>` (bit for LCtrl 0x01, LShift 0x02, LAlt 0x04, LGui 0x08, RCtrl 0x10, RShift 0x20, RAlt 0x40, RGui 0x80)
  - `pub struct HeldInput` with `pub fn key_down(&mut self, vk: u16) -> KeyboardReport`, `pub fn key_up(&mut self, vk: u16) -> KeyboardReport`, `pub fn button(&mut self, mask: u8, down: bool) -> u8` (returns current button byte), `pub fn release_all(&mut self) -> (KeyboardReport, MouseReport)`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn split_small_delta_is_one_report() { assert_eq!(split_delta(5, -3), vec![(5, -3)]); }
#[test] fn split_large_delta_sums_exactly() {
    let parts = split_delta(300, -260);
    assert!(parts.iter().all(|&(x, y)| (-127..=127).contains(&x) && (-127..=127).contains(&y)));
    assert_eq!(parts.iter().map(|p| p.0 as i32).sum::<i32>(), 300);
    assert_eq!(parts.iter().map(|p| p.1 as i32).sum::<i32>(), -260);
}
#[test] fn split_zero_is_empty() { assert!(split_delta(0, 0).is_empty()); }
#[test] fn mouse_report_bytes() {
    assert_eq!(MouseReport { buttons: 1, dx: -1, dy: 2, wheel: 0 }.to_bytes(), [1, 0xFF, 2, 0]);
}
#[test] fn letter_a_maps_to_usage_4() { assert_eq!(vk_to_hid(0x41), Some(0x04)); }
#[test] fn ctrl_is_modifier_not_key() { assert_eq!(vk_to_modifier(0xA2), Some(0x01)); assert_eq!(vk_to_hid(0xA2), None); }
#[test] fn held_ctrl_a_then_release_all_is_zero() {
    let mut h = HeldInput::default();
    h.key_down(0xA2); let r = h.key_down(0x41);
    assert_eq!(r.modifiers, 0x01); assert_eq!(r.keys[0], 0x04);
    h.button(0x01, true);
    let (k, m) = h.release_all();
    assert_eq!(k.to_bytes(), [0; 8]); assert_eq!(m.to_bytes(), [0; 4]);
}
#[test] fn seventh_key_is_ignored_not_panicking() {
    let mut h = HeldInput::default();
    for vk in 0x41..=0x47 { h.key_down(vk); }
    assert_eq!(h.key_down(0x48).keys.iter().filter(|&&k| k != 0).count(), 6);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p bridge-core`
Expected: compile errors (items not defined).

- [ ] **Step 3: Implement `hid.rs`**

`split_delta`: emit chunks of at most ±127 per axis until both remainders are 0; axes chunk independently within the same report.

- [ ] **Step 4: Run tests and clippy**

Run: `cargo test -p bridge-core && cargo clippy -p bridge-core -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml rust-toolchain.toml .gitignore core .github
git commit -m "feat(core): workspace, CI and HID report encoding"
```

---

### Task 2: Return-edge tracker

**Files:**
- Create: `core/src/edge.rs`, export from `core/src/lib.rs`
- Test: `core/src/edge.rs`

**Interfaces:**
- Produces:
  - `pub enum Side { Left, Right }` — which edge of the *remote* screen touches Windows
  - `pub struct EdgeTracker` with `pub fn new(side: Side, width: f64, gain: f64, threshold: f64) -> Self`
  - `pub fn pin_delta(&self) -> i32` — the push to send on entry: `-(width * 2)` toward the Windows side as a raw dx (negative for `Left`, positive for `Right`)
  - `pub fn enter(&mut self)` — sets estimate to 0 (pinned at the Windows-side edge), clears overflow
  - `pub fn on_move(&mut self, dx: i32) -> EdgeEvent` where `pub enum EdgeEvent { Stay, Return }`

Algorithm (spec §5.1 "pin + clamp"):
```text
d = dx * gain, sign-flipped so "toward Windows" is negative
if est == 0 and d < 0: overflow += -d   else: overflow = 0
est = clamp(est + d, 0, width)
return Return if overflow >= threshold else Stay
```

- [ ] **Step 1: Write the failing tests**

```rust
fn t() -> EdgeTracker { let mut e = EdgeTracker::new(Side::Left, 1000.0, 1.0, 50.0); e.enter(); e }
#[test] fn pin_delta_points_toward_windows() {
    assert_eq!(EdgeTracker::new(Side::Left, 1000.0, 1.0, 50.0).pin_delta(), -2000);
    assert_eq!(EdgeTracker::new(Side::Right, 1000.0, 1.0, 50.0).pin_delta(), 2000);
}
#[test] fn pushing_past_edge_returns() { let mut e = t(); assert_eq!(e.on_move(-30), EdgeEvent::Stay); assert_eq!(e.on_move(-30), EdgeEvent::Return); }
#[test] fn moving_away_resets_overflow() { let mut e = t(); e.on_move(-40); e.on_move(5); e.on_move(-5); assert_eq!(e.on_move(-40), EdgeEvent::Stay); }
#[test] fn must_travel_back_before_overflow_counts() { let mut e = t(); e.on_move(300); assert_eq!(e.on_move(-320), EdgeEvent::Stay); assert_eq!(e.on_move(-50), EdgeEvent::Return); }
#[test] fn far_edge_clamps() { let mut e = t(); e.on_move(5000); e.on_move(-1000); assert_eq!(e.on_move(-50), EdgeEvent::Return); }
#[test] fn right_side_mirrors() { let mut e = EdgeTracker::new(Side::Right, 1000.0, 1.0, 50.0); e.enter(); assert_eq!(e.on_move(60), EdgeEvent::Return); }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core edge` → compile errors.
- [ ] **Step 3: Implement `edge.rs`** per the algorithm above. `gain` and `threshold` stay parameters (calibration knobs).
- [ ] **Step 4: Run tests** — `cargo test -p bridge-core` → all pass.
- [ ] **Step 5: Commit** — `git commit -am "feat(core): return-edge tracker"` (add `core/src/edge.rs` first).

---

### Task 3: Spike 1 — iPad clipboard via `idevice`

**Files:**
- Create: `spikes/ipad-clip/Cargo.toml` (deps: `idevice` 0.1.68 with features needed by the tools' `coredevice_pasteboard` path — copy them from `idevice/tools/Cargo.toml`; `tokio` full; `arboard`), `spikes/ipad-clip/src/main.rs`
- Create: `docs/spikes/results.md` (one section per spike: date, iPadOS / One UI version, what was tried, pass/fail, numbers, decision)

**Interfaces:**
- Consumes: nothing from `core/`.
- Produces: a go/no-go for spec §7 "Windows ↔ iPad, no app needed".

- [ ] **Step 1: Prepare devices (owner)** — record iPadOS and One UI versions in `results.md`. Install Apple's "Apple Devices" app from the Microsoft Store (provides the USB mux service). Plug the iPad in via USB, trust the PC, enable Settings → Privacy & Security → Developer Mode (appears after the tool first connects; reboot when asked).
- [ ] **Step 2: Install the reference CLI** — `cargo install --git https://github.com/jkcoxson/idevice idevice-tools` (if the package name differs, use the name in `tools/Cargo.toml`). If it can't find the device, set `USBMUXD_SOCKET_ADDRESS=127.0.0.1:27015` and retry.
- [ ] **Step 3: Mount the developer disk image** using the tool's `mounter` subcommand (see its `-h`). Expected: success, or a clear error to record.
- [ ] **Step 4: Manual pasteboard checks (owner)** — run, in order, and record each result:
  - `idevice-rs-tools pasteboard paste` after copying text in iPad Notes → text printed
  - `idevice-rs-tools pasteboard copy "héllo 👋 مرحبا"` → paste in iPad Notes shows it identically (Review Focus 5)
  - `idevice-rs-tools pasteboard watch` while copying 3 times on iPad → 3 pushes
  - copy a screenshot on iPad, `pasteboard items` then `resolve <index> public.png --out shot.png` → valid PNG
  - lock the iPad 5 min, unlock, repeat `paste` → still works (or note reconnect needed)
- [ ] **Step 5: Write `spikes/ipad-clip`** — `main` connects like `tools/src/coredevice_pasteboard.rs` (`CoreDeviceProxy::connect` → `create_software_tunnel` → `RsdHandshake::new` → `PasteboardServiceClient::connect_rsd`), then runs two loops: iPad `recv_push` text → `arboard` set; Windows `arboard` text poll every 300 ms → `set_text` on the iPad. Keep a `last_synced: Option<String>` so an item set by one side isn't sent back; skip empty strings. Log only lengths.
- [ ] **Step 6: Run it (owner)** — `cargo run -p ipad-clip`; copy on each side, paste on the other, 10 times each way, including the Step 4 emoji string. Record latency feel and failures.
- [ ] **Step 7: Try over Wi-Fi** — unplug USB; use the tool's network option (`--host <iPad IP> --pairing-file <file>`; the pairing file comes from the tool's `pair` subcommand and must stay out of git). Record pass/fail.
- [ ] **Step 8: Commit** — `git add spikes/ipad-clip docs/spikes/results.md && git commit -m "spike: iPad clipboard via idevice"`

---

### Task 4: Spikes 2–4 — BLE HID from Windows, two hosts, edge switching

**Files:**
- Create: `spikes/ble-hid/Cargo.toml` (deps: `bridge-core` path, `windows` 0.62 with features `Devices_Bluetooth`, `Devices_Bluetooth_GenericAttributeProfile`, `Storage_Streams`, `Foundation`, `Foundation_Collections`, `Win32_UI_Input`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_UI_WindowsAndMessaging`, `Win32_Foundation`, `Win32_System_LibraryLoader`)
- Create: `spikes/ble-hid/src/peripheral.rs`, `spikes/ble-hid/src/capture.rs`, `spikes/ble-hid/src/main.rs`

**Interfaces:**
- Consumes: `bridge_core::hid::{REPORT_MAP, MouseReport, KeyboardReport, split_delta, HeldInput}`, `bridge_core::edge::{EdgeTracker, Side, EdgeEvent}`
- Produces (spike-internal):
  - `peripheral::HidPeripheral::start() -> windows::core::Result<HidPeripheral>` — publishes GATT HID service 0x1812 (HID Information 0x2A4A, Report Map 0x2A4B = `REPORT_MAP`, HID Control Point 0x2A4C, Protocol Mode 0x2A4E, Input Report 0x2A4D ×2 with Report Reference 0x2908 = `[1,1]` keyboard / `[2,1]` mouse), plus Battery 0x180F and Device Information 0x180A; reads/writes require encryption; advertises connectable + discoverable. Mirror Universal Bridge's GATT setup (grep its `src/` for `GattServiceProvider`) for every flag it sets.
  - `HidPeripheral::hosts(&self) -> Vec<String>` — device ids of subscribed clients
  - `HidPeripheral::send_mouse(&self, host: &str, r: MouseReport)` / `send_keyboard(&self, host: &str, r: KeyboardReport)` — `NotifyValueAsync(value, subscribedClient)` to that client only
  - `capture::run(on_event: impl FnMut(Input) -> Suppress)` — Raw Input mouse deltas/buttons/wheel + `WH_KEYBOARD_LL`/`WH_MOUSE_LL` hooks that swallow local input while `Suppress::Yes`

- [ ] **Step 1: Peripheral + `demo` mode** — `cargo run -p ble-hid -- demo`: start the peripheral, wait for a host, then every 5 s move the pointer in a 200 px square and type `hello`.
- [ ] **Step 2: Spike 2 check (owner)** — quit Universal Bridge first. Pair the iPad from iPad Settings → Bluetooth. Expected: pointer draws the square, `hello` appears in Notes. Then click/scroll/drag via a `demo --clicks` variant. Record in `results.md`; on failure record the exact `windows` error (fallback: run Universal Bridge as an external helper).
- [ ] **Step 3: Spike 3 check (owner)** — pair the S24+ as well. `demo --target <id>` with each id from `hosts()`. Expected: only the targeted device moves; then both stay connected for 10 min. Record result (fallback: phone input over LAN).
- [ ] **Step 4: `edge` mode** — `cargo run -p ble-hid -- edge --side right --width 1180 --gain 1.0 --threshold 60 --target <id>`: the remote device sits right of the primary monitor. Pointer at the right edge for 300 ms → suppress local input, send `pin_delta` via `split_delta`, forward deltas/buttons/wheel/keys; `EdgeEvent::Return`, `Ctrl+Alt+Q` or `Esc` ×3 in 1 s → `release_all()` to the host, stop suppressing, place the Windows cursor 20 px inside the right edge. Peripheral host disconnect or Ctrl+C → same release path. Install a panic hook that unhooks before exit.
- [ ] **Step 5: Spike 4 check (owner)** — on iPad and phone: 20 crossings each way, typing with Ctrl/Shift held across a switch (Review Focus 3), unplug/turn off Bluetooth on the iPad while holding Shift. Expected: no stuck keys, Windows always regains input. Tune `--gain`/`--threshold` and record the values that felt right.
- [ ] **Step 6: Commit** — `git add spikes/ble-hid docs/spikes/results.md && git commit -m "spike: BLE HID peripheral, multi-host and edge switching"`

---

### Task 5: Core file transfer over iroh

**Files:**
- Create: `core/src/transfer.rs`, `core/src/names.rs`; modify `core/Cargo.toml` (deps `iroh` 1.4, `tokio`, `sha2`, `serde`/`serde_json`, `anyhow`)
- Create: `spikes/transfer-cli/` (`bridge-recv <dir>` prints ticket; `bridge-send <ticket> <file>` prints MB/s)
- Test: `core/src/names.rs` unit tests, `core/tests/transfer.rs` integration test

**Interfaces:**
- Produces:
  - `pub fn safe_name(dir: &Path, wanted: &str) -> PathBuf` — base name only (strip any `/` `\` path parts and drive prefixes), replace `<>:"|?*` and control chars with `_`, prefix `_` for Windows reserved names (`con`, `nul`, `com1`…), empty → `file`, existing → `name (1).ext`, `(2)`…
  - `pub struct Receiver` with `pub async fn start(dir: PathBuf) -> anyhow::Result<Receiver>`, `pub fn ticket(&self) -> String`, `pub async fn next(&mut self) -> anyhow::Result<Received>`; `pub struct Received { pub path: PathBuf, pub size: u64 }`
  - `pub async fn send_file(ticket: &str, path: &Path) -> anyhow::Result<u64>` (bytes sent)
  - Wire: ALPN `b"ctp/transfer/1"`; one bi-stream per file; sender writes a JSON line `{"v":1,"name":..,"size":..}\n` then the bytes; receiver streams to `safe_name(..)` + `.part`, hashes with SHA-256, replies the hex hash + `\n`; sender compares with its own hash and errors on mismatch; receiver renames `.part` → final only after the stream ends with exactly `size` bytes, else deletes `.part`.
  - Ticket: the receiver's iroh endpoint address serialized as a string (use iroh's own ticket/addr serialization).

- [ ] **Step 1: Write the failing tests**

```rust
// core/src/names.rs
#[test] fn strips_traversal() { let d = tempdir(); assert_eq!(safe_name(d.path(), "../../evil.txt"), d.path().join("evil.txt")); }
#[test] fn strips_windows_path() { let d = tempdir(); assert_eq!(safe_name(d.path(), r"C:\Windows\x.pdf"), d.path().join("x.pdf")); }
#[test] fn reserved_name() { let d = tempdir(); assert_eq!(safe_name(d.path(), "con.txt"), d.path().join("_con.txt")); }
#[test] fn empty_name() { let d = tempdir(); assert_eq!(safe_name(d.path(), ""), d.path().join("file")); }
#[test] fn duplicate_gets_suffix() { let d = tempdir(); std::fs::write(d.path().join("a.pdf"), b"x").unwrap(); assert_eq!(safe_name(d.path(), "a.pdf"), d.path().join("a (1).pdf")); }

// core/tests/transfer.rs
#[tokio::test] async fn round_trip_identical_bytes() { /* 5 MiB random file; Receiver::start(tmp) ; send_file(ticket); next() → bytes equal, size == 5*1024*1024 */ }
#[tokio::test] async fn cut_off_transfer_leaves_no_file() { /* spawn send_file of 50 MiB, abort the task after first chunk; assert receive dir has no non-.part file and no .part after next() returns Err */ }
```
(`tempdir` from `tempfile` as dev-dependency.)

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core` → compile errors.
- [ ] **Step 3: Implement `names.rs` and `transfer.rs`** to the interfaces above; iroh endpoint uses its default (relay-capable) config so v2 needs no auth change, but this spike only targets LAN.
- [ ] **Step 4: Run tests** — `cargo test -p bridge-core` → all pass.
- [ ] **Step 5: Write `transfer-cli`** and run Windows → Windows (two terminals) with a 1 GB file; record MB/s in `results.md`.
- [ ] **Step 6: Commit** — `git add core spikes/transfer-cli docs/spikes/results.md && git commit -m "feat(core): file transfer over iroh"`

---

### Task 6: UniFFI exports + Android spike app

**Files:**
- Create: `core/src/ffi.rs` (UniFFI proc-macro exports), `core/uniffi.toml`, `uniffi-bindgen` binary at `core/src/bin/uniffi-bindgen.rs`
- Create: `spikes/android-app/` (Gradle project: `settings.gradle.kts`, `app/build.gradle.kts`, `app/src/main/AndroidManifest.xml`, `app/src/main/java/dev/crosstheplatforms/spike/{MainActivity.kt,ReceiveService.kt,ClipTile.kt}`), `spikes/android-app/build-rust.ps1`

**Interfaces:**
- Consumes: `Receiver`, `send_file` from Task 5
- Produces (UniFFI, used by Task 7 too):
  - `#[uniffi::export] async fn start_receiver(dir: String) -> Result<Arc<FfiReceiver>, FfiError>`; `FfiReceiver::ticket() -> String`; `FfiReceiver::next() -> Result<FfiReceived, FfiError>` (async); `FfiReceived { path: String, size: u64 }`
  - `#[uniffi::export] async fn send_file_ffi(ticket: String, path: String) -> Result<u64, FfiError>`
  - `#[derive(uniffi::Error)] enum FfiError { Failed { message: String } }`

- [ ] **Step 1: Exports compile** — add `ffi.rs`; `cargo build -p bridge-core` passes; `cargo run -p bridge-core --bin uniffi-bindgen -- generate --library target/debug/bridge_core.dll --language kotlin --out-dir spikes/android-app/app/src/main/java` produces `uniffi/bridge_core/bridge_core.kt`.
- [ ] **Step 2: `build-rust.ps1`** — `rustup target add aarch64-linux-android`; `cargo install cargo-ndk`; `$env:ANDROID_NDK_HOME="$env:LOCALAPPDATA\Android\Sdk\ndk\27.0.12077973"`; `cargo ndk -t arm64-v8a -o spikes/android-app/app/src/main/jniLibs build -p bridge-core --release`; then the bindgen command. Expected: `jniLibs/arm64-v8a/libbridge_core.so` exists.
- [ ] **Step 3: App** — Compose screen with: "Receive" (starts `ReceiveService`, a foreground service of type `dataSync` that calls `start_receiver(<Downloads>/Bridge)`, shows the ticket as text + QR), "Send file" (system file picker → `send_file_ffi` to a pasted/scanned ticket, shows MB/s), and a quick-settings tile `ClipTile` that reads the clipboard (allowed: tile tap brings a transparent activity to the foreground) and sends its text as a `clip.txt` transfer to the last ticket. Dependency `net.java.dev.jna:jna` (aar) for UniFFI.
- [ ] **Step 4: Build** — with `JAVA_HOME="C:\Program Files\Android\Android Studio\jbr"`: `./gradlew :app:assembleDebug` in `spikes/android-app`. Expected: `BUILD SUCCESSFUL`.
- [ ] **Step 5: Spike 5 + 7 checks (owner)** — install the APK on the S24+. Phone receives a 5 MB PDF and a 1 GB video from `bridge-send`; phone sends both to `bridge-recv`. Time each and the same files with LocalSend on the same Wi-Fi; record MB/s. Tap the clipboard tile with emoji text copied → `clip.txt` on Windows matches. Record whether the tile flow feels acceptable.
- [ ] **Step 6: Commit** — `git add core spikes/android-app docs/spikes/results.md && git commit -m "spike: UniFFI core in Android app, phone transfer and clipboard tile"`

---

### Task 7: iOS spike app via GitHub Actions + sideload

**Files:**
- Create: `spikes/ios-app/project.yml` (XcodeGen: app target `Spike`, SwiftUI, iOS 17.0, links `BridgeCore.xcframework`), `spikes/ios-app/Sources/{SpikeApp.swift,ContentView.swift}`
- Create: `.github/workflows/ios-spike.yml`

**Interfaces:**
- Consumes: Task 6 UniFFI exports (Swift bindings generated with `--language swift`).

- [ ] **Step 1: Workflow** — on `workflow_dispatch` and pushes touching `core/**` or `spikes/ios-app/**`, `runs-on: macos-latest`: install Rust targets `aarch64-apple-ios`; `cargo build -p bridge-core --release --target aarch64-apple-ios`; generate Swift bindings from the built `libbridge_core.a`; `xcodebuild -create-xcframework` → `BridgeCore.xcframework`; `brew install xcodegen && xcodegen`; `xcodebuild -scheme Spike -sdk iphoneos -configuration Release CODE_SIGNING_ALLOWED=NO build`; zip `Payload/Spike.app` → `Spike.ipa`; upload artifact with `retention-days: 3`.
- [ ] **Step 2: App** — one SwiftUI screen: "Receive" (calls `startReceiver` into the app's Documents dir, shows ticket text; `UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace` in Info so files show in Files → On My iPad → Spike), "Send" (`.fileImporter` → `sendFileFfi` to a pasted ticket, shows MB/s).
- [ ] **Step 3: Run the workflow** — `gh workflow run ios-spike.yml && gh run watch`; expected: green run with `Spike.ipa` artifact. `gh run download` it.
- [ ] **Step 4: Sideload (owner)** — install SideStore (or AltStore) on Windows per its docs, sign in with a free Apple account, install `Spike.ipa`. Record any step that failed.
- [ ] **Step 5: Spike 5 iPad check (owner)** — same PDF/video timings as Task 6 Step 5 against `bridge-recv`/`bridge-send` and LocalSend; check the received PDF opens from the Files app. Record.
- [ ] **Step 6: Commit** — `git add spikes/ios-app .github/workflows/ios-spike.yml docs/spikes/results.md && git commit -m "spike: iOS app built on GitHub Actions and sideloaded"`

---

### Task 8: Spike verdicts

**Files:**
- Modify: `docs/spikes/results.md` (summary table: spike → pass/fail → chosen path from spec §10 fallbacks), `docs/specs/2026-11-02-device-bridge-design.md` (only where a fallback was taken; note it inline with the date)

- [ ] **Step 1: Fill the summary table** from the recorded sections; every spike has pass/fail and the next path.
- [ ] **Step 2: Update the spec** for each fallback taken.
- [ ] **Step 3: Commit** — `git commit -am "docs: spike verdicts"`; then write the v1 plans (input, clipboard, transfer, apps) from these results.
