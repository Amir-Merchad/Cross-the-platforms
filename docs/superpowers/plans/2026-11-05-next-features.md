# Next Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the owner's 13 features from `docs/v1.5/next-features.md`.

**Architecture:** Each feature is a small slice: pure logic in `core` with tests, a thin Windows or mobile layer, and hub UI following the base UI design. A tolerant message parser comes first, so new messages cannot break older devices.

**Tech Stack:** Rust, Tauri 2, React, Compose, SwiftUI, `idevice` 0.1.68.

**Spec:** `docs/v1.5/next-features.md`. Context: `docs/v1.5/morning.md`, `docs/spikes/ipad-pointer.md`. Calls below are Claude's, for the owner to check.

## Global Constraints

- Branch `v1.5/input-27ztiw`. Push only to `v1.5/*`. No merge, PR or release.
- Never log file names, keys or clipboard text. History and notes stay on the device. Never read pairing records.
- The engine thread never waits on the network or disk.
- Hub UI follows the Foundations tokens and plain-sentence copy. Every action works by keyboard.
- Ask the owner to quit from the tray before rebuilding into `target\debug`. Never force-kill `ctp-app`.
- Features that need a device end with a checklist row in `docs/v1.5/checklist-features.md`, never a claim that it works.

## Review Focus

1. A password manager copies a secret: it never enters history, sync or disk.
2. A newer device sends a message an older build does not know: the older build ignores it and stays connected.
3. A huge image or HTML copy: history stays within its size limit, and the clipboard stays responsive.
4. History file corrupt or locked: Bridge starts with an empty history and keeps the bad file aside.
5. A hotkey fires with no device in control or nothing selected: a plain message, nothing sent.

---

### Task 1: Tolerant messages

**Files:** `core/src/node.rs`, `core/tests/node.rs`

- [ ] Test `unknown_control_message_is_ignored`: a line with an unknown `t` value decodes to `Ctl::Unknown`, gets no reply, and the connection continues.
- [ ] Add `#[serde(other)] Unknown` to `Ctl`, handle it in `on_ctl`. Commit `feat(core): ignore unknown control messages`.

### Task 2: Clipboard history (feature 1)

**Files:** create `core/src/cliphistory.rs`; modify `platform/windows/src/clipsync.rs`, `clipboard.rs`, `apps/tauri/src/pages/Clipboard.tsx`, `commands.rs`, `apps/ios/BridgeKit`, `apps/ios/Sources`.

Calls: 50 unpinned items, pins do not count and never expire. Images kept as files, 20 MB each at most, 200 MB total. Stored in `%APPDATA%\CrossThePlatforms\history`. A History switch, default on, and Clear history. Skip anything Windows marks `ExcludeClipboardContentFromMonitorProcessing` or `CanIncludeInClipboardHistory` = 0, in history and in sync (Review Focus 1).

- [ ] Tests: `oldest_unpinned_goes_at_51`, `pins_survive_trimming`, `duplicate_moves_to_top`, `search_matches_text_case_insensitive`, `corrupt_file_starts_empty_and_keeps_bak`, `oversize_image_is_skipped`.
- [ ] Windows: `clipboard_excluded()` test through a pure format-list check. Hub: search box, list with source device and time, click copies, pin, delete. iPad: same list from clips seen while Bridge is open.
- [ ] Commit per layer.

### Task 3: Quick notes inbox (feature 7)

**Files:** `core/src/node.rs` (`Ctl::Note { text }`), `core/src/ffi.rs`, new `apps/tauri/src/pages/Notes.tsx`, Android share and tile, iPad button.

Calls: one Markdown file, `Documents\Bridge\Inbox.md`, newest first, each entry with time and device. A new top tab, Notes. Android: a "Send to notes" choice in the share sheet.

- [ ] Tests: `note_reaches_the_pc`, `note_is_appended_with_time_and_device`, `note_survives_restart`. Build UI. Commit.

### Task 4: Keep formatting (feature 2)

**Files:** `core/src/clipboard.rs`, `platform/windows/src/clipboard.rs`, `ipad.rs`.

Calls: `Content::Text` gains optional `html`. Windows reads and writes `CF_HTML` with plain text alongside. iPad sends `public.html` if the pasteboard service allows it, proven first in a spike step. Phones stay plain text.

- [ ] Tests: `cf_html_round_trip`, `html_fragment_offsets_are_right`, `plain_text_always_present`, `old_history_entries_load`. Commit.

### Task 5: Drop on the Desk (feature 4)

Already built in the base UI (`useFileDrop.ts`). Add a checklist row. Add `Ctrl` as nothing new. No code unless the row fails.

### Task 6: Send to the device in control (feature 3)

**Files:** `core/src/router.rs`, `platform/windows/src/capture.rs`, `engine.rs`, new `platform/windows/src/selection.rs`.

Calls: `Ctrl+Alt+V` puts the clipboard on the controlled device, then presses its paste (`Cmd+V` iPad, `Ctrl+V` others). `Ctrl+Alt+F` sends Explorer's selected files to it. Hub Settings lists both.

- [ ] Tests: `paste_hotkey_with_no_device_says_so` (Review Focus 5), `paste_uses_cmd_on_ipad`. Selection through Shell COM on a worker thread, never the engine thread. Commit.

### Task 7: Smoother scroll (feature 9)

**Files:** `core/src/router.rs`, `core/src/ipadhid.rs`, `platform/windows/src/capture.rs`.

Calls: `Input::Wheel` carries raw `WHEEL_DELTA` units for `ipad-wifi`, summed and emitted in report 7 units at 16 per 120 raw, at least `SCROLL_MIN_UNITS` (4) to a report. The Bluetooth path keeps whole notches.

- [ ] Tests: `small_deltas_accumulate_to_one_unit`, `full_notch_is_sixteen_units`, `direction_is_kept`. Commit.

### Task 8: Per-device keyboard layout (feature 10)

**Files:** create `core/src/keylayout.rs`; modify `core/src/ipadhid.rs`, hub device panel, `settings.rs`.

Calls: `keyboard_layout` per device: `same` (default) or `german-to-us`. In `german-to-us`, German keys become the US sequences from proof 8: Option+u then vowel for umlauts, Option+s for ß, plus swapped Y and Z and symbols. Test the owner's iPad `Hardware Keyboard` setting first, since `same` needs no translation if German is offered.

- [ ] Tests: `umlaut_becomes_dead_key_sequence`, `shift_gives_capital_umlaut`, `z_and_y_swap`, `same_changes_nothing`. Commit.

### Task 9: iPad screenshot to clipboard (feature 5)

**Files:** `platform/windows/src/ipadinput.rs`, `engine.rs`, `clipboard.rs`.

Calls: `Ctrl+Alt+S`. Uses `ScreenCaptureServiceClient`, takes about 2 s, and a status line confirms it. Also saves to history.

- [ ] Port from the spike. Test `screenshot_hotkey_needs_a_ready_ipad`. Device row. Commit.

### Task 10: Battery and link on the Desk (feature 11)

**Files:** `ipadinput.rs`, `core/src/node.rs` (`Ctl::Status { battery, charging }`), Android, hub `DeviceObject.tsx`, `DevicePanel.tsx`.

Calls: iPad battery through its diagnostics or lockdown battery domain, every 60 s. Phone through `Ctl::Status`. Link shown as Good, OK or Weak from keepalive round trip: under 30 ms, under 100 ms, above.

- [ ] Tests: `link_grades`, `missing_battery_shows_nothing`. Device row. Commit.

### Task 11: Bluetooth fallback (feature 12)

**Files:** `platform/windows/src/engine.rs`, `core/src/settings.rs`, hub.

Calls: a setting, Fallback to Bluetooth, default off because the adapter crashes. When on, the iPad's Bluetooth host is kept instead of retired, and is used if `ipad-wifi` stays down for 30 s. Switching routes never happens during control.

- [ ] Tests: `route_prefers_wifi`, `route_falls_back_after_30s`, `no_switch_during_control`. Commit.

### Task 12: Focus mode (feature 6)

**Files:** `core/src/node.rs` (`Ctl::Focus { on }`), Android, hub.

Calls: a hub switch. Android uses `NotificationManager.setInterruptionFilter`, and needs Do Not Disturb access, with a screen explaining it. Ending the session or losing the PC for 2 hours restores the phone. iPad focus cannot be set by an app.

- [ ] Tests: `focus_message_round_trip`. Android unit test for restoring. Device row. Commit.

### Task 13: Gestures (feature 13)

**Files:** `spikes/ipad-input`, then `core/src/ipadhid.rs`.

Spike first, with the owner: report IDs for dock swipe, navigation swipe, scale and rotation in `docs/spikes/ipad-pointer.md`. Windows keeps three and four finger swipes for itself unless the owner disables them, so the first version maps hotkeys and Ctrl+wheel to iPad gestures. Stop and report if reports do nothing.

### Task 14: Latency number (feature 8)

**Files:** `docs/v1.5/latency-test.md`, hub Settings.

A Settings button flashes a square on the PC and sends a key. The owner films at 240 fps and counts frames. Record the result and tune pacing if useful.

### Task 15: Close out

Checklist `docs/v1.5/checklist-features.md`, HANDOFF, final review.
