# Handoff — where the work stands (2026-11-04)

Read this first, then `docs/specs/2026-11-02-device-bridge-design.md`, `docs/spikes/results.md`,
`docs/superpowers/plans/2026-11-04-v1-windows-hub-input.md`, `docs/v1/checklist-input.md`.

## Project
Own open-source app suite: Windows 11 laptop (hub) + Galaxy S24+ + iPad Air 13" M2 work as one setup.
v1 = keyboard/mouse sharing over BLE HID (done, debugging), clipboard, file transfer, then Kotlin/Swift/Tauri-mobile apps. v2 = internet.
Owner is in Germany (German keyboard layout, AltGr matters), uses Rust/Tauri/shadcn, no Mac, free Apple ID.

## Branch / PR state
- `main` has the foundation + all spikes (PR #1 merged by the owner).
- Current branch: **`v1/windows-hub-input`** (pushed). Plan 1 Tasks 1–8 implemented, review fix pass done, then:
  - `a22f2fe` keyboard queue (core::keyqueue) — **did NOT fix the real bug** (see below); keep or revert after root cause.
  - **Uncommitted at handoff → committed as WIP "diag"**: `platform/windows/src/diag.rs` + logging in capture/engine/peripheral (writes `%APPDATA%\CrossThePlatforms\engine.log`; ids/timings/verdicts only, never key codes).
- No PR open for this branch yet.

## THE OPEN BUG (top priority — safety rule violated)
In `ctp-app.exe` (Tauri tray app), while the iPad is in control: **keyboard never reaches the device, Ctrl+Alt+1/Q and Esc×3 go to Windows** — owner had to turn iPad Bluetooth off to get back. Mouse still moves the iPad.
- Evidence (engine.log of that run): iPad active 41 s → 86 s with **zero keyboard sends**; keyboard + mouse subscribed together at 2.3 s (so the "late keyboard subscription" hypothesis behind `a22f2fe` was wrong).
- The headless `ctp-engine.exe` (release) earlier passed all engine checks incl. hotkeys/Esc×3 (checklist rows marked PASS, pre-review build).
- **Leading hypothesis:** the `WH_KEYBOARD_LL` hook is gone. Candidates: (a) process-wide panic hook in `capture::start` calls `unhook()` on a panic on ANY thread (Tauri/UI) — reviewer flagged this; (b) Windows silently removes LL hooks whose callback exceeds LowLevelHooksTimeout (debug build, hook thread also handles WM_INPUT). Mouse still "works" because Raw Input `Move` is forwarded regardless of hooks.
- The diagnostic build now logs: `hooks installed`, `hooks removed`, `PANIC on thread …`, and every `kb hook: down suppress=… -> Verdict`. **Next step:** have the owner run `target\debug\ctp-app.exe`, wait for iPad connected, Ctrl+Alt+1, type, move mouse, Ctrl+Alt+Q (or BT off), quit from tray; then read `%APPDATA%\CrossThePlatforms\engine.log`.
  - No `kb hook:` lines at all → hook never/no longer installed → look for `hooks removed`/`PANIC`.
  - `kb hook` lines with `suppress=false` while active → SUPPRESS cleared (unhook) → find who called it.
- **Status 2026-11-03: marked WORKING by the owner — closed.** Do not keep investigating; the owner reports it if it ever recurs.
  Notes below stay only as a head start for that case.
- **Update 2026-11-03 (4 device runs): intermittent, not root-caused.**
  - 3 of 4 runs clean (hotkey and edge entry, typing, Esc×3, Ctrl+Alt+Q all fine). Run 2 failed: iPad entered by edge 5 s after launch,
    and `kb_hook` was **never called** from launch until ~79 s (no `kb hook` lines at all, incl. two iPad sessions), then worked normally.
    No `hooks removed`, no `PANIC` → (a) panic-unhook and (b) Windows removing the hook are both **ruled out**; Windows skipped the hook.
    Mouse kept working (Raw Input), so the capture thread was pumping — not a hung thread.
  - Not UIPI in the clean runs (fg never elevated). Touchpad, not a high-rate mouse. Hook lag 0 ms, callback < 3 ms in all logged seconds.
  - Only remaining difference spotted: in run 2 Notepad really had focus; clean runs had `claude.exe` focused. Untested.
  - Uncommitted diag in worktree `ctp-windows-hub-input-bug-b2c51e`: per-second hook stats + foreground exe/elevation, and a raw-keyboard
    witness that logs `HOOK SKIPPED` when Windows delivers keys our hook never saw (swallowed keys never reach raw input, so raw keys = skip).
  - Proposed safety net (not built): raw key-**down** while SUPPRESS → return to Windows + reinstall hooks, so the first key press frees you.
- Follow superpowers:systematic-debugging (one hypothesis at a time; this is fix attempt #2 after `a22f2fe`).
- Likely fixes once confirmed: only unhook in the panic hook for panics on the capture/engine threads (name them); detect a dead hook (no hook callbacks while WM_INPUT arrives) and reinstall; move work out of the hook thread; test in a **release** build of the app.

## Owner device facts (see docs/spikes/results.md)
- iPad id `BluetoothLE#BluetoothLE14:d4:24:54:53:bc-5b:95:30:4d:1c:f5`, S24+ id `…-4e:5d:31:c1:13:6e`. Monitor `\\.\DISPLAY5` 1920×1080 (scaled coords). Settings file at `%APPDATA%\CrossThePlatforms\settings.json` (iPad Right, S24+ Left).
- iPad BLE interval 15 ms (minimum). LocalSend 90 MB/s vs our iroh ~40–44 (phone), 79 (PC→iPad), 16 (iPad→PC) → v1 transfer plan: LAN bulk over TLS-TCP.
- iPad clipboard: idevice over RemotePairing works on Wi-Fi; Developer Mode on; DDI must be re-mounted after each iPad reboot.

## Machine gotchas (do not relearn these)
- **Never force-kill a BLE publisher** (ble-hid/ctp-engine/ctp-app): the MediaTek MT7921 BT function wedged once and needed Shift+Shutdown + 20 s power-button drain. Stop ctp-engine with **Enter**; quit the app from **tray → Quit**.
- Windows locks running exes: ask the owner to quit before rebuilding.
- Gradle needs `TMP=C:\dev\_ref\tmp` (JDK socket path limit); JAVA_HOME = Android Studio jbr.
- `pnpm dlx` is broken here → use `npx`. TypeScript 7 (no `baseUrl`). shadcn CLI writes `from "cn"` imports → fix to `@/lib/utils`.
- adb: `$LOCALAPPDATA/Android/Sdk/platform-tools/adb.exe`; use `MSYS_NO_PATHCONV=1` for device paths, but then pass Windows paths to Windows exes.
- Local-only files (not in git): `C:\dev\_ref\` (Universal Bridge + idevice clones, DDI files in `ddi\`, pairing records in `pairing\`), `idevice-tools` installed via cargo.
- Many tool calls need `dangerouslyDisableSandbox` (gh push, Bluetooth, gradle loopback).
- Node 26.3.0 here has CVE-2026-111497; owner advised to upgrade.

## Remaining after the bug
0. **Plan 2 (clipboard, Windows ↔ iPad) written:** `docs/superpowers/plans/2026-11-03-v1-clipboard.md` — awaiting owner review + execution choice; implement on `v1/clipboard` from `v1/windows-hub-input`.
1. Finish v1 plan 1 acceptance — **owner deferred 2026-11-03** ("base app is working; fix bugs as they show up") (`docs/v1/checklist-input.md` App rows: tray, single instance, layout/calibration/diagnostics, BT off/on, sleep/resume, reboot, AltGr), then open the PR for `v1/windows-hub-input`.
2. Deferred minors from the final review are listed at the end of the plan-1 conversation (hold reset on monitor poll, notices raw ids, stale Ctrl/Alt after secure desktop, RunEvent::Exit, settings range checks, connected() polling cost, gain label rounding, shadcn in deps).
3. v1 plans 2–4: clipboard (all three devices), file transfer (TLS-TCP bulk), then Kotlin + Swift apps, then Tauri mobile.
