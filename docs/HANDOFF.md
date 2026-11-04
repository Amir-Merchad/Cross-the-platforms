# Handoff — where the work stands (2026-11-04)

Read this first. Then, as needed: `docs/specs/2026-11-02-device-bridge-design.md` (the vision; §4 apps, §7 clipboard,
§8 transfer, §9 platform limits), `docs/spikes/results.md` (measured facts), the plans in `docs/superpowers/plans/`,
and the checklists in `docs/v1/`.

## 1. Project in one paragraph
Own open-source suite so a Windows 11 laptop (hub), Galaxy S24+ and iPad Air 13" M2 work as one setup.
**v1** = keyboard/mouse from Windows to iPad + phone over BLE HID with edge switching, clipboard across all three, LAN file
transfer, and the three apps (Tauri+shadcn hub, native Kotlin Android, native Swift iPad; Rust core shared via UniFFI).
**v1.5** = cross-platform input + screen mirroring/extension. **v2** = internet transfer. Owner: Germany (German layout, AltGr
matters), no Mac, free Apple ID, uses it daily while studying. No merge / PR / push to `main` without the owner saying so.

## 2. What works today (owner-verified on devices)
| Feature | State |
|---|---|
| Keyboard + mouse → iPad and S24+ (BLE HID, edge switching, German layout) | ✅ daily use ("runs smoothly") |
| Clipboard Windows ⇄ iPad (text + images, USB or Wi-Fi, no iPad app) | ✅ `docs/v1/checklist-clipboard.md` all PASS |
| Pairing PC ⇄ S24+ by QR (one-time, 5 min) | ✅ |
| Files PC → phone ≥ 80 MB/s; phone → PC (disk-limited, see §6) | ✅ |
| Phone share sheet → PC; Explorer "Send to → Bridge – <phone>" → phone | ✅ |
| Clipboard PC → phone (automatic) and phone → PC (Quick Settings tile, 52 ms) | ✅ |
| iPad app Bridge: pair, files both ways, share sheet, queue while closed (SideStore sideload) | ✅ `docs/v1/checklist-ipad.md`, overall owner report; 7-day re-sign (row 13) still to check |
| Logs contain no file names / clipboard text / keys | ✅ |
| Base UI (Windows hub, Android, iPad: Rubik, amber, light/dark, Desk board, pages Devices / Transfers / Clipboard / Settings) | Built on `v1/ui`. **Not yet verified on devices:** Windows rows 2 to 25 of `docs/v1/checklist-ui.md` (only row 1 has a result), all Android rows, all iPad rows |

## 3. Branches (all stacked; nothing merged into `main` yet)
```
main ── v1/windows-hub-input (04c6a13, pushed) ── v1/clipboard (2b73b7b, pushed) ── v1/transfer (pushed) ── v1/ios (pushed) ── v1/ui (pushed)
```
- **`v1/ui`** is the newest and contains everything (base UI on all three apps). It lives in the worktree
  `C:\dev\Cross-the-platforms\.claude\worktrees\ctp-windows-hub-input-bug-b2c51e`. Pushed to `origin`.
- `main` = foundation + spikes (PR #1, merged by the owner).
- To finish v1 cleanly: open PRs in order `v1/windows-hub-input → main`, then `v1/clipboard`, then `v1/transfer` (each
  based on the previous), then `v1/ios`, then `v1/ui`, or one PR of `v1/ui → main` with all commits. Owner decides.

## 4. Build, run, install
Repo layout: `core/` (bridge-core: hid, edge, layout, settings, router, keyqueue, clipboard, names, identity, pairing, node,
bulk, transfer, ffi), `platform/windows/` (ctp-windows: BLE peripheral, capture, engine, clipboard, clipsync, ipad, transfer,
sendto), `apps/tauri/` (hub app), `apps/android/` (Bridge), `spikes/*` (throwaway), `tools/uniffi-bindgen`.

| What | Command (from the worktree root unless noted) |
|---|---|
| Rust tests | `cargo test -p bridge-core -p ctp-windows` (core 98 + integration 18, platform 39) |
| Web tests | `cd apps/tauri && pnpm test` (20) |
| Windows app | `cd apps/tauri && pnpm tauri build --debug --no-bundle` → `target\debug\ctp-app.exe` |
| …while the app is running | build to a side dir: `CARGO_TARGET_DIR=C:/dev/ctp-target-check pnpm tauri build --debug --no-bundle`, then quit the app from the tray and copy the exe over `target\debug\ctp-app.exe` |
| Android Rust core + bindings | `powershell -File apps/android/build-rust.ps1` (cargo-ndk, arm64; ~9 min first time) |
| Android APK | `cd apps/android`, `JAVA_HOME="C:\Program Files\Android\Android Studio\jbr"`, `TMP=TEMP=C:\dev\_ref\tmp`, `./gradlew :app:assembleDebug` |
| Install on phone | `"$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe" install -r apps/android/app/build/outputs/apk/debug/app-debug.apk` |
| Throughput check | `cargo test -p bridge-core --release --test bulk loopback_throughput -- --ignored --nocapture` (82 MB/s here, disk-bound) |

Autostart (`HKCU\...\Run\Cross the Platforms`) points at `<worktree>\target\debug\ctp-app.exe`. If you delete the worktree,
build from the new place and start it once so autostart follows.

## 5. How the transfer works (plan 3, so you can change it safely)
- **Identity:** each device has a 32-byte iroh key (`%APPDATA%\CrossThePlatforms\node\identity.key`; phone `files/node/`). A damaged
  key is an error, never regenerated (that would unpair everything). Paired list `peers.json`, address book `addrs.json`, fixed
  port `port`.
- **Pairing:** PC shows QR `ctp-pair:{json}` (relay + primary LAN IPv4 only — the full address list made it unscannable).
  Code: 128-bit, 5 min, single use; wrong guesses don't burn it. Phone proves it saw the code over iroh ALPN `ctp/pair/1`.
- **Control channel** `ctp/ctl/1`: only paired ids (checked on the authenticated connection before reading). Messages
  `Hello{addr}` (sent first every time + every 60 s — iroh 1.4 has no LAN discovery), `Offer`, `Accept{tcp, cert_sha256, token}`,
  `Reject`, `Clip{text}`.
- **Bytes:** receiver binds a one-shot TLS-over-TCP listener (fresh self-signed cert, pinned by SHA-256, plus a one-time token,
  exactly one connection). If TCP/TLS can't connect within 3 s per address → same framing over iroh ALPN `ctp/bulk/1`
  (only the offering peer may claim it). Per file: size, bytes, SHA-256; written to `<name>.part`, renamed only after the hash
  matches. Names via `names::safe_name` (no traversal, reserved names, `a (1).pdf`; a name is taken while its `.part` exists).
- **Events** (`node::Event`, serde tag `type`): Paired, Incoming, Progress (≤ every 100 ms), Received, Failed (every failure,
  also before Accept), Clip (never sent to the window).
- Android: foreground service type **connectedDevice** (dataSync is capped at 6 h/day on Android 15+ and can't start at boot).
  Clipboard leaves the phone only via the tile or the share sheet (Android forbids background clipboard reads).

## 6. Known issues / facts to remember
- **Phone → PC big files are limited by the laptop SSD, not the code:** Kingston NV2 1 TB, DRAM-less, 94 % full (51 GB free).
  Raw writes: 3 GB/s for ~0.5 GB, then 10–37 MB/s with 10–20 s stalls in any folder. Free 150–200 GB and run
  "Defragment and Optimize Drives → C: → Optimize" (TRIM); or point Transfer → Change… at a faster drive.
- **iPad clipboard after iPad/laptop restarts** can fail with `pair verify … early eof`; fix = Clipboard page → **Pair over USB**
  (done 2026-11-03, worked). Worth automating detection: after N pair-verify failures, show "Re-pair over USB" in the status.
- Old spike app `dev.crosstheplatforms.spike` ("CTP Spike", tile "Send clipboard") is still on the phone and confuses the tile
  choice — uninstall it: `adb uninstall dev.crosstheplatforms.spike`. Bridge's tile is **"Send clipboard to PC"**.
- Debug builds only so far; no installer, no code signing.

## 7. Remaining work to the finished product, in order
### 7.1 Finish plan 3 (transfer) — DONE (owner closed it 2026-11-04)
Owner skipped the open device checks, so these stay **untested**: rows 2, 3, 4, 9, 10, 13 of `docs/v1/checklist-transfer.md` and the
5-file share from the phone (final-review fix I4). If pairing, cut-off, firewall fallback or boot receive misbehaves, start there.

### 7.2 Deferred minors from plan 3's final review (do any time; each is small)
1. `node.rs` `on_ctl` Hello: ignore `Hello{addr}` unless `addr.id == from`.
2. `sendto.rs` `refresh`: sanitise the peer name *before* adding `Bridge – ` (odd names lose the prefix and are never cleaned up).
3. `.part` leftovers on app quit / dropped receive / failed rename → sweep `*.part` in the receive dir at start, or a drop guard.
4. No free-space check / offer limits for paired peers → refuse an offer bigger than free space up front (matters on this disk).
5. `ClipTile.kt`: `startActivityAndCollapse(PendingIntent)` needs API 34 but `minSdk = 31` → version check or raise minSdk.
6. Tests: stranger `Offer` and stranger `ctp/bulk/1` claim; end-to-end pairing reuse/expiry through `handle_pair`.
7. Clippy: manual fold in `bulk.rs` token compare; `field_reassign_with_default` in the router test.
Older deferred minors (clipboard plan): connect loop runs while sync is off; tray check can disagree if save fails; watcher
failure not in status; wrong DDI remounts forever; parallel Pair clicks / notice never clears; size check after decode.

### 7.3 Plan 4 — iPad Swift app — DONE (owner sideloaded and ran the checklist 2026-11-04)
Plan: `docs/superpowers/plans/2026-11-04-v1-ipad-app.md`. Code in `apps/ios/`, build on GitHub Actions (`ios` workflow, artifact
`Bridge-ipa`, kept 3 days), install via iloader → SideStore 0.7.0 (free Apple ID ⇒ re-sign every 7 days). Still open:
- Checklist row 13: confirm data and pairing survive the first real re-sign. Rows 4 and 5 have no MB/s recorded, so the iPad→PC speed is unmeasured.
- Rulings and deferred minors: `docs/v1/ipad-deferred.md`.
- `ios-spike` CI has failed since `v1/transfer`: `spikes/ios-app` calls `startReceiver` and `sendFileFfi`, which the core no longer exposes. Not an app bug.
- Run `apps/android/build-rust.ps1` before the next Android build; the local bindings are stale.

### 7.4 Plan 5 — Tauri mobile flavours (spec §4: "the Tauri app with shadcn on all devices")
Same React UI on Android/iPad as a settings/companion app, sharing `apps/tauri/src`. Decide with the owner how it coexists with
the native apps (spec §4 has the owner's decision; §11/§12 list what was left to the builder).

### 7.5 Ship v1
Release builds; Windows installer (`pnpm tauri build` without `--no-bundle` → NSIS/MSI); Android signed release (Play Console
internal testing track gives auto-updates — needs a privacy policy, data-safety form "no data leaves your devices", and a
connectedDevice service justification); merge the three branches; tag `v1.0`.

### 7.6 Then v1.5 / v2 (spec §2, §5, §6)
Cross-platform input (phone/iPad controlling Windows and each other), screen mirroring/extension; v2 internet transfer — the
pairing keys and iroh relay already work over the internet (QR carries the relay URL), so v2 is mostly UX + limits.

## 8. Decisions made during plan 3 (owner to check; "cost if wrong" in brackets)
- Wrong pairing guesses don't burn the code [none: 128-bit]. `Hello` address announce every 60 s [tiny message/min].
- Fixed iroh port across restarts + id-only n0 lookup fallback [none]. Bulk addrs = primary LAN IPv4 + loopback [multi-NIC may
  try the wrong one first; iroh still delivers].
- One-time token + single connection attempt on the TCP listener [a LAN stranger can only force the slower iroh path].
- Fallback only on connect failure, not mid-stream [a Wi-Fi drop mid-file ends as Failed; resend manually].
- Larger QUIC windows on the node endpoint [memory]. `start_node` takes `kind` [one arg]. Generated bindings/jniLibs gitignored.
- Phones get clipboard text only, no images [images don't reach the phone]. Shared plain text from the phone → PC clipboard.
- Send To shortcuts via COM `IShellLinkW`; "Open folder" = `explorer.exe`.
- QR = relay + primary IPv4 only [phone on another subnet pairs via relay first].
- Android share fix verified by build + install only [re-run the 5-file share].

## 9. Machine gotchas (do not relearn these)
- **Never force-kill a BLE publisher** (ctp-app / ctp-engine / ble-hid): the MT7921 Bluetooth wedged before (needed a BIOS
  reset once). Quit from **tray → Quit**. Earlier BT hangs were caused by `FromIdAsync` per 200 ms poll + `ThroughputOptimized`
  requests — fixed (cached device + watcher thread); don't reintroduce either.
- Windows locks running exes → build to `C:\dev\ctp-target-check` or quit the app first.
- Keyboard hook: Windows skips `WH_KEYBOARD_LL` while our own window is foreground → fixed (focus moves to the shell + raw-input
  fallback via `HookWitness`). If keys ever leak to Windows again, look there first.
- Gradle needs `TMP=C:\dev\_ref\tmp`; JAVA_HOME = Android Studio jbr. `pnpm dlx` broken → `npx`. TypeScript 7 (no `baseUrl`).
  shadcn CLI writes `from "cn"` → fix to `@/lib/utils`.
- adb: `$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe`; `MSYS_NO_PATHCONV=1` for device paths.
- Don't read `%APPDATA%\CrossThePlatforms\ipad\rp-pairing.plist` (secret; existence checks only). Apple ID password only typed by
  the owner into tools, never in chat.
- `.gitignore` has `pairing*` for secrets, with exceptions for `core/src/pairing.rs` and `core/tests/pairing.rs`. New files named `pairing*` need an exception too. Never run `git clean -X` here without checking.
- Local-only (not in git): `C:\dev\_ref\` (Universal Bridge + idevice clones, DDI in `ddi\`, pairing records).
- Many tool calls need the sandbox disabled (gh push, Bluetooth, Gradle loopback, pnpm install, adb).
- `[profile.dev.package."*"] opt-level = 2`: a fresh target dir takes ~20 min for the Tauri app, ~9 min for cargo-ndk.
- Node 26.3.0 here has CVE-2026-111497 (a hook warns once); upgrade to 26.12.1+.
- Device ids: iPad `BluetoothLE#BluetoothLE14:d4:24:54:53:bc-5b:95:30:4d:1c:f5`, S24+ BLE `…-4e:5d:31:c1:13:6e`; phone adb serial
  `R3CX80HSKMZ` (SM-S926B). Monitor `\\.\DISPLAY5` 1920×1080. Settings `%APPDATA%\CrossThePlatforms\settings.json`,
  log `%APPDATA%\CrossThePlatforms\engine.log`.

## 10. How to resume with Claude (cheapest path)
Start a session in the worktree and say: *"Read docs/HANDOFF.md. Continue with §7.x."* For a new feature, use
superpowers:writing-plans → executing-plans (native) → one final opus review, as done for plans 2 and 3.
