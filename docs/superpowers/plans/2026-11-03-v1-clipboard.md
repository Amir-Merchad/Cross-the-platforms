# v1 Plan 2: Clipboard Sync (Windows ↔ iPad) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Copy on Windows, paste on the iPad, and the other way round, for text and images, with no app on the iPad and nothing to do by hand after a reboot of either device.

**Architecture:** A pure clipboard `Hub` in `core/` decides what goes where (content hashes stop echoes, last writer wins, pause, size limit) and a pure codec turns PNG/JPEG/TIFF into one canonical RGBA form, so a round trip through either clipboard hashes the same. `platform/windows` gets a Windows clipboard watcher (event-driven via `AddClipboardFormatListener`), an iPad link ported from `spikes/ipad-clip` (RemotePairing over Wi-Fi, USB fallback, auto-reconnect, automatic developer-disk-image mount) and a `clipsync` service thread that wires them through the Hub. The Tauri app hosts the service next to (not inside) the input engine, adds a tray toggle and a Clipboard tab.

**Tech Stack:** Rust 1.97, `idevice` `=0.1.68` (features `full`), `tokio` 1, `mdns-sd` 0.13, `arboard` 3, `image` 0.25 (`png`, `jpeg`, `tiff` only), `sha2` (already in core), `windows` 0.62, Tauri 2, React + shadcn/ui.

**Spec:** `docs/specs/2026-11-02-device-bridge-design.md` §3 (anti-goals), §7 (clipboard). Device facts and proven flows: `docs/spikes/results.md` (Spike 1). Reference code: `spikes/ipad-clip/src/main.rs` (frozen; port, don't import).

**Out of scope here:** phone ↔ Windows clipboard. It needs pairing and a transport (file-transfer plan) and the Android app; the Hub is built so the phone becomes one more endpoint (`Endpoint::Peer(id)`) then. The iPad-app clipboard fallback (§7) comes with the iPad app. Rich text (RTF/HTML) and files-on-clipboard are later (§7).

**Branch:** `v1/clipboard`, created from `v1/windows-hub-input`.

## Global Constraints

- Never log clipboard contents, file names or pairing secrets. Logs/diagnostics: lengths, kinds (`text`/`image`), image dimensions, timings, counts, device ids.
- Sync is a no-op loop guard by content: an item equal (same hash) to the last synced content is never re-sent, in either direction.
- Size limit: canonical item size (`text.len()` or `width × height × 4`) ≤ **64 MiB** (`MAX_ITEM_BYTES = 64 * 1024 * 1024`); bigger items are skipped and counted, the other side is left untouched.
- Formats v1: UTF-8 text and images. iPad image UTIs in priority order `public.png`, `public.jpeg`, `public.tiff`; images go *to* the iPad as `public.png`.
- iPad files live outside git under `%APPDATA%\CrossThePlatforms\ipad\`: `rp-pairing.plist` (RemotePairing record), `ddi\Image.dmg`, `ddi\Image.dmg.trustcache`, `ddi\BuildManifest.plist`. RemotePairing host name `cross-the-platforms`.
- Reconnect to the iPad every **2 s** while it's unreachable (lock, sleep, Wi-Fi change, reboot); never block or slow the input engine; never touch the Windows clipboard while the iPad is down.
- Clipboard sync on by default; the toggle is persisted as `Settings.clipboard_sync`.
- Windows clipboard busy (`OpenClipboard` held by another app): retry 5 × 50 ms, then skip that item.

## Review Focus

1. **Echo/ping-pong:** writing to one side makes that side report the same content back; Windows→iPad→Windows must stop after one hop, including images that come back re-encoded. Tests in Task 1 (Hub) and Task 2 (codec round trip).
2. **Windows clipboard busy or holding non-syncable data** (Explorer file copy, RTF-only, empty) → nothing sent, nothing on the iPad cleared, no crash. Test in Task 3.
3. **Item over the size limit** (huge screenshot, giant text) → skipped, counter up, Windows/iPad clipboards untouched. Test in Task 1.
4. **iPad unreachable or not ready** (asleep, off Wi-Fi, Developer Mode off, DDI unmounted after reboot, pairing record missing) → status names the reason, Windows clipboard keeps working, reconnects by itself. Test in Task 4 (`classify`).
5. **Sync paused, then resumed** → nothing synced while paused, nothing replayed on resume; the next real copy syncs. Test in Task 1.

---

### Task 1: `core::clipboard` — the Hub, plus the persisted toggle

**Files:**
- Create: `core/src/clipboard.rs`; modify `core/src/lib.rs` (`pub mod clipboard;`)
- Modify: `core/src/settings.rs` (add `clipboard_sync`)
- Test: `core/src/clipboard.rs` tests, `core/src/settings.rs` tests

**Interfaces:**
- Produces:
  - `pub const MAX_ITEM_BYTES: usize = 64 * 1024 * 1024;`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub struct Image { pub width: u32, pub height: u32, pub rgba: Vec<u8> }`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum Content { Text(String), Image(Image) }` with `pub fn size(&self) -> usize` and `pub fn kind(&self) -> &'static str` (`"text"`/`"image"`)
  - `#[derive(Clone, Debug, PartialEq, Eq, Hash)] pub enum Endpoint { Windows, Ipad, Peer(String) }`
  - `#[derive(Clone, Debug, PartialEq)] pub struct Item { pub id: u64, pub origin: Endpoint, pub content: Content }`
  - `pub struct Hub` with `pub fn new(endpoints: Vec<Endpoint>) -> Self`, `pub fn set_paused(&mut self, p: bool)`, `pub fn observe(&mut self, from: Endpoint, content: Content) -> Vec<(Endpoint, Item)>`, `pub fn skipped_too_large(&self) -> u64`
  - `Settings.clipboard_sync: bool` (default `true`)

- [ ] **Step 1: Write the failing tests** (helper `fn hub() -> Hub { Hub::new(vec![Endpoint::Windows, Endpoint::Ipad]) }`, `fn t(s: &str) -> Content { Content::Text(s.into()) }`)

```rust
#[test] fn windows_copy_goes_to_ipad_only() {
    let out = hub().observe(Endpoint::Windows, t("hi"));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, Endpoint::Ipad);
    assert_eq!(out[0].1.origin, Endpoint::Windows);
    assert_eq!(out[0].1.content, t("hi"));
}
#[test] fn echo_from_target_is_dropped() { // Review Focus 1
    let mut h = hub();
    h.observe(Endpoint::Windows, t("hi"));
    assert!(h.observe(Endpoint::Ipad, t("hi")).is_empty());
}
#[test] fn same_content_twice_is_sent_once() {
    let mut h = hub();
    h.observe(Endpoint::Windows, t("a"));
    assert!(h.observe(Endpoint::Windows, t("a")).is_empty());
}
#[test] fn last_writer_wins_and_a_can_come_back() {
    let mut h = hub();
    h.observe(Endpoint::Windows, t("a"));
    assert_eq!(h.observe(Endpoint::Ipad, t("b"))[0].0, Endpoint::Windows);
    assert_eq!(h.observe(Endpoint::Windows, t("a"))[0].0, Endpoint::Ipad, "a is new again after b");
}
#[test] fn ids_increase() {
    let mut h = hub();
    let a = h.observe(Endpoint::Windows, t("a"))[0].1.id;
    let b = h.observe(Endpoint::Windows, t("b"))[0].1.id;
    assert!(b > a);
}
#[test] fn empty_text_is_ignored() { assert!(hub().observe(Endpoint::Windows, t("")).is_empty()); }
#[test] fn too_large_is_skipped_and_counted() { // Review Focus 3
    let mut h = hub();
    let big = Content::Image(Image { width: 4097, height: 4097, rgba: vec![0; 4097 * 4097 * 4] });
    assert!(h.observe(Endpoint::Windows, big).is_empty());
    assert_eq!(h.skipped_too_large(), 1);
}
#[test] fn paused_syncs_nothing_and_resume_replays_nothing() { // Review Focus 5
    let mut h = hub();
    h.set_paused(true);
    assert!(h.observe(Endpoint::Windows, t("secret")).is_empty());
    h.set_paused(false);
    assert!(h.observe(Endpoint::Ipad, t("secret")).is_empty(), "copied while paused counts as seen");
    assert_eq!(h.observe(Endpoint::Windows, t("next")).len(), 1);
}
#[test] fn three_endpoints_fan_out_to_both_others() {
    let mut h = Hub::new(vec![Endpoint::Windows, Endpoint::Ipad, Endpoint::Peer("s24".into())]);
    let to: Vec<_> = h.observe(Endpoint::Ipad, t("x")).into_iter().map(|(e, _)| e).collect();
    assert_eq!(to, vec![Endpoint::Windows, Endpoint::Peer("s24".into())]);
}
```

In `settings.rs` tests: `#[test] fn clipboard_sync_defaults_on() { assert!(Settings::default().clipboard_sync); }` and extend `older_file_missing_fields_keeps_known_ones` with `assert!(s.clipboard_sync);`.

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core clipboard settings` → compile errors.
- [ ] **Step 3: Implement `clipboard.rs` and the settings field.** Hub keeps `last: Option<[u8; 32]>` = SHA-256 of (`kind` byte + `width`/`height` LE for images + data) and a `next_id: u64`. `observe`: empty text → nothing; hash == last → nothing; set `last = hash` (also while paused, so resume replays nothing); paused → nothing; over `MAX_ITEM_BYTES` → count, nothing; else one `Item` per endpoint ≠ `from`, in `new()` order.
- [ ] **Step 4: Run tests + clippy** — `cargo test -p bridge-core && cargo clippy -p bridge-core -- -D warnings` → pass, no warnings.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): clipboard hub (echo guard, last writer wins, pause, size limit)"`

---

### Task 2: `core::clipboard::codec` — canonical images

**Files:**
- Modify: `core/src/clipboard.rs` (add `pub mod codec` inline or `core/src/clipboard/codec.rs`), `core/Cargo.toml` (`image = { version = "0.25", default-features = false, features = ["png", "jpeg", "tiff"] }`)
- Test: codec tests

**Interfaces:**
- Consumes: `Image` (Task 1)
- Produces: `pub fn encode_png(img: &Image) -> Result<Vec<u8>, String>`, `pub fn decode(bytes: &[u8]) -> Result<Image, String>` (any of PNG/JPEG/TIFF, output RGBA8)

- [ ] **Step 1: Write the failing tests**

```rust
fn sample() -> Image { Image { width: 3, height: 2, rgba: (0..24).map(|i| (i * 10) as u8).collect() } }
#[test] fn png_round_trip_is_identical() { // Review Focus 1: echo of our own PNG hashes the same
    assert_eq!(decode(&encode_png(&sample()).unwrap()).unwrap(), sample());
}
#[test] fn decodes_jpeg_to_rgba() {
    let mut jpg = Vec::new();
    image::RgbImage::new(4, 4).write_to(&mut std::io::Cursor::new(&mut jpg), image::ImageFormat::Jpeg).unwrap();
    let img = decode(&jpg).unwrap();
    assert_eq!((img.width, img.height, img.rgba.len()), (4, 4, 64));
}
#[test] fn garbage_is_an_error_not_a_panic() { assert!(decode(b"not an image").is_err()); }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p bridge-core codec` → compile errors.
- [ ] **Step 3: Implement** with `image::load_from_memory(..).to_rgba8()` and `RgbaImage::from_raw` + `write_to(.., ImageFormat::Png)`.
- [ ] **Step 4: Run tests + clippy** — pass. Also `cargo build -p bridge-core --target aarch64-linux-android` is *not* required here; CI covers mobile later.
- [ ] **Step 5: Commit** — `git commit -m "feat(core): canonical RGBA clipboard images (PNG/JPEG/TIFF)"`

---

### Task 3: `platform/windows::clipboard` — Windows clipboard watch and write

**Files:**
- Create: `platform/windows/src/clipboard.rs`; modify `platform/windows/src/lib.rs`, `platform/windows/Cargo.toml` (`arboard = "3"`)
- Test: `platform/windows/src/clipboard.rs` tests

**Interfaces:**
- Consumes: `bridge_core::clipboard::{Content, Image}`
- Produces:
  - `pub fn read() -> Option<Content>` — text if present (non-empty), else image, else `None` (files, RTF-only, empty)
  - `pub fn write(c: &Content) -> Result<(), String>`
  - `pub fn watch(tx: std::sync::mpsc::Sender<Content>) -> WatchHandle` — own thread, message-only window, `AddClipboardFormatListener`; on `WM_CLIPBOARDUPDATE` sends `read()` when `Some`. `WatchHandle::stop(&mut self)` + `Drop` (post `WM_QUIT`, join), same shape as `capture::CaptureHandle`.
  - `fn with_retry<T>(attempts: u32, delay: Duration, f: impl FnMut() -> Result<T, arboard::Error>) -> Result<T, arboard::Error>` — retries only `arboard::Error::ClipboardOccupied`.

- [ ] **Step 1: Write the failing tests** (`#[cfg(test)]`; these touch the real clipboard, so mark `#[ignore]` for CI and run locally with `--ignored`, except `retry_*`)

```rust
#[test] fn retry_gives_up_after_attempts() { // Review Focus 2
    let mut n = 0;
    let r: Result<(), _> = with_retry(5, Duration::ZERO, || { n += 1; Err(arboard::Error::ClipboardOccupied) });
    assert!(r.is_err()); assert_eq!(n, 5);
}
#[test] fn retry_stops_on_other_errors() {
    let mut n = 0;
    let _ = with_retry(5, Duration::ZERO, || { n += 1; Err::<(), _>(arboard::Error::ContentNotAvailable) });
    assert_eq!(n, 1);
}
#[test] #[ignore] fn text_round_trip() {
    write(&Content::Text("héllo 👋 مرحبا".into())).unwrap();
    assert_eq!(read(), Some(Content::Text("héllo 👋 مرحبا".into())));
}
#[test] #[ignore] fn image_round_trip_is_identical() { // Review Focus 1: Windows echo of an iPad image
    let img = Image { width: 2, height: 2, rgba: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 255] };
    write(&Content::Image(img.clone())).unwrap();
    assert_eq!(read(), Some(Content::Image(img)));
}
#[test] #[ignore] fn empty_clipboard_reads_none() { arboard::Clipboard::new().unwrap().clear().unwrap(); assert_eq!(read(), None); }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p ctp-windows clipboard -- --include-ignored` → compile errors.
- [ ] **Step 3: Implement.** `read`/`write` via `arboard` inside `with_retry(5, 50 ms, ..)`. Images with alpha must round-trip exactly; if `image_round_trip_is_identical` fails because Windows premultiplies or drops alpha, normalise in `read()` (force alpha 255 when the source format has none) and note it in the test.
- [ ] **Step 4: Run** — `cargo test -p ctp-windows -- --include-ignored && cargo clippy -p ctp-windows -- -D warnings` → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(platform/windows): clipboard watch/read/write"`

---

### Task 4: `platform/windows::ipad` — iPad pasteboard link

**Files:**
- Create: `platform/windows/src/ipad.rs`; modify `lib.rs`, `Cargo.toml` (`idevice = { version = "=0.1.68", features = ["full"] }`, `tokio = { version = "1", features = ["full"] }`, `mdns-sd = "0.13"`)
- Test: `classify` tests in `ipad.rs`

**Interfaces:**
- Consumes: `Content`, `codec::{decode, encode_png}`
- Produces:
  - `pub fn ipad_dir() -> PathBuf` → `%APPDATA%\CrossThePlatforms\ipad`
  - `#[derive(Clone, Debug, PartialEq, Serialize)] pub enum IpadStatus { NotPaired, Connecting, Connected { wifi: bool }, Unavailable(String) }`
  - `pub fn classify(err: &str) -> IpadStatus` — maps a connect/session error string to a user-facing reason
  - `pub struct IpadLink` with `pub async fn connect(dir: &Path) -> Result<IpadLink, String>` (Wi-Fi via `rp-pairing.plist` + mDNS, else USB), `pub fn is_wifi(&self) -> bool`, `pub async fn next_change(&mut self) -> Result<Content, String>`, `pub async fn write(&mut self, c: &Content) -> Result<(), String>`
  - `pub async fn pair_usb(dir: &Path) -> Result<(), String>` — writes `rp-pairing.plist`

- [ ] **Step 1: Write the failing tests** (Review Focus 4)

```rust
#[test] fn missing_record_means_not_paired() { assert_eq!(classify("pairing record: NotFound"), IpadStatus::NotPaired); }
#[test] fn service_not_found_means_ddi() {
    assert_eq!(classify("pasteboard (watch): ServiceNotFound"), IpadStatus::Unavailable("Developer disk image not mounted yet".into()));
}
#[test] fn not_on_network_is_explained() {
    assert!(matches!(classify("iPad's _remotepairing._tcp not found on the network"), IpadStatus::Unavailable(m) if m.contains("Wi-Fi")));
}
#[test] fn developer_mode_off_is_explained() {
    assert!(matches!(classify("core device proxy (Developer Mode on? DDI mounted?): InvalidService"), IpadStatus::Unavailable(m) if m.contains("Developer Mode")));
}
#[test] fn anything_else_is_passed_through_short() { assert!(matches!(classify("weird"), IpadStatus::Unavailable(_))); }
```

- [ ] **Step 2: Run to verify they fail** — `cargo test -p ctp-windows ipad` → compile errors.
- [ ] **Step 3: Implement.** Port `connect_usb`, `discover`, `connect_wifi`, `rp_pair_usb` and the watcher/setter pair from the spike (record path from `dir`). `next_change`: `recv_push`, then text (`snap.text()`, non-empty) else image: first promised item with UTI in `["public.png", "public.jpeg", "public.tiff"]`, `resolve(GENERAL_PASTEBOARD, item_index, uti)` on the setter client, `decode`; skip pushes with neither. `write`: text → `set_text`; image → `set_image(&encode_png(img)?, "public.png", ..)`. Never put content in an error string.
- [ ] **Step 4: Manual (owner, iPad on Wi-Fi, record from `C:\dev\_ref\pairing\ipad-rp.plist` copied to `ipad_dir()\rp-pairing.plist`)** — a scratch `#[tokio::test] #[ignore]` that connects, writes `"ctp test"` and waits for one change → iPad Notes paste shows `ctp test`; copying on the iPad ends the test with `Content::Text`.
- [ ] **Step 5: Run tests + clippy** — pass.
- [ ] **Step 6: Commit** — `git commit -m "feat(platform/windows): iPad pasteboard link (Wi-Fi RemotePairing, USB fallback)"`

---

### Task 5: Developer disk image auto-mount

**Files:**
- Modify: `platform/windows/src/ipad.rs`
- Test: `ddi_files` test in `ipad.rs`

**Interfaces:**
- Consumes: `IpadLink::connect` internals (adapter + `RsdHandshake`, or USB provider)
- Produces: `pub fn ddi_files(dir: &Path) -> Result<DdiFiles, String>` (`DdiFiles { image: PathBuf, trust_cache: PathBuf, manifest: PathBuf }`; error names the missing file); `connect` mounts the DDI itself when the pasteboard service is missing, then retries once.

- [ ] **Step 1: Write the failing test**

```rust
#[test] fn ddi_files_names_what_is_missing() {
    let d = tempfile::tempdir().unwrap();
    std::fs::create_dir(d.path().join("ddi")).unwrap();
    std::fs::write(d.path().join("ddi/Image.dmg"), b"x").unwrap();
    let e = ddi_files(d.path()).unwrap_err();
    assert!(e.contains("Image.dmg.trustcache"), "{e}");
}
```
(`tempfile` as a dev-dependency of `ctp-windows`.)

- [ ] **Step 2: Run to verify it fails** — `cargo test -p ctp-windows ddi` → compile error.
- [ ] **Step 3: Implement.** On `ServiceNotFound` for the pasteboard: read the three files, get `UniqueChipID` (RSD handshake properties over Wi-Fi; lockdown `get_value("UniqueChipID")` over USB), then `ImageMounter` `mount_personalized_rsd` (Wi-Fi) or `mount_personalized` (USB), then reconnect the pasteboard. If mounting over Wi-Fi is refused, status becomes `Unavailable("Plug the iPad in once by USB to finish setup after its reboot")` and the USB path mounts it as soon as it's plugged in.
- [ ] **Step 4: Manual (owner)** — copy `C:\dev\_ref\ddi\*` to `ipad_dir()\ddi\`; reboot the iPad, unlock it once; within ~10 s the link is `Connected` without touching the PC (or, if Wi-Fi mount is refused, after plugging USB in once). Record which in `docs/v1/checklist-clipboard.md`.
- [ ] **Step 5: Run tests + clippy** — pass.
- [ ] **Step 6: Commit** — `git commit -m "feat(platform/windows): mount the developer disk image automatically"`

---

### Task 6: `platform/windows::clipsync` — the service

**Files:**
- Create: `platform/windows/src/clipsync.rs`; modify `lib.rs`
- Test: none beyond build (logic is in the Hub); manual in Task 8

**Interfaces:**
- Consumes: `Hub`, `Endpoint`, `clipboard::{watch, write}`, `ipad::{IpadLink, IpadStatus, classify, ipad_dir, pair_usb}`, `diag::log`
- Produces:
  - `pub enum ClipCommand { SetEnabled(bool), PairIpad, Stop }`
  - `#[derive(Clone, Debug, Default, PartialEq, Serialize)] pub struct ClipStatus { pub enabled: bool, pub ipad: Option<IpadStatus>, pub synced: u64, pub skipped_too_large: u64, pub notice: Option<String> }`
  - `pub fn start(enabled: bool, on_status: impl Fn(ClipStatus) + Send + 'static) -> ClipHandle`; `ClipHandle::send(&self, ClipCommand)`; `Drop` sends `Stop` and joins.

- [ ] **Step 1: Implement.** One OS thread running a current-thread tokio runtime. Windows changes arrive from `clipboard::watch` over std mpsc, bridged into tokio with `spawn_blocking`/`tokio::sync::mpsc`. Loop: hold an `Option<IpadLink>`; while `None`, try `IpadLink::connect(&ipad_dir())` every 2 s and set `ipad` status via `classify`; `select!` over Windows changes, `link.next_change()`, commands. Every change → `hub.observe(from, content)` → for each `(Endpoint::Windows, item)` call `clipboard::write` (via `spawn_blocking`), for `(Endpoint::Ipad, item)` call `link.write` (if the link is down, drop the item: no queue, last writer wins later). Any link error → `link = None`, status via `classify`. Log `"{kind} {from:?} -> {to:?} ({size} bytes)"` only. `PairIpad` → `pair_usb`, notice "iPad paired" or the error. `SetEnabled(b)` → `hub.set_paused(!b)`.
- [ ] **Step 2: Build + clippy** — `cargo build -p ctp-windows && cargo clippy -p ctp-windows -- -D warnings` → pass.
- [ ] **Step 3: Commit** — `git commit -m "feat(platform/windows): clipboard sync service"`

---

### Task 7: App — host the service, tray toggle, Clipboard tab

**Files:**
- Modify: `apps/tauri/src-tauri/src/main.rs` (start `clipsync` in `setup`, `AppState.clip: Mutex<Option<ClipHandle>>`, `AppState.clip_status: Mutex<ClipStatus>`, emit `"clip-status"`; drop it on Quit after the engine)
- Modify: `apps/tauri/src-tauri/src/commands.rs` (`get_clip_status`, `pair_ipad`; `save_settings` also sends `ClipCommand::SetEnabled(settings.clipboard_sync)`)
- Modify: `apps/tauri/src-tauri/src/tray.rs` (check item "Clipboard sync" toggling `settings.clipboard_sync`, saved like the UI does)
- Create: `apps/tauri/src/pages/Clipboard.tsx`; modify `apps/tauri/src/lib/api.ts`, `apps/tauri/src/App.tsx` (tab `clipboard` after `devices`)
- Test: `apps/tauri/src/lib/clip.test.ts`

**Interfaces:**
- Consumes: `clipsync::{start, ClipHandle, ClipCommand, ClipStatus}`, `ipad::IpadStatus`
- Produces (TS): `interface ClipStatus { enabled: boolean; ipad: IpadStatus | null; synced: number; skipped_too_large: number; notice: string | null }`, `type IpadStatus = "NotPaired" | "Connecting" | { Connected: { wifi: boolean } } | { Unavailable: string }`, `getClipStatus()`, `pairIpad()`, `onClipStatus(cb)`, `Settings.clipboard_sync: boolean`, and `export function ipadLine(s: IpadStatus | null): string` in `lib/clip.ts`.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from "vitest";
import { ipadLine } from "./clip";
describe("ipadLine", () => {
  it("connected over Wi-Fi", () => expect(ipadLine({ Connected: { wifi: true } })).toBe("Connected over Wi-Fi"));
  it("connected over USB", () => expect(ipadLine({ Connected: { wifi: false } })).toBe("Connected over USB"));
  it("not paired", () => expect(ipadLine("NotPaired")).toBe("Not paired — plug the iPad in by USB and press Pair"));
  it("unavailable shows the reason", () => expect(ipadLine({ Unavailable: "Developer disk image not mounted yet" })).toBe("Developer disk image not mounted yet"));
  it("no status yet", () => expect(ipadLine(null)).toBe("Starting…"));
});
```

- [ ] **Step 2: Run to verify it fails** — `cd apps/tauri && pnpm vitest run` → module not found.
- [ ] **Step 3: Implement** `lib/clip.ts`, the API wrappers and the tab: a Switch "Sync clipboard" bound to `clipboard_sync` (saved immediately via `saveSettings`), the `ipadLine` status with a Badge, "Pair iPad (USB)" Button calling `pairIpad`, counters "Synced N · Skipped (too large) M", and an Alert with `notice`. Tray: check item text "Clipboard sync", checked = `settings.clipboard_sync`.
- [ ] **Step 4: Run** — `pnpm vitest run && npx tsc --noEmit && pnpm tauri build --debug` → pass.
- [ ] **Step 5: Commit** — `git commit -m "feat(apps/tauri): clipboard sync toggle, iPad status and pairing"`

---

### Task 8: Acceptance on devices

**Files:**
- Create: `docs/v1/checklist-clipboard.md`

- [ ] **Step 1: Write the checklist** with these rows (Result column `PENDING`): Windows text → iPad Notes (incl. `héllo 👋 مرحبا`), < 1 s · iPad text → Windows Notepad, < 1 s · Windows screenshot (Win+Shift+S) → iPad paste in Notes · iPad screenshot copy → Windows paste in Paint · copy the same thing twice, no flicker/loop (watch counters: +1 each) · Explorer file copy on Windows → iPad clipboard unchanged · toggle off → nothing syncs either way; toggle on → next copy syncs, nothing replayed · iPad locked → unlock → sync works within ~5 s · iPad off Wi-Fi → status says so → back on → reconnects · iPad reboot → unlock once → DDI remounted, sync back (note if USB was needed) · laptop reboot → sync works without opening the app · `engine.log` contains no clipboard text.
- [ ] **Step 2: Owner runs it** with `target\debug\ctp-app.exe` (quit from the tray before rebuilds; never force-kill) and records results.
- [ ] **Step 3: Commit** — `git commit -m "docs(v1): clipboard acceptance checklist"`
