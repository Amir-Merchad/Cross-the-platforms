# v1 Plan 3: File Transfer + Android App — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pair the S24+ once by QR, then send files both ways (Windows hub ↔ phone) faster than LocalSend on the same Wi-Fi, and sync the phone's clipboard with Windows — all from one native Android app and the existing Windows tray app.

**Architecture:** A `Node` in `core/` owns a persistent iroh identity and a list of paired peers. Pairing and control messages (offers, clipboard text) travel over authenticated iroh connections that only paired peers may open. File bytes move over a direct TLS-over-TCP stream on the LAN, negotiated over the control connection (the receiver listens, sends its addresses and certificate fingerprint; the sender pins that fingerprint); if TCP can't connect within 3 s the bytes go over iroh instead. Windows hosts the node in the Tauri app; Android hosts it in a foreground service through UniFFI.

**Tech Stack:** Rust (`iroh` 1.4, `tokio`, `rustls` 0.23 + `tokio-rustls` 0.26, `rcgen` 0.14, `sha2`, `serde_json`, `uniffi` 0.32), Tauri 2 (+ `tauri-plugin-dialog`), React/shadcn, Kotlin + Jetpack Compose (minSdk 31, targetSdk 36), `zxing-android-embedded` for QR scanning.

**Spec:** `docs/specs/2026-11-02-device-bridge-design.md` §8 (file transfer), §7 (phone clipboard), §9 (platform limits); measured facts in `docs/spikes/results.md` (iroh 40–44 MB/s phone↔PC vs LocalSend 90 MB/s; tile flow accepted by the owner).

## Global Constraints

- Only paired peers may open control or bulk connections; every inbound connection is checked against the paired list by iroh endpoint id (public key). No LAN-only assumptions in auth (v2 uses the same keys over the internet).
- Pairing: one-time code, valid 5 minutes, single use; QR shown on Windows, scanned by the phone app.
- Received files: phone `Download/Bridge`, Windows `%USERPROFILE%\Downloads\Bridge` (configurable). Hostile/duplicate names via existing `names::safe_name`; data goes to `.part` and is renamed only after size **and** SHA-256 match.
- Speed target on the owner's Wi-Fi: ≥ 80 MB/s PC↔phone for a 1 GB file (LocalSend: 90 MB/s).
- Never log file names, clipboard text or file contents; sizes, counts, timings and peer ids only.
- Android: foreground service (`dataSync`) keeps the node alive; clipboard leaves the phone only through the Quick Settings tile or the share sheet (Android forbids background clipboard reads — owner accepted the tile).
- Machine gotchas from `docs/HANDOFF.md` apply (Gradle `TMP=C:\dev\_ref\tmp`, JDK = Android Studio jbr, `npx` not `pnpm dlx`, never force-kill the app).

## Review Focus

1. **An unpaired device connects** (anyone on the café Wi-Fi with the app) and sends an offer or clipboard text → rejected before anything is read or written. Test in Task 3.
2. **The pairing QR is reused or photographed later** → code works once, within 5 minutes; afterwards "expired". Test in Task 2.
3. **Wi-Fi drops or the phone sleeps mid-transfer** → no complete-looking file on the receiver, sender reports the failure. Test in Task 4.
4. **Windows Firewall blocks the TCP listener** (first-run prompt dismissed) or the devices are on different subnets → the transfer still completes over iroh within 3 s of the failed connect. Test in Task 5.
5. **Two files with the same name in one send, or a name that already exists** → `name (1).ext`, both kept, nothing outside the receive folder. Test in Task 4.

---

### Task 1: `core::identity` — persistent key and paired peers

**Files:** Create `core/src/identity.rs`; modify `core/src/lib.rs`. Test in the same file.

**Interfaces:**
- Produces:
  - `pub fn load_or_create_key(path: &Path) -> anyhow::Result<iroh::SecretKey>` — 32 raw bytes on disk; creates parent dirs; unreadable/short file → error (never silently regenerate: that would unpair every device)
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] pub struct Peer { pub id: String, pub name: String, pub kind: PeerKind }` with `pub enum PeerKind { Windows, Android, Ipad }` (`id` = iroh `EndpointId` as string)
  - `pub struct PairedPeers` with `load(path) -> Self` (missing → empty), `save(&self, path) -> Result<(), String>` (tmp + rename), `add(Peer)` (replaces same id), `remove(id)`, `contains(id) -> bool`, `list() -> &[Peer]`

- [ ] **Step 1: Failing tests** — `key_persists_across_loads` (same public key twice), `short_key_file_is_an_error`, `peers_round_trip`, `add_replaces_same_id`, `missing_peers_file_is_empty`.
- [ ] **Step 2: Run** `cargo test -p bridge-core identity` → compile errors.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run tests + clippy** → pass.
- [ ] **Step 5: Commit** `feat(core): persistent identity and paired peers`

---

### Task 2: `core::pairing` — QR offer, one-time code, pair handshake

**Files:** Create `core/src/pairing.rs`. Test: unit tests + `core/tests/pairing.rs`.

**Interfaces:**
- Consumes: Task 1.
- Produces:
  - `pub const PAIR_ALPN: &[u8] = b"ctp/pair/1";`
  - `#[derive(Serialize, Deserialize, PartialEq, Debug)] pub struct PairInvite { pub v: u32, pub name: String, pub addr: iroh::EndpointAddr, pub code: String }` with `to_qr_text(&self) -> String` (prefix `ctp-pair:` + JSON) and `parse(text) -> Result<Self, String>` (rejects other prefixes / `v != 1`)
  - `pub struct PendingCode` with `new(now: Instant) -> Self` (16 random bytes, hex), `check(&mut self, code: &str, now: Instant) -> Result<(), PairError>`; `pub enum PairError { Wrong, Expired, Used }` — valid 5 min, single use
  - Wire (JSON lines on one bi-stream): joiner → `{"code":…, "name":…, "kind":…}`; host → `{"ok":true,"name":…,"kind":…}` or `{"ok":false,"error":"wrong|expired|used"}`. Both sides add the other to `PairedPeers` only on `ok`.
  - Server side lives in Task 3's `Node::accept_pair`; this task provides `pub async fn join(ep: &iroh::Endpoint, invite: &PairInvite, me: &Peer) -> Result<Peer, String>` (returns host as `Peer` with the id from the connection, not from the invite JSON) and `pub async fn answer(conn, pending: &mut PendingCode, me: &Peer, now) -> Result<Peer, PairError>`.

- [ ] **Step 1: Failing tests** — `code_works_once` (second `check` → `Used`), `code_expires_after_5_min` (`now + 301 s` → `Expired`), `wrong_code_rejected`, `qr_round_trip`, `foreign_qr_rejected` (`"https://…"` → error), integration `pair_two_endpoints_locally` (both sides end with the other's real endpoint id).
- [ ] **Step 2: Run** → compile errors.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run tests + clippy** → pass.
- [ ] **Step 5: Commit** `feat(core): QR pairing with one-time codes`

---

### Task 3: `core::node` — endpoint, paired-only control channel, clipboard messages

**Files:** Create `core/src/node.rs`. Test: `core/tests/node.rs`.

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:
  - `pub const CTL_ALPN: &[u8] = b"ctp/ctl/1";`
  - `#[derive(Serialize, Deserialize, Debug, PartialEq)] #[serde(tag = "t")] pub enum Ctl { Offer { id: u64, files: Vec<FileMeta> }, Accept { id: u64, tcp: Vec<std::net::SocketAddr>, cert_sha256: String }, Reject { id: u64, reason: String }, Clip { text: String } }`; `pub struct FileMeta { pub name: String, pub size: u64 }`
  - `pub enum Event { Paired(Peer), Incoming { from: String, files: Vec<FileMeta> }, Progress { id: u64, done: u64, total: u64 }, Received { from: String, path: PathBuf, size: u64 }, Failed { id: u64, reason: String }, Clip { from: String, text: String } }`
  - `pub struct Node` with `pub async fn start(cfg: NodeConfig) -> anyhow::Result<(Node, tokio::sync::mpsc::Receiver<Event>)>`; `pub struct NodeConfig { pub data_dir: PathBuf, pub receive_dir: PathBuf, pub me: Peer }` (key at `data_dir/identity.key`, peers at `data_dir/peers.json`)
  - `Node::invite(&self) -> PairInvite` (fresh 5-min code), `Node::join(&self, invite: &PairInvite) -> Result<Peer, String>`, `Node::peers(&self) -> Vec<Peer>`, `Node::forget(&self, id)`, `Node::send_clip(&self, peer_id, text) -> Result<(), String>`
  - Accept loop: `PAIR_ALPN` only while an invite is open; `CTL_ALPN` only from `PairedPeers::contains(remote_id)` — others closed immediately with reason `"not paired"`.

- [ ] **Step 1: Failing tests** — `unpaired_peer_is_rejected` (Review Focus 1: a fresh endpoint opening `CTL_ALPN` gets closed and no `Event` is produced), `clip_round_trip_between_paired_nodes`, `pair_then_restart_keeps_pairing` (restart both nodes from the same `data_dir`, `send_clip` still works).
- [ ] **Step 2: Run** → compile errors.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run tests + clippy** → pass.
- [ ] **Step 5: Commit** `feat(core): node with paired-only control channel`

---

### Task 4: `core::bulk` — TLS-over-TCP file stream

**Files:** Create `core/src/bulk.rs`; add `rustls`, `tokio-rustls`, `rcgen` to `core/Cargo.toml`. Test: unit + `core/tests/bulk.rs`.

**Interfaces:**
- Consumes: `names::safe_name`.
- Produces:
  - `pub struct BulkListener` with `pub async fn bind() -> Result<Self>` (all LAN interfaces, ephemeral port, fresh self-signed cert per listener), `pub fn addrs(&self) -> Vec<SocketAddr>` (non-loopback IPv4 first), `pub fn cert_sha256(&self) -> String`, `pub async fn receive(self, expected: &[FileMeta], dir: &Path, progress: impl FnMut(u64, u64)) -> Result<Vec<PathBuf>>` (accepts exactly one connection, then closes)
  - `pub async fn send(addrs: &[SocketAddr], cert_sha256: &str, files: &[PathBuf], progress: impl FnMut(u64, u64)) -> Result<u64>` — connects to the first address that answers within `CONNECT_TIMEOUT = 3 s`; TLS verifier accepts only the pinned SHA-256 of the leaf cert
  - Framing per file: `u64 LE size` + bytes; after each file the receiver answers 32 raw SHA-256 bytes; sender compares; receiver writes to `.part`, renames only after size and hash match. 1 MiB buffers.

- [ ] **Step 1: Failing tests** — `two_files_round_trip`, `wrong_fingerprint_is_refused` (sender errors, receiver writes nothing), `cut_off_leaves_no_file` (Review Focus 3: drop the sender after 1 MiB of a 20 MiB file → no final file, no `.part`), `duplicate_names_both_kept` (Review Focus 5: `a.pdf` twice → `a.pdf`, `a (1).pdf`), `hostile_name_stays_inside`, ignored `loopback_throughput` printing MB/s for 1 GiB.
- [ ] **Step 2: Run** → compile errors.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run tests + clippy**; run `cargo test -p bridge-core --release loopback_throughput -- --ignored --nocapture` and record MB/s in the ledger (iroh loopback was 42–64 MB/s; expect well above).
- [ ] **Step 5: Commit** `feat(core): TLS-over-TCP bulk file stream`

---

### Task 5: `Node::send_files` — offer, accept, bulk with iroh fallback

**Files:** Modify `core/src/node.rs`. Test: `core/tests/node.rs`.

**Interfaces:**
- Consumes: Tasks 3–4, existing `transfer` stream code for the fallback.
- Produces:
  - `Node::send_files(&self, peer_id: &str, paths: Vec<PathBuf>) -> Result<u64, String>` — sends `Ctl::Offer`; receiver auto-accepts from paired peers (binds `BulkListener`, replies `Accept`), emits `Incoming`, then `Progress`/`Received`; sender runs `bulk::send`; on connect failure falls back to an iroh bi-stream with the same framing.
  - `Event::Progress` at most every 100 ms per transfer.

- [ ] **Step 1: Failing tests** — `send_files_end_to_end` (2 files, both `Received` events, bytes identical), `falls_back_to_iroh_when_tcp_unreachable` (Review Focus 4: test hook makes `Accept` carry an unroutable address `10.255.255.1:9` → completes, total time < 8 s), `send_to_unpaired_peer_fails_cleanly`.
- [ ] **Step 2–4:** run → fail; implement; run → pass + clippy.
- [ ] **Step 5: Commit** `feat(core): send files with LAN TLS stream and iroh fallback`

---

### Task 6: UniFFI surface for the Android app

**Files:** Replace `core/src/ffi.rs` (old spike exports go). Test: build + `uniffi-bindgen` generation.

**Interfaces:**
- Consumes: Tasks 3–5.
- Produces (Kotlin names via UniFFI):
  - `#[uniffi::export(callback_interface)] pub trait NodeEvents: Send + Sync { fn on_event(&self, e: FfiEvent); }` with `FfiEvent` mirroring `Event` (`PathBuf` → `String`)
  - `start_node(data_dir: String, receive_dir: String, name: String, events: Box<dyn NodeEvents>) -> Result<Arc<FfiNode>, FfiError>`
  - `FfiNode::join(qr_text: String) -> Result<FfiPeer, FfiError>`, `peers() -> Vec<FfiPeer>`, `forget(id)`, `send_files(peer_id, paths: Vec<String>) -> Result<u64, FfiError>`, `send_clip(peer_id, text) -> Result<(), FfiError>`
- [ ] **Step 1:** `cargo build -p bridge-core` and generate Kotlin bindings (`cargo run -p uniffi-bindgen -- generate --library target/debug/bridge_core.dll --language kotlin --out-dir apps/android/app/src/main/java --no-format`) → succeeds.
- [ ] **Step 2: Commit** `feat(core): UniFFI node API for mobile apps`

---

### Task 7: Windows — transfer service, pairing, Send To, Transfer tab

**Files:** Create `platform/windows/src/transfer.rs` (owns the `Node` on a tokio runtime thread, bridges `Event`s), modify `platform/windows/src/clipsync.rs` (phone endpoint: `Event::Clip` → hub → Windows clipboard; Windows clipboard changes → `send_clip` to paired phones, text only), `apps/tauri/src-tauri/src/{main.rs, commands.rs}`, add `tauri-plugin-dialog`; create `apps/tauri/src/pages/Transfer.tsx`, `apps/tauri/src/lib/transfer.ts`. Test: `apps/tauri/src/lib/transfer.test.ts` (Vitest) + Rust unit test for `sendto_shortcut_args`.

**Interfaces:**
- Commands: `pair_invite() -> { qr: String, expires_in_s: u64 }`, `paired_peers() -> Vec<Peer>`, `forget_peer(id)`, `send_files(peer_id, paths: Vec<String>)`, `receive_dir() / set_receive_dir(path)`, `open_receive_dir()`; event `transfer` carrying `Event`.
- `--send-to <peer_id> <files…>` CLI args reach the running instance through `tauri-plugin-single-instance` and call `send_files`; `pub fn sendto_shortcut_args(peer_id: &str) -> String` builds the args; one `.lnk` per paired peer in `%APPDATA%\Microsoft\Windows\SendTo` named `Bridge – <name>`, refreshed on pair/forget.
- Transfer tab: paired devices with "Send files…" (dialog) and drop zone per device, QR card for pairing (rendered with the `qrcode` npm package, 5-min countdown), progress bars, recent received files (size only in logs), "Open folder".
- `lib/transfer.ts`: `formatRate(bytes, ms) -> string` and `progressPercent(done, total) -> number` with tests (`0 / 0 → 0`, never > 100).

- [ ] **Step 1: Failing tests** (Vitest + Rust) → run → fail.
- [ ] **Step 2: Implement** service, commands, Send To, UI.
- [ ] **Step 3: Run** `cargo test -p ctp-windows`, `pnpm test`, `pnpm tauri build --debug --no-bundle` → pass.
- [ ] **Step 4: Commit** `feat: Windows transfer service, pairing QR, Send To and Transfer tab`

---

### Task 8: Android app (`apps/android`, Kotlin + Compose)

**Files:** Create `apps/android/` (Gradle project modelled on `spikes/android-app`, package `dev.crosstheplatforms.app`), `apps/android/build-rust.ps1`. Kotlin files: `App.kt` (holds the node), `NodeService.kt` (foreground `dataSync` service: starts node, posts "Receiving…/Received <size>" notifications, writes incoming `Clip` with `ClipboardManager`), `MainActivity.kt` (paired devices, "Pair with PC" → ZXing scanner → `join`, send button → document picker), `ShareActivity.kt` (`ACTION_SEND`/`SEND_MULTIPLE`, any MIME → pick paired PC if more than one → `send_files`), `ClipTile.kt` + `ClipActivity.kt` (reads clipboard in foreground → `send_clip`), `BootReceiver.kt` (restart service after reboot).

**Interfaces:** Consumes Task 6 bindings. Receive dir `Download/Bridge`; shared files are copied to cache with their display names (spike `Shared.copyToCache`, moved off the main thread).

- [ ] **Step 1: Build** `powershell -File apps/android/build-rust.ps1` then `./gradlew :app:assembleDebug` (with `JAVA_HOME`, `TMP` per Global Constraints) → `BUILD SUCCESSFUL`.
- [ ] **Step 2: Install** `adb install -r app-debug.apk` → `Success`.
- [ ] **Step 3: Commit** `feat(apps/android): pairing, receive, share-to-PC, clipboard tile`

---

### Task 9: Acceptance on devices

**Files:** Create `docs/v1/checklist-transfer.md`.

| Check | Expected |
|---|---|
| Pair: Windows Transfer tab QR → phone scans | both list each other; survives reboot of either |
| Same QR again after pairing / after 5 min | "already used" / "expired" |
| 1 GB video phone → PC and PC → phone | ≥ 80 MB/s, bytes identical (compare with LocalSend 90 MB/s) |
| PDF via phone share sheet → "Bridge" | lands in `Downloads\Bridge` on Windows |
| Explorer right-click → Send to → Bridge – S24+ | lands in phone `Download/Bridge`, notification |
| Phone Wi-Fi off mid-transfer | no complete-looking file; error shown on both sides |
| Windows Firewall prompt denied | still delivers (iroh fallback), slower |
| Phone clipboard tile → Windows paste | text arrives; Windows copy → phone paste |
| Phone rebooted | service back, receives without opening the app |
| Logs | no file names or clipboard text |

- [ ] Record results; fix failures with superpowers:systematic-debugging; commit `docs(v1): transfer acceptance`.

---

**Next plans:** Plan 4 — iPad Swift app (pairing, receive into Files while open, share extension, LocalSend-protocol fallback, CI build + sideload). Plan 5 — Tauri mobile flavours.
