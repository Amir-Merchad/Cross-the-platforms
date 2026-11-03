# Handoff — where the work stands (2026-11-03, evening)

Read this first, then `docs/specs/2026-11-02-device-bridge-design.md`, `docs/spikes/results.md`,
`docs/superpowers/plans/2026-11-03-v1-clipboard.md`, `docs/v1/checklist-clipboard.md`.

## Project
Own open-source app suite: Windows 11 laptop (hub) + Galaxy S24+ + iPad Air 13" M2 work as one setup.
v1 = keyboard/mouse sharing over BLE HID (done), clipboard (in device testing), file transfer, then Kotlin/Swift/Tauri-mobile apps. v2 = internet.
Owner is in Germany (German keyboard layout, AltGr matters), uses Rust/Tauri/shadcn, no Mac, free Apple ID.

## Where the code is (IMPORTANT — two checkouts)
- **`v1/clipboard`** (current work, **local only, not pushed**, no PR) lives in the worktree
  `C:\dev\Cross-the-platforms\.claude\worktrees\ctp-windows-hub-input-bug-b2c51e`. Branched from `v1/windows-hub-input` @ `04c6a13`.
  Latest commits: `a6e26b6` diag (BLE interval at connect, iPad connect-failure reason) · `2877c0b` final-review fix pass · plan Tasks 1–8 before that.
  Build: `cargo build -p ctp-app --features tauri/custom-protocol` (UI dist copied in) or `cd apps/tauri && pnpm tauri build --debug` (node_modules installed there).
  Exe under test: `<worktree>\target\debug\ctp-app.exe`.
- **`v1/windows-hub-input`** (keyboard/mouse, pushed up to `9cd8c1e`; local has `59066d6`, `04c6a13` on top, not pushed) is checked out in the main
  checkout `C:\dev\Cross-the-platforms`. Its exe `C:\dev\Cross-the-platforms\target\debug\ctp-app.exe` has **no clipboard** (handy for A/B).
- Owner said: no merge / no PR yet. `main` = foundation + spikes (PR #1).
- Autostart (`HKCU\...\Run\Cross the Platforms`) points to whichever exe was started last.

## Clipboard (plan 2) — device session in progress
Implemented: core `Hub` (hash echo guard + 4-deep pending-echo ring per endpoint, last writer wins, pause, 64 MiB limit) and RGBA codec;
Windows clipboard watcher (skips password-manager copies marked `ExcludeClipboardContentFromMonitorProcessing`); iPad link over
RemotePairing Wi-Fi with USB fallback, TCP keepalive, 30 s connect timeout, auto DDI mount, change-count baseline; `clipsync` service
thread; tray "Clipboard sync" check + Clipboard tab (status, Pair iPad (USB), counters). iPad files in `%APPDATA%\CrossThePlatforms\ipad\`
(`rp-pairing.plist`, `ddi\*`, copied from `C:\dev\_ref`). Tests: core 85, platform 31 (+5 device-only, `--include-ignored`), UI vitest 10.

**Device results so far (over USB, 2026-11-03):** a Windows text → iPad ✅ · b iPad text → Windows ✅ · c Windows screenshot → iPad ✅ ·
**d iPad image → Windows Paint ❌** · e same copy twice ✅ · f Explorer file copy ignored ✅ · g/h toggle off/on, no replay ✅.
Unplugging USB → switched to **Wi-Fi by itself in 3 s** ✅ (log `iPad clipboard connected (wifi=true)`).

**Open items, in order:**
1. **d fails** although the log says `clip image -> Windows (22380544 bytes)` (2732×2048 RGBA written via arboard). So the write "succeeds"
   but Paint can't paste it. Next: owner copies an image on the iPad, then inspect clipboard *format names only* (e.g. PowerShell
   `Add-Type -A System.Windows.Forms; [Windows.Forms.Clipboard]::GetDataObject().GetFormats()`); suspect arboard's CF_DIBV5/PNG output
   for large top-down RGBA, or alpha. Fix in `platform/windows/src/clipboard.rs::write`.
2. **At startup the badge said "Not paired"** although `rp-pairing.plist` exists (later Wi-Fi worked). Cause unknown — the new build
   (`a6e26b6`) logs `iPad clipboard not connected: <reason>` once per new reason; read it on the next run.
3. Pending checklist rows: lock/unlock, iPad off Wi-Fi → back, iPad reboot (DDI remount over Wi-Fi or needs USB?), laptop reboot,
   input still fine while syncing, log has no clipboard text. Record in `docs/v1/checklist-clipboard.md`.
4. Deferred minors (final review): connect loop runs while sync is off; tray check can disagree if save fails; watcher failure not in
   status; wrong DDI remounts forever; parallel Pair clicks / notice never clears; size check after decode; tab switch freezes if service dies.

**The run the owner was about to do** (iPad unplugged, Wi-Fi from the start): start the worktree exe → Clipboard tab badge after ~15 s →
Diagnostics tab while controlling the iPad (Report interval, Send avg/max) → copy an image on the iPad, don't paste → inspect formats.

## iPad pointer stutter (new, not clipboard)
Owner sees the iPad pointer update in jerks (~every 200 ms). Same with the old no-clipboard build → **not caused by clipboard sync**.
No "mouse send slow" in the log. Next: Diagnostics tab numbers (interval should be 15 ms; 30 ms seen once before), new log line
`connected; interval N ms`. Spike notes: remaining feel = iPad 60 Hz + 15 ms slot + iPad pointer animations (owner may turn those off).

## Keyboard hook bug (owner closed it as "working" — but new evidence)
Original: in `ctp-app.exe` while the iPad is in control, keys/hotkeys/Esc×3 went to Windows; only BT-off got control back.
Findings (diag build, `HOOK SKIPPED` = raw keyboard input arrived but `WH_KEYBOARD_LL` was never called):
- Hook is never removed, no panic; Windows simply **skips** it (capture thread keeps pumping; mouse fine).
- **2026-11-03 clipboard session: `HOOK SKIPPED` while our own settings window (`ctp-app.exe`) had focus**, and once with `claude.exe`
  focused. Strong lead: the hook is skipped when the app's own (WebView2) window is foreground — matches the original failure if the
  settings window was open. Workaround told to owner: close the settings window before controlling the iPad.
- Owner was asked whether to investigate after the clipboard session (answer pending). Proposed safety net: raw key-down while
  SUPPRESS → back to Windows + reinstall hooks. Use superpowers:systematic-debugging; one hypothesis at a time.

## Owner device facts (see docs/spikes/results.md)
- iPad id `BluetoothLE#BluetoothLE14:d4:24:54:53:bc-5b:95:30:4d:1c:f5`, S24+ id `…-4e:5d:31:c1:13:6e`. Monitor `\\.\DISPLAY5` 1920×1080 (scaled coords). Settings `%APPDATA%\CrossThePlatforms\settings.json` (iPad Right, S24+ Left).
- iPad BLE interval 15 ms (minimum). LocalSend 90 MB/s vs our iroh ~40–44 (phone), 79 (PC→iPad), 16 (iPad→PC) → v1 transfer plan: LAN bulk over TLS-TCP.
- Logs: `%APPDATA%\CrossThePlatforms\engine.log` (older runs kept as `engine.run*.log`). Never contains keys or clipboard contents.

## Machine gotchas (do not relearn these)
- **Never force-kill a BLE publisher** (ble-hid/ctp-engine/ctp-app): the MT7921 BT function wedged once (Shift+Shutdown + 20 s power-button drain). Quit the app from **tray → Quit**; stop ctp-engine with **Enter**.
- Windows locks running exes: ask the owner to quit before rebuilding.
- The worktree guard blocks Edit-tool writes into the main checkout from this worktree session; work on the worktree branch, fast-forward the other branch with git.
- Reading `%APPDATA%\CrossThePlatforms\ipad\rp-pairing.plist` contents is (rightly) blocked as a secret — check existence only.
- Gradle needs `TMP=C:\dev\_ref\tmp`; JAVA_HOME = Android Studio jbr. `pnpm dlx` broken → `npx`. TypeScript 7 (no `baseUrl`). shadcn CLI writes `from "cn"` → fix to `@/lib/utils`.
- adb: `$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe`; `MSYS_NO_PATHCONV=1` for device paths.
- Local-only (not in git): `C:\dev\_ref\` (Universal Bridge + idevice clones, DDI in `ddi\`, pairing records in `pairing\`).
- Many tool calls need `dangerouslyDisableSandbox` (gh push, Bluetooth, gradle loopback, pnpm install).
- `[profile.dev.package."*"] opt-level = 2` is set: first dev build after a clean takes ~10 min.
- Node 26.3.0 here has CVE-2026-111497; owner advised to upgrade to 26.12.1+.

## After clipboard
1. v1 plan 1 acceptance (`docs/v1/checklist-input.md` App rows) — owner deferred; then PRs for `v1/windows-hub-input` and `v1/clipboard`.
2. Phone ↔ Windows clipboard comes with the Android app (needs pairing/transport from the file-transfer plan).
3. Next plans: file transfer (TLS-TCP bulk), then Kotlin + Swift apps, then Tauri mobile.
