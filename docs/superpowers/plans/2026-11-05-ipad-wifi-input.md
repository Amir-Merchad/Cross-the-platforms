# iPad Wi-Fi Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Windows keyboard and mouse reach the iPad over Wi-Fi with no Bluetooth, with edge crossing unchanged.

**Architecture:** A new `ipadinput` service owns the iPad tunnel, display stream and virtual devices on its own thread. The engine routes actions for host `ipad-wifi` to it through a non-blocking channel. Pure report building and key translation live in `core`.

**Tech Stack:** Rust, `idevice` =0.1.68, Tauri 2, React, vitest.

**Spec:** `docs/superpowers/specs/2026-11-05-ipad-wifi-input-design.md`. Facts: `docs/spikes/ipad-pointer.md`. Code to port: `spikes/ipad-input`.

## Global Constraints

- Branch `v1.5/input`, stacked on `v1/ui`. Local until the owner says to push. No merge, no PR.
- Host id `ipad-wifi`. One iPad only.
- Pointer report: 19 bytes, `05 dx dy`, 8 zeros, `buttons`, 7 zeros. `dx`, `dy` signed 8-bit.
- Keepalive every 500 ms; two misses, a write error or a stream error mark the link down. Back on Windows within about a second.
- The engine thread never waits on the network. Buttons and keys are never dropped; motion is summed.
- Stream lifetime: only while the iPad is in control if start time is under about 250 ms, otherwise while awake and placed, stopping after 5 idle minutes.
- Status text: "iPad is on a call", "Plug in the iPad once", "iPad not found on Wi-Fi", "Turn on Developer Mode". No raw errors or ids.
- Never log file names, keys or clipboard text. Never read, print or copy pairing record contents.
- Never force-kill `ctp-app`, `ctp-engine` or `ble-hid`. Ask the owner to quit before rebuilding into `target\debug`.
- Never run `git clean -X`, `-fdx` or bare `git stash`. Do not touch `.idea/`, `.red.txt`, `apps/tauri/vite.mock.config.ts`.
- Do not change Windows or iPad settings. Stop if the iPad is locked or on a call.

## Review Focus

1. Wi-Fi drops while the iPad has control: input returns to Windows within about a second, with nothing held down on either side.
2. A call starts while the iPad is in control: control returns or stays usable as proof 4 shows, with the Desk saying why.
3. Bluetooth is off or the adapter is missing at launch: hooks, hotkeys, Esc three times and `ipad-wifi` all still work. Today `engine::run` loops on `HidPeripheral::start` before capture starts, so nothing works.
4. The PC or iPad sleeps and wakes, or its IP changes: the link comes back with no action.
5. A key is held when control leaves, or during a link drop: it is released on the iPad, and no key repeats forever.

---

### Task 1: Finish the proofs

**Files:**
- Modify: `spikes/ipad-input/src/main.rs`, `spikes/ipad-input/src/probe.rs`, `docs/spikes/ipad-pointer.md`

The owner must be present with the iPad unlocked. Run over Wi-Fi unless a proof needs USB.

- [ ] **Step 1: Add one subcommand per proof, and run them**

Record for each: what was sent, evidence, result.
1. `scroll`: report 7, vertical and horizontal, and the units for a usable speed.
2. `rightdrag`: right click through button bit 1, and a drag with the button held.
3. `streamtime`: time from nothing to the first accepted report, 5 runs.
4. `callcheck`: run while the owner starts and ends a call. Does input stop, does the stream die, does it recover?
5. `twotunnels`: a second tunnel open while one is running, using a pasteboard call on one and input on the other.
6. `wifimount`: disk image mount over Wi-Fi through the lockdown route, after the owner restarts the iPad.
7. `idle`: find the keepalive that stops first reports after 5 idle minutes being lost.
8. `keyboard`: Shift, Ctrl, Cmd (`0xE3`), Option, Esc, arrows, F keys, a held key repeating, and German `ä ö ü ß` with the iPad's German layout.

- [ ] **Step 2: Report results and rule**

Update `docs/spikes/ipad-pointer.md`. Apply the spec's fallbacks: proof 5 failing means `ipadinput` owns the one tunnel and clipboard goes through it, and Task 4 changes accordingly. Proof 6 failing means "Plug in the iPad once". If scroll or right click fails, stop and tell the owner.

- [ ] **Step 3: Commit**

`spike(ipad-input): proofs for scroll, clicks, stream time, calls, tunnels, mount, idle, keyboard`

### Task 2: Pairing, disk image and discovery

**Files:**
- Modify: `platform/windows/src/ipad.rs`
- Test: same file

**Interfaces:**
- Produces: `pub async fn connect_tunnel(dir: &Path) -> Result<(AdapterHandle, RsdHandshake, bool), String>` (the `bool` is `wifi`), `pub async fn mount_usb(dir: &Path) -> Result<(), String>`, `pub fn pairing_host_name(seed: &str) -> String`, `pub async fn discover(dir: &Path) -> Result<SocketAddr, String>`.

- [ ] **Step 1: Write the failing tests**

`each_pairing_gets_a_new_name`: `pairing_host_name("a") != pairing_host_name("b")`, and both start with `cross-the-platforms`.
`saved_address_round_trips`: writing and reading `ipad-last-ip.txt` in a temp folder returns the same IP.
`bad_signature_means_pair_again`: `classify("pair verify: early eof")` returns the "needs pairing" status.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p ctp-windows --lib ipad`. Expected: FAIL, functions missing.

- [ ] **Step 3: Implement**

`pair_usb` uses a random seed in `pairing_host_name`, and stores the name with the record, so `RemotePairingClient::new` uses the same name when connecting. Port `mount_usb` and the unicast mDNS lookup from the spike. `discover` tries the saved IP by unicast, then multicast, then a unicast sweep of the local /24 at 32 at a time, then saves the result. Mount over the tunnel is replaced by `mount_usb`, plus the Wi-Fi route if proof 6 passed.

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test -p ctp-windows --lib`, `cargo clippy -p ctp-windows -- -D warnings`. Expected: pass.
Commit: `fix(ipad): new pairing identity each time, lockdown mount, unicast discovery`

### Task 3: Reports, key translation and settings

**Files:**
- Create: `core/src/ipadhid.rs`
- Modify: `core/src/lib.rs`, `core/src/settings.rs`, `apps/tauri/src/lib/api.ts`

**Interfaces:**
- Produces:
  - `pub const IPAD_HOST: &str = "ipad-wifi";`
  - `pub fn pointer_reports(r: &MouseReport) -> Vec<Vec<u8>>`, splitting `dx`, `dy` into signed 8-bit steps, with `buttons` on each.
  - `pub fn scroll_report(wheel: i8, pan: i8) -> Vec<u8>`, as proof 1 found.
  - `pub enum KeyEvent { Down(u8), Up(u8) }`
  - `pub struct KeyTranslator` with `pub fn apply(&mut self, r: &KeyboardReport) -> Vec<KeyEvent>` and `pub fn release_all(&mut self) -> Vec<KeyEvent>`. Modifier bits map to usages `0xE0` to `0xE7`.
  - `Settings.bluetooth_enabled: bool` (default true), `Settings.retired_hosts: Vec<String>`.

- [ ] **Step 1: Write the failing tests**

`pointer_report_layout`: `MouseReport { dx: 5, dy: -3, buttons: 1, .. }` gives one report of 19 bytes with byte 0 `5`, byte 1 `5`, byte 2 `0xFD`, byte 11 `1`, all else 0.
`large_motion_splits_to_i8`: `dx: 300` gives reports whose `dx` bytes sum to 300, each in -128..=127.
`press_and_release_yield_events`: `apply` of `keys [4]` gives `[Down(4)]`; `apply` of empty gives `[Up(4)]`.
`modifier_bits_become_usages`: `modifiers: 0x02` gives `Down(0xE1)`.
`release_all_lets_go_of_everything` (Review Focus 5): after holding `a` and Shift, `release_all` gives both `Up`, and a second call gives nothing.
`older_file_keeps_bluetooth_on`: settings without the fields load with `bluetooth_enabled == true`, and `retired_hosts` empty.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p bridge-core ipadhid`. Expected: FAIL, module missing.

- [ ] **Step 3: Implement, then run**

Run: `cargo test -p bridge-core`. Expected: pass.

- [ ] **Step 4: Commit**

`feat(core): iPad pointer reports, key translation, Bluetooth and retired-host settings`

### Task 4: The iPad input service

**Files:**
- Create: `platform/windows/src/ipadinput.rs`
- Modify: `platform/windows/src/lib.rs`

**Interfaces:**
- Consumes: Task 2 `connect_tunnel`, Task 3 reports.
- Produces:
  - `pub enum IpadInputState { NotSetUp, Connecting, Ready, OnCall, NeedsUsb, NotFound, NeedsDeveloperMode }`
  - `pub enum IpadCommand { Mouse(MouseReport), Keyboard(KeyboardReport), Enter, Leave, Stop }`
  - `pub struct IpadInputHandle` with `pub fn send(&self, c: IpadCommand)` (never blocks) and `Drop` stopping the thread.
  - `pub fn start(dir: PathBuf, on_state: impl Fn(IpadInputState) + Send + 'static) -> IpadInputHandle`

- [ ] **Step 1: Write the failing tests for the pure parts**

`classify_stream_error`: a "phone or VoIP call" error gives `OnCall`. `missed_keepalives_mark_down`: a `Keepalive` struct with `tick(ok: bool) -> bool` returns down after two misses and resets on success (Review Focus 1). `motion_is_summed_while_busy`: a `Pending` buffer given two moves yields one summed report, and keeps button changes separate and in order.

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p ctp-windows --lib ipadinput`. Expected: FAIL.

- [ ] **Step 3: Implement the service**

Single-threaded tokio with `LocalSet`, as `clipsync.rs` does. Port stream start, media discard, RTCP liveness, virtual mouse and keyboard creation from the spike. `Leave` and link loss send `release_all` first when the link allows. Reconnect with backoff of 2 s up to 30 s. Apply the stream lifetime rule from proof 3 and the keepalive from proof 7. Remove virtual services and stop the stream on `Stop`.

- [ ] **Step 4: Run on the device**

A temporary `ble-hid`-style binary is not needed: add `spikes/ipad-input` subcommand `service`, driving `ipadinput::start` with a scripted square and typing. Expected: pointer moves and text appears, over Wi-Fi.

- [ ] **Step 5: Run tests, then commit**

Run: `cargo test -p ctp-windows --lib`, `cargo clippy -p ctp-windows -- -D warnings`. Expected: pass.
Commit: `feat(windows): iPad input service over the tunnel`

### Task 5: Engine routing and optional Bluetooth

**Files:**
- Modify: `platform/windows/src/engine.rs`, `apps/tauri/src-tauri/src/main.rs`

**Interfaces:**
- Consumes: Task 4 `IpadInputHandle`, `IPAD_HOST`, `Settings.bluetooth_enabled`.
- Produces: `Status.ipad: Option<IpadInputState>`, serialized for the hub. `Status.hosts` includes `ipad-wifi` with `connected` true only when `Ready`.

- [ ] **Step 1: Write the failing tests**

Extract `fn sink_for(host: &str) -> Sink` (`Sink::Ipad` or `Sink::Ble`) and test `ipad_wifi_goes_to_the_service`. Extract `fn wants_peripheral(s: &Settings) -> bool` and test both values. In `core/src/router.rs` tests, add `ipad_wifi_disconnect_returns_to_windows`: entering host `ipad-wifi`, then `connected` false on `tick`, yields `Suppress(false)` and `Active(None)` (Review Focus 1).

- [ ] **Step 2: Run them and see them fail**

Run: `cargo test -p ctp-windows --lib engine`, `cargo test -p bridge-core router`. Expected: FAIL for the new functions.

- [ ] **Step 3: Implement**

`peripheral` becomes `Option<Arc<HidPeripheral>>`. `run` no longer blocks on `HidPeripheral::start`: capture, router and hotkeys start at once, and the peripheral is retried in the background when `bluetooth_enabled` (Review Focus 3). Route `Action::Mouse` and `Action::Keyboard` by `sink_for`. Send `Enter` and `Leave` from `Action::Active`. The BLE watcher runs only with a peripheral. `set_interval(IPAD_HOST, 8)`.

- [ ] **Step 4: Run tests, then commit**

Run: `cargo test -p bridge-core`, `cargo test -p ctp-windows --lib`, `cargo clippy -p bridge-core -p ctp-windows -- -D warnings`, `cargo check -p ctp-app`. Expected: pass.
Commit: `feat(windows): route iPad input over Wi-Fi, make Bluetooth optional`

### Task 6: Hub

**Files:**
- Modify: `apps/tauri/src/lib/devices.ts`, `apps/tauri/src/lib/addflow.ts`, `apps/tauri/src/lib/api.ts`, `apps/tauri/src/pages/Settings.tsx`, `apps/tauri/src/pages/AddDevice.tsx`, `apps/tauri/src/pages/Desk.tsx`, `apps/tauri/src/components/DevicePanel.tsx`, `apps/tauri/src/lib/hub.tsx`
- Test: `devices.test.ts`, `addflow.test.ts`

**Interfaces:**
- Consumes: `Status.ipad`, `Settings.retired_hosts`, `Settings.bluetooth_enabled`.
- Produces: `migrateIpad(s: Settings, peers: Peer[]): Settings`, `ipadLine(state: IpadInputState | null): string`.

- [ ] **Step 1: Write the failing tests**

`migrate_moves_the_ipad_once`: an attachment on a host linked to an Ipad peer moves to `ipad-wifi` in the same list position, the link moves, the old host joins `retired_hosts`, and running it again changes nothing.
`migrate_without_a_link_does_nothing`.
`retired_hosts_are_not_listed`: `buildDevices` omits them.
`ipad_states_read_plainly`: each state gives the Global Constraints text.
`ipad_add_flow_has_two_steps`: `addStep("ipad", ...)` runs USB, then Files.

- [ ] **Step 2: Run them and see them fail**

Run: `pnpm test` in `apps/tauri`. Expected: FAIL.

- [ ] **Step 3: Implement**

Run `migrateIpad` once on load through `update`. Settings gets a Bluetooth switch. The Bluetooth banner shows only when `bluetooth_enabled` and unavailable. iPad steps: "Plug in by USB and tap Trust" with Pair over USB, explaining Developer Mode, then the code for files.

- [ ] **Step 4: Run tests, then commit**

Run: `pnpm test`, `pnpm exec tsc --noEmit -p tsconfig.app.json`. Expected: pass.
Commit: `feat(hub): iPad over Wi-Fi states, migration, Bluetooth switch, one-step iPad setup`

### Task 7: Device check and docs

**Files:**
- Create: `docs/v1.5/checklist-ipad-input.md`
- Modify: `docs/HANDOFF.md`, `docs/superpowers/specs/2026-11-05-v1.5-v2-roadmap.md`

- [ ] **Step 1: Write the checklist**

One row per spec "Done means" line and Review Focus item, including Wi-Fi pulled mid-control and a call.

- [ ] **Step 2: Build**

Ask the owner to quit from the tray, then run `pnpm tauri build --debug --no-bundle` in `apps/tauri`. Expected: `target\debug\ctp-app.exe`.

- [ ] **Step 3: Owner runs the checklist with Bluetooth off**

Fix what fails, with a test where logic is involved. Record results.

- [ ] **Step 4: Update docs and commit**

`docs: iPad input over Wi-Fi checklist and handoff`
