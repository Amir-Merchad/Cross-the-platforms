# v1 Plan 1: Windows Hub App + Keyboard/Mouse Sharing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Windows tray app (Tauri + shadcn/ui) that shares the laptop's keyboard and mouse with the iPad and S24+ over one BLE HID link, with a layout editor, edge switching, hotkeys, calibration and diagnostics — the spike behaviour turned into a product.

**Architecture:** Pure, unit-tested logic in `core/` (layout geometry, settings, an input `Router` state machine that owns edge entry/return, pacing, held keys and hotkeys). Windows-only code in a new `platform/windows` crate (BLE HID peripheral, Raw Input + hooks capture, monitor enumeration, an `Engine` thread that wires capture → router → peripheral). The Tauri app in `apps/tauri` hosts the engine, tray and settings UI. Spike code under `spikes/` stays frozen as reference.

**Tech Stack:** Rust 1.97, `windows` 0.62, `serde`/`serde_json`, Tauri 2 (+ `tauri-plugin-autostart`, `tauri-plugin-single-instance`), React 19 + Vite + TypeScript + Tailwind v4 + shadcn/ui, pnpm.

**Spec:** `docs/specs/2026-11-02-device-bridge-design.md` (§3 goals/anti-goals, §4 apps, §5.1 input). Device facts: `docs/spikes/results.md`. Proven reference code: `spikes/ble-hid/src/{peripheral,capture,keys,main}.rs`, `core/src/{hid,edge}.rs`.

## Global Constraints

- Windows must never be left without working input: Ctrl+Alt+Q and Esc ×3 within 1 s always return to Windows instantly (input released *before* any BLE send); app exit, panic or host disconnect returns control too.
- Never log keystrokes, clipboard contents or file names; diagnostics are counts, timings and device ids only.
- Nothing to start by hand after reboot: app starts with Windows (tray, no window), re-advertises and reconnects by itself.
- BLE GATT layout, report formats and timings exactly as the passing spike: 8-byte keyboard / 7-byte mouse reports (`core::hid`), wait up to 12 s through transient `Aborted`, keep all characteristics alive, stop advertising on drop, never force-kill the publisher in scripts.
- Motion paced at the host's BLE connection interval (`ConnectionInterval × 1.25 ms`, clamped 8–100 ms; iPad gives 15 ms); request `ThroughputOptimized` connection parameters per host.
- Return guard defaults: 1 s entry grace, dwell 300 ms, min push 96 counts, max gap 180 ms; entry hold 300 ms. All per-device tunable.
- Hotkeys (v1 fixed): Ctrl+Alt+Q → Windows; Ctrl+Alt+1…9 → n-th attached device; Esc ×3 → Windows.
- Settings live at `%APPDATA%\CrossThePlatforms\settings.json`; devices are keyed by BLE device id, never by name.
- All `core/` logic is platform-free and unit-tested; `platform/windows` and `apps/tauri` are verified by build + the manual checklist in Task 8.

## Review Focus

1. **Settings file missing, corrupt or from an older version** → app starts with defaults, the bad file is kept as `settings.json.bak`, nothing crashes. Test in Task 2.
2. **Monitor layout changed** (external monitor unplugged, device attached to a monitor that no longer exists) → that attachment is inactive, other edges and hotkeys still work. Test in Task 1.
3. **Hotkey for a device that isn't connected** → stays on Windows, no input suppression, status says why. Test in Task 3.
4. **Two devices attached to the same edge of the same monitor** → settings validation rejects it with a message; UI can't save it. Test in Task 1.
5. **Pointer crossing an edge that has another monitor beyond it** (e.g. laptop screen's right edge with an external monitor to the right) → never enters a device there; Windows' own multi-monitor movement wins. Test in Task 1.

---

### Task 1: `core::layout` — monitors, attachments, entry detection

**Files:**
- Create: `core/src/layout.rs`; modify `core/src/lib.rs` (`pub mod layout;`), `core/Cargo.toml` (serde already present)
- Test: `core/src/layout.rs` tests

**Interfaces:**
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)] pub enum Edge { Left, Right, Top, Bottom }`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Monitor { pub id: String, pub rect: Rect }` and `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Rect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }` (right/bottom exclusive, Windows convention)
  - `#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)] pub struct Attachment { pub device_id: String, pub monitor_id: String, pub edge: Edge }`
  - `pub fn validate(attachments: &[Attachment]) -> Result<(), String>` — error text names the clash, e.g. `"two devices on the Right edge of \\\\.\\DISPLAY1"`
  - `pub fn entry<'a>(attachments: &'a [Attachment], monitors: &[Monitor], cursor: (i32, i32), dx: i32, dy: i32) -> Option<&'a Attachment>` — `Some` when the cursor is on the attachment's monitor edge pixel (`x == right-1`, `x == left`, `y == top`, `y == bottom-1`), the delta pushes outward, and no other monitor touches that edge at the cursor's position
  - `pub fn side(edge: Edge) -> crate::edge::Side` mapping Windows-side edge to the remote's touching edge (`Right → Side::Left`, `Left → Side::Right`; Top/Bottom map to `Side::Left`/`Side::Right` on the vertical axis — see Task 3)

- [ ] **Step 1: Write the failing tests**

```rust
fn mon(id: &str, l: i32, t: i32, r: i32, b: i32) -> Monitor { Monitor { id: id.into(), rect: Rect { left: l, top: t, right: r, bottom: b } } }
fn att(d: &str, m: &str, e: Edge) -> Attachment { Attachment { device_id: d.into(), monitor_id: m.into(), edge: e } }

#[test] fn right_edge_push_enters() {
    let a = [att("ipad", "M1", Edge::Right)]; let m = [mon("M1", 0, 0, 1920, 1080)];
    assert_eq!(entry(&a, &m, (1919, 500), 3, 0).map(|x| x.device_id.as_str()), Some("ipad"));
}
#[test] fn moving_inward_at_edge_does_not_enter() {
    let a = [att("ipad", "M1", Edge::Right)]; let m = [mon("M1", 0, 0, 1920, 1080)];
    assert!(entry(&a, &m, (1919, 500), -3, 0).is_none());
}
#[test] fn left_edge_enters_left_device() {
    let a = [att("ipad", "M1", Edge::Right), att("s24", "M1", Edge::Left)]; let m = [mon("M1", 0, 0, 1920, 1080)];
    assert_eq!(entry(&a, &m, (0, 10), -2, 0).unwrap().device_id, "s24");
}
#[test] fn neighbouring_monitor_wins_over_device() { // Review Focus 5
    let a = [att("ipad", "M1", Edge::Right)];
    let m = [mon("M1", 0, 0, 1920, 1080), mon("M2", 1920, 0, 3840, 1080)];
    assert!(entry(&a, &m, (1919, 500), 3, 0).is_none());
}
#[test] fn neighbour_only_covering_part_of_edge() {
    let a = [att("ipad", "M1", Edge::Right)];
    let m = [mon("M1", 0, 0, 1920, 1080), mon("M2", 1920, 0, 3840, 400)];
    assert!(entry(&a, &m, (1919, 200), 3, 0).is_none());
    assert!(entry(&a, &m, (1919, 800), 3, 0).is_some());
}
#[test] fn missing_monitor_disables_attachment() { // Review Focus 2
    let a = [att("ipad", "GONE", Edge::Right)]; let m = [mon("M1", 0, 0, 1920, 1080)];
    assert!(entry(&a, &m, (1919, 500), 3, 0).is_none());
}
#[test] fn same_edge_twice_is_invalid() { // Review Focus 4
    let e = validate(&[att("a", "M1", Edge::Right), att("b", "M1", Edge::Right)]).unwrap_err();
    assert!(e.contains("Right") && e.contains("M1"));
}
#[test] fn same_device_twice_is_invalid() {
    assert!(validate(&[att("a", "M1", Edge::Right), att("a", "M1", Edge::Left)]).is_err());
}
#[test] fn distinct_edges_are_valid() { assert!(validate(&[att("a", "M1", Edge::Right), att("b", "M1", Edge::Left)]).is_ok()); }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core layout` → compile errors.
- [ ] **Step 3: Implement `layout.rs`** to the interfaces. A neighbour "touches" the edge at the cursor when its rect shares the edge line (`other.left == m.right` for Right) and spans the cursor's perpendicular coordinate.
- [ ] **Step 4: Run tests** — `cargo test -p bridge-core && cargo clippy -p bridge-core -- -D warnings` → pass, no warnings.
- [ ] **Step 5: Commit** — `git add core && git commit -m "feat(core): monitor layout and edge entry"`

---

### Task 2: `core::settings` — persisted settings

**Files:**
- Create: `core/src/settings.rs`; modify `core/src/lib.rs`
- Test: `core/src/settings.rs` tests (`tempfile` dev-dep exists)

**Interfaces:**
- Consumes: `layout::{Attachment, validate}`
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(default)] pub struct Calibration { pub gain: f64, pub min_push: f64, pub dwell_ms: u64 }` — `Default`: `1.0, 96.0, 300`
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(default)] pub struct Settings { pub version: u32, pub attachments: Vec<Attachment>, pub calibration: std::collections::BTreeMap<String, Calibration>, pub entry_hold_ms: u64, pub start_with_windows: bool, pub remote_width: std::collections::BTreeMap<String, f64> }` — `Default`: `version 1`, empty, `entry_hold_ms 300`, `start_with_windows true`; `remote_width` default per device `1366.0` (iPad Air 13" points) via `pub fn width_for(&self, id: &str) -> f64`
  - `pub fn calibration_for(&self, id: &str) -> Calibration` (default if absent)
  - `pub fn load(path: &Path) -> Settings` — missing → default; unreadable/corrupt/`validate` fails → rename to `settings.json.bak`, return default
  - `pub fn save(&self, path: &Path) -> Result<(), String>` — runs `validate`, writes `settings.json.tmp` then renames over the target; creates parent dirs

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn missing_file_gives_defaults() { let d = tempdir().unwrap(); assert_eq!(Settings::load(&d.path().join("s.json")), Settings::default()); }
#[test] fn round_trip() {
    let d = tempdir().unwrap(); let p = d.path().join("s.json");
    let mut s = Settings::default();
    s.attachments.push(Attachment { device_id: "ipad".into(), monitor_id: "M1".into(), edge: Edge::Right });
    s.calibration.insert("ipad".into(), Calibration { gain: 1.4, min_push: 120.0, dwell_ms: 250 });
    s.save(&p).unwrap();
    assert_eq!(Settings::load(&p), s);
}
#[test] fn corrupt_file_is_backed_up_and_defaults_used() { // Review Focus 1
    let d = tempdir().unwrap(); let p = d.path().join("settings.json");
    std::fs::write(&p, "{not json").unwrap();
    assert_eq!(Settings::load(&p), Settings::default());
    assert_eq!(std::fs::read_to_string(d.path().join("settings.json.bak")).unwrap(), "{not json");
}
#[test] fn older_file_missing_fields_keeps_known_ones() {
    let d = tempdir().unwrap(); let p = d.path().join("s.json");
    std::fs::write(&p, r#"{"version":1,"entry_hold_ms":500}"#).unwrap();
    let s = Settings::load(&p);
    assert_eq!(s.entry_hold_ms, 500); assert!(s.start_with_windows);
}
#[test] fn invalid_layout_is_not_saved() {
    let d = tempdir().unwrap(); let p = d.path().join("s.json");
    let mut s = Settings::default();
    for id in ["a", "b"] { s.attachments.push(Attachment { device_id: id.into(), monitor_id: "M1".into(), edge: Edge::Right }); }
    assert!(s.save(&p).is_err()); assert!(!p.exists());
}
#[test] fn calibration_defaults_for_unknown_device() { assert_eq!(Settings::default().calibration_for("x"), Calibration { gain: 1.0, min_push: 96.0, dwell_ms: 300 }); }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core settings` → compile errors.
- [ ] **Step 3: Implement `settings.rs`.**
- [ ] **Step 4: Run tests + clippy** — pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): persisted settings with safe fallback"`

---

### Task 3: `core::router` — input routing state machine

**Files:**
- Create: `core/src/router.rs`; modify `core/src/lib.rs`
- Test: `core/src/router.rs` tests

**Interfaces:**
- Consumes: `hid::{HeldInput, MouseReport, KeyboardReport, split_delta}`, `edge::{EdgeTracker, EdgeEvent}`, `layout::{entry, side, Monitor}`, `settings::Settings`
- Produces:
  - `pub enum Input { Move { dx: i32, dy: i32, cursor: (i32, i32) }, Button { mask: u8, down: bool }, Wheel { v: i8, h: i8 }, Key { vk: u16, down: bool }, Hotkey(u8), Escape }` — `Hotkey(0)` = back to Windows, `1..=9` = n-th attachment in settings order
  - `#[derive(Debug, PartialEq)] pub enum Action { Suppress(bool), Mouse { host: String, report: MouseReport }, Keyboard { host: String, report: KeyboardReport }, PlaceCursor(i32, i32), Active(Option<String>), Notice(String) }`
  - `pub struct Router` with `pub fn new(settings: Settings, monitors: Vec<Monitor>) -> Self`, `pub fn set_settings(&mut self, s: Settings)`, `pub fn set_monitors(&mut self, m: Vec<Monitor>)`, `pub fn set_interval(&mut self, host: &str, ms: u64)`
  - `pub fn on_input(&mut self, input: Input, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action>`
  - `pub fn tick(&mut self, now_ms: u64, connected: &dyn Fn(&str) -> bool) -> Vec<Action>` — flushes paced motion when due; leaves the remote if the active host is no longer connected
  - `pub fn active(&self) -> Option<&str>`, `pub fn next_tick_ms(&self) -> Option<u64>`
- Behaviour (ported from `spikes/ble-hid/src/main.rs`, which passed on device):
  - While local: `Move` at an attachment edge (Task 1 `entry`) for ≥ `entry_hold_ms` continuously → enter: `[Suppress(true), Mouse{pin}…, Active(Some(id))]`, tracker `enter(now)`. Leaving the edge resets the hold timer.
  - `Hotkey(n)` for a connected device → enter it the same way (no hold); for a disconnected one → `[Notice("<name or id> is not connected")]` and stay local (Review Focus 3).
  - While remote: moves accumulate; `tick` emits at most one `Mouse` move report per host interval (default 8 ms until `set_interval`), splitting with `split_delta`; buttons/wheel/keys flush pending motion first, then emit immediately; `EdgeEvent::Return`, `Hotkey(0)`, `Escape` or disconnect → leave.
  - Leave: `[Suppress(false), Keyboard{release}, Mouse{release}, PlaceCursor(inside the edge by 20 px at the cursor's last y/x), Active(None)]` — `Suppress(false)` always first.
  - Top/Bottom attachments use `dy` for the tracker instead of `dx`.

- [ ] **Step 1: Write the failing tests** (helper `fn r() -> Router` with monitor `M1 0,0,1920,1080` and `ipad` on Right, `s24` on Left; `fn up(_: &str) -> bool { true }`, `fn down(_: &str) -> bool { false }`)

```rust
#[test] fn hold_at_edge_enters_after_300ms() {
    let mut r = r();
    assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &up).is_empty());
    let a = r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 300, &up);
    assert_eq!(a.first(), Some(&Action::Suppress(true)));
    assert!(a.contains(&Action::Active(Some("ipad".into()))));
    assert_eq!(r.active(), Some("ipad"));
}
#[test] fn leaving_edge_resets_hold() {
    let mut r = r();
    r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 0, &up);
    r.on_input(Input::Move { dx: -3, dy: 0, cursor: (1900, 500) }, 100, &up);
    assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 350, &up).is_empty());
}
#[test] fn hotkey_enters_connected_device() {
    let mut r = r();
    let a = r.on_input(Input::Hotkey(2), 0, &up);
    assert!(a.contains(&Action::Active(Some("s24".into()))));
}
#[test] fn hotkey_for_disconnected_device_stays_local() { // Review Focus 3
    let mut r = r();
    let a = r.on_input(Input::Hotkey(1), 0, &down);
    assert!(!a.contains(&Action::Suppress(true)));
    assert!(matches!(a.as_slice(), [Action::Notice(_)]));
    assert_eq!(r.active(), None);
}
#[test] fn escape_leaves_with_suppress_false_first() {
    let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
    r.on_input(Input::Key { vk: 0xA0, down: true }, 10, &up); // Shift held on remote
    let a = r.on_input(Input::Escape, 20, &up);
    assert_eq!(a.first(), Some(&Action::Suppress(false)));
    assert!(a.iter().any(|x| matches!(x, Action::Keyboard { report, .. } if report.to_bytes() == [0; 8])));
    assert!(a.contains(&Action::Active(None)));
}
#[test] fn motion_is_paced_by_interval() {
    let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up); r.set_interval("ipad", 15);
    for t in 1..=10 { r.on_input(Input::Move { dx: 1, dy: 0, cursor: (0, 0) }, 2000 + t, &up); }
    let a = r.tick(2016, &up);
    let moves: Vec<_> = a.iter().filter_map(|x| match x { Action::Mouse { report, .. } => Some(report.dx), _ => None }).collect();
    assert_eq!(moves, vec![10]); // coalesced into one report
    assert!(r.tick(2020, &up).is_empty(), "next report not before 2031");
}
#[test] fn disconnect_on_tick_leaves() {
    let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
    let a = r.tick(100, &down);
    assert_eq!(a.first(), Some(&Action::Suppress(false)));
    assert_eq!(r.active(), None);
}
#[test] fn click_flushes_pending_motion_first() {
    let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up);
    r.on_input(Input::Move { dx: 5, dy: 0, cursor: (0, 0) }, 2000, &up);
    let a = r.on_input(Input::Button { mask: 1, down: true }, 2001, &up);
    let kinds: Vec<_> = a.iter().filter_map(|x| match x { Action::Mouse { report, .. } => Some((report.dx, report.buttons)), _ => None }).collect();
    assert_eq!(kinds, vec![(5, 0), (0, 1)]);
}
#[test] fn deliberate_push_back_returns() {
    let mut r = r(); r.on_input(Input::Hotkey(1), 0, &up); // ipad on Right → its Left edge faces Windows
    let mut left = false;
    for i in 0..40u64 { let a = r.on_input(Input::Move { dx: -5, dy: 0, cursor: (0, 0) }, 2000 + i * 10, &up); a.iter().for_each(|x| left |= *x == Action::Active(None)); r.tick(2000 + i * 10, &up); }
    assert!(left);
}
#[test] fn monitor_unplugged_keeps_hotkeys_working() { // Review Focus 2
    let mut r = r(); r.set_monitors(vec![]);
    assert!(r.on_input(Input::Move { dx: 3, dy: 0, cursor: (1919, 500) }, 400, &up).is_empty());
    assert!(r.on_input(Input::Hotkey(1), 500, &up).contains(&Action::Active(Some("ipad".into()))));
}
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core router` → compile errors.
- [ ] **Step 3: Implement `router.rs`.** One `EdgeTracker` per active entry, built from `settings.calibration_for(id)` and `settings.width_for(id)`; pin with `split_delta(tracker.pin_delta(), 0)` on the tracker's axis.
- [ ] **Step 4: Run tests + clippy** — pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): input router (edge entry, pacing, hotkeys, safe leave)"`

---

### Task 4: `platform/windows` — peripheral, capture, monitors

**Files:**
- Create: `platform/windows/Cargo.toml` (crate `ctp-windows`, deps `bridge-core`, `windows` 0.62 with the spike's feature list), `platform/windows/src/{lib.rs, peripheral.rs, capture.rs, keys.rs, monitors.rs}`; add `platform/windows` to workspace members
- Test: `platform/windows/src/keys.rs` tests (moved from the spike, extended)

**Interfaces:**
- Consumes: `bridge_core::router::Input`, `bridge_core::layout::{Monitor, Rect}`
- Produces:
  - `peripheral::HidPeripheral` — port of `spikes/ble-hid/src/peripheral.rs` as is (start/hosts/connected/report_interval_ms/request_fast/send_mouse/send_keyboard/take_stats/Drop), plus `pub fn host_name(&self, id: &str) -> Option<String>` from `BluetoothLEDevice.Name`
  - `capture::start(tx: std::sync::mpsc::Sender<Input>) -> CaptureHandle` (own thread with message loop), `capture::SUPPRESS: AtomicBool`, `CaptureHandle::stop(&self)`, `capture::unhook()`; emits `Input::Move` **always** (cursor from `GetCursorPos`, deltas from Raw Input), and buttons/wheel/keys only while suppressed. Fixes from the spike review: accumulate hi-res wheel deltas (<120) and emit `Wheel{v,h}` incl. `RI_MOUSE_HWHEEL`; ignore `MOUSE_MOVE_ABSOLUTE` packets; panic hook calls `unhook()`.
  - `keys::KeyGate::on_key(vk, down, suppress) -> Verdict` with `Verdict { PassToWindows, Remote, Escape, Hotkey(u8) }`: Ctrl+Alt+Q → `Hotkey(0)` while remote (instead of `Escape`) and Ctrl+Alt+1…9 → `Hotkey(n)` **in both states**; hotkey keys are swallowed. The hook sets `SUPPRESS=false` itself for `Escape` and `Hotkey(0)` before sending.
  - `monitors::list() -> Vec<Monitor>` via `EnumDisplayMonitors` + `GetMonitorInfoW` (`szDevice` as id).

- [ ] **Step 1: Write the failing tests** (keep all spike `KeyGate` tests, adjusted: `ctrl_alt_q_escapes_while_suppressed` now expects `Verdict::Hotkey(0)`; add):

```rust
#[test] fn ctrl_alt_digit_is_hotkey_when_local() {
    let mut g = KeyGate::default();
    g.on_key(0xA2, true, false); g.on_key(0xA4, true, false);
    assert_eq!(g.on_key(0x32, true, false), Verdict::Hotkey(2));
}
#[test] fn ctrl_alt_digit_is_hotkey_when_remote() {
    let mut g = KeyGate::default();
    g.on_key(0xA2, true, true); g.on_key(0xA4, true, true);
    assert_eq!(g.on_key(0x31, true, true), Verdict::Hotkey(1));
}
#[test] fn digit_alone_is_not_hotkey() { let mut g = KeyGate::default(); assert_eq!(g.on_key(0x31, true, false), Verdict::PassToWindows); }
#[test] fn hotkey_key_up_is_swallowed() {
    let mut g = KeyGate::default();
    g.on_key(0xA2, true, false); g.on_key(0xA4, true, false); g.on_key(0x31, true, false);
    assert_ne!(g.on_key(0x31, false, false), Verdict::PassToWindows);
}
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p ctp-windows` → failures/compile errors.
- [ ] **Step 3: Port the code**, applying the interface changes above. No behaviour change in `peripheral.rs`.
- [ ] **Step 4: Run** — `cargo test -p ctp-windows && cargo clippy -p ctp-windows -- -D warnings` → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(platform/windows): BLE HID peripheral, capture, monitors"`

---

### Task 5: `platform/windows::engine` — wiring and lifecycle

**Files:**
- Create: `platform/windows/src/engine.rs`, `platform/windows/src/bin/ctp-engine.rs` (headless runner for manual testing: loads settings, prints status lines)

**Interfaces:**
- Consumes: Tasks 2–4
- Produces:
  - `pub enum Command { Reload(Settings), SwitchTo(Option<String>), Stop }`
  - `#[derive(Clone, Debug, Serialize)] pub struct Status { pub advertising: bool, pub hosts: Vec<HostStatus>, pub active: Option<String>, pub notice: Option<String> }`, `#[derive(Clone, Debug, Serialize)] pub struct HostStatus { pub id: String, pub name: Option<String>, pub connected: bool, pub interval_ms: u64, pub notify_avg_ms: f64, pub notify_max_ms: f64 }`
  - `pub fn start(settings: Settings, on_status: impl Fn(Status) + Send + 'static) -> EngineHandle`; `EngineHandle::send(&self, Command)`; `Drop` sends `Stop`, joins, unhooks, drops the peripheral (stops advertising)
- Loop: `recv_timeout(router.next_tick_ms())` → `router.on_input`/`tick` → execute actions (`Suppress` → `capture::SUPPRESS`, `Mouse`/`Keyboard` → peripheral, `PlaceCursor` → `SetCursorPos`); every 200 ms poll `connected()` for hosts, refresh `set_interval` + `request_fast` for newly connected hosts (keep requests alive), `monitors::list()` on `WM_DISPLAYCHANGE` or every 2 s; emit `Status` on change and every 1 s while remote. Advertising start failure → retry every 5 s with `Status.notice`.

- [ ] **Step 1: Build** — `cargo build -p ctp-windows --release` → succeeds.
- [ ] **Step 2: Manual run (owner)** — `target\release\ctp-engine.exe` with a settings file attaching the iPad id (`…5b:95:30:4d:1c:f5`) to the primary monitor's Right edge: enter/return, Ctrl+Alt+1, Ctrl+Alt+Q, Esc ×3, Bluetooth-off disconnect. Expected: identical behaviour to the passing spike checks. Record in `docs/v1/checklist-input.md`.
- [ ] **Step 3: Commit** — `git commit -m "feat(platform/windows): engine wiring capture, router and BLE"`

---

### Task 6: Tauri app shell — tray, single instance, autostart, engine host

**Files:**
- Create: `apps/tauri/` via `pnpm create tauri-app` (React + TypeScript + Vite), then: `apps/tauri/src-tauri/{Cargo.toml, tauri.conf.json, src/main.rs, src/tray.rs, src/commands.rs}`; add `apps/tauri/src-tauri` to workspace members; Tailwind v4 + shadcn init (`pnpm dlx shadcn@latest init`) in `apps/tauri`
- Modify: root `.gitignore` (`apps/tauri/node_modules/`, `apps/tauri/dist/`)

**Interfaces:**
- Consumes: Task 5 `engine::{start, Command, Status}`, Task 2 `Settings::{load, save}`
- Produces (Tauri commands, used by Task 7):
  - `get_settings() -> Settings`, `save_settings(settings: Settings) -> Result<(), String>` (save + `Command::Reload`), `get_status() -> Status`, `list_monitors() -> Vec<Monitor>` (serde-serialised), `switch_to(device_id: Option<String>)`
  - Event `status` (payload `Status`) emitted on every engine status change
- App behaviour: starts hidden to the tray (`"visible": false`), window title "Cross the Platforms", tray menu: status line (disabled item), one item per connected host ("Switch to <name>"), "Back to Windows", "Settings…", "Quit"; left-click opens the window; closing the window hides it. `tauri-plugin-single-instance` focuses the existing window; `tauri-plugin-autostart` enabled when `settings.start_with_windows`. Quit drops the engine handle before exit.

- [ ] **Step 1: Scaffold and build** — `cd apps/tauri && pnpm install && pnpm tauri build --debug` → produces `ctp.exe` (app identifier `dev.crosstheplatforms.app`).
- [ ] **Step 2: Manual (owner)** — launch: tray icon appears, no window; tray "Back to Windows" and per-device switch work; a second launch only focuses the first; after reboot the tray icon is there and the iPad reconnects without touching anything.
- [ ] **Step 3: Commit** — `git commit -m "feat(apps/tauri): tray app hosting the input engine"`

---

### Task 7: Settings UI (shadcn) — devices, layout, calibration, diagnostics

**Files:**
- Create: `apps/tauri/src/{App.tsx, lib/api.ts, pages/Devices.tsx, pages/Layout.tsx, pages/Calibration.tsx, pages/Diagnostics.tsx}`, shadcn components via `pnpm dlx shadcn@latest add tabs card button select slider switch badge alert`
- Test: `apps/tauri/src/lib/layout.test.ts` (Vitest)

**Interfaces:**
- Consumes: Task 6 commands and `status` event
- Produces:
  - `lib/api.ts`: typed wrappers `getSettings()`, `saveSettings(s)`, `getStatus()`, `listMonitors()`, `switchTo(id)`, `onStatus(cb)`; TS types mirroring `Settings`, `Attachment`, `Calibration`, `Monitor`, `Status`, `HostStatus`
  - `lib/layout.ts`: `clashes(attachments: Attachment[]): string | null` — same rule as Rust `validate` so the Save button disables before the backend rejects
- Pages (tabs): **Devices** — advertising state, "Pair from the device's Bluetooth settings: this PC is called <hostname>", list of hosts with name, connected badge, "Switch now"; **Layout** — monitors drawn to scale from `listMonitors()`, each connected host gets a monitor + edge select (Left/Right/Top/Bottom), clash message from `clashes()`; **Calibration** — per device sliders: gain 0.5–2.0, min push 32–256 counts, dwell 100–800 ms, remote width 600–2000; plus entry hold 100–800 ms and "Start with Windows" switch; **Diagnostics** — per host interval, notify avg/max ms, last notice; never shows key or clipboard data. Hotkeys listed read-only (Global Constraints).

- [ ] **Step 1: Write the failing test**

```ts
import { clashes } from "./layout";
test("same edge twice clashes", () => {
  expect(clashes([{ device_id: "a", monitor_id: "M1", edge: "Right" }, { device_id: "b", monitor_id: "M1", edge: "Right" }])).toMatch(/Right/);
});
test("distinct edges are fine", () => {
  expect(clashes([{ device_id: "a", monitor_id: "M1", edge: "Right" }, { device_id: "b", monitor_id: "M1", edge: "Left" }])).toBeNull();
});
```

- [ ] **Step 2: Run to verify it fails** — `pnpm vitest run` → module not found.
- [ ] **Step 3: Implement `lib/layout.ts`, `lib/api.ts` and the pages.**
- [ ] **Step 4: Run** — `pnpm vitest run && pnpm tsc --noEmit && pnpm tauri build --debug` → pass.
- [ ] **Step 5: Manual (owner)** — attach iPad Right and S24+ Left in the Layout tab, save, cross both edges; change iPad gain and feel the difference without restarting; Diagnostics shows ~15 ms interval for the iPad.
- [ ] **Step 6: Commit** — `git commit -m "feat(apps/tauri): settings UI for devices, layout, calibration, diagnostics"`

---

### Task 8: Acceptance on devices + CI

**Files:**
- Create: `docs/v1/checklist-input.md` (filled in during Tasks 5–7)
- Modify: `.github/workflows/core.yml` (add `cargo test -p ctp-windows`, `cargo clippy --workspace --exclude ble-hid --exclude ipad-clip -- -D warnings`, and `pnpm vitest run` in `apps/tauri`)

- [ ] **Step 1: CI** — push; `core` workflow green on `windows-latest`.
- [ ] **Step 2: Owner acceptance** — the full checklist: pairing both devices from cold boot, both edges, all hotkeys, Esc ×3, held Shift across both directions, Bluetooth-off on each device while held, reboot laptop → everything returns without action, quit from tray → Windows input normal, no keystrokes in the log file.
- [ ] **Step 3: Commit** — `git commit -m "docs(v1): input acceptance checklist"`
