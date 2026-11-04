# Base UI for Bridge (Windows, iPad, Android)

Approved look: the Claude Design canvas "Bridge UI redesign"
(https://claude.ai/artifact/KLqYNGJBTFEyF17ccYxxTk). It is the visual source of truth and draws every state named here.
Motion reference: the prototype "Bridge Desk" (https://claude.ai/artifact/WBmNaX5jGsp8TF1Rf6wr8a). Use it for how things
move, not for how they look: it was restyled afterwards and the owner preferred the canvas look.

## Why

The apps work, but the owner finds them plain. Today's UI is a stack of generic cards and forms, and a device is scattered
across five tabs. The owner wants an app that is fun to use and clear about what it does, with direct manipulation such as
dragging devices into position, and that still looks reliable rather than like a toy. This is a base UI for the current
features, and later features must fit in without a redesign.

## Goals

- The Desk: your devices are objects you can pick up and put next to a screen edge. Files are dropped on them.
- At a glance: where the keyboard is, what is moving, what needs attention.
- One place per device, and one guided way to add a device instead of three unrelated pairing paths.
- Every state is designed: empty, offline, waiting, failed, Bluetooth off, expired code. No raw error strings.
- One look and one vocabulary across Windows, iPad and Android, in light and dark, following the system setting.

## Non-goals

- No new product features beyond what this document lists. Cancelling a transfer is left out.
- No Tauri mobile apps. Spec §4 and HANDOFF §7.4 planned them to give the same UI on mobile. The owner said they only care
  about which app looks best, so the native apps get the new look instead, keeping their background service, share
  extension and Bluetooth code. *(Claude's call, stated to the owner, not objected to.)* Update the main spec and HANDOFF.

## Anti-goals

- Toy feel: wobble, tilt or long bounces. Motion is quick and settles.
- Plain feel: a form-and-card settings app.
- Fake states: never show "Connected", "Sent" or "Saved" unless the app knows it is true.

## Constraints

- Never block or slow the input engine from the UI. Never log file names, keys or clipboard text.
- Input safety is unchanged: the emergency exit (Esc three times) and Ctrl+Alt+Q always work.
- No Mac: every iPad look check is a CI build plus a sideload, so iPad comes last and gets as few rounds as possible.
- `settings.json` must keep loading older files (`#[serde(default)]`). `apps/android/build-rust.ps1` must run after any
  core FFI change.
- Keyboard use: every action works without a mouse. Reduced motion makes all movement instant.

## Structure

**Windows hub.** Window 1100×760 by default (today 920×660). Top bar: Bridge, the tabs Desk, Transfers, Clipboard, Settings, a
live status line, and Add a device. New global features become a tab. New per-device features become a row in the device panel.

- **Desk.** Left: the map, with Screen 1 drawn in the middle, placed devices at its edges, a "Not placed" tray, and an activity
  strip below with the current transfer and the clipboard switch. Right: the panel for the selected device: Switch button,
  status of Keyboard and mouse, Files and Clipboard, pointer speed, Fine tuning (today's Return push, Return hold, Screen width
  under plain names), colour, Hold to forget.
- **Several monitors.** The canvas draws one screen. With more, draw them all to scale from `list_monitors`, each with its own
  edges. Attachments already store monitor and edge.
- **Transfers.** Moving now, Waiting, History (kept for the session), received folder with Open folder and Change, Send again
  on failures.
- **Clipboard.** Sync switch, status per device, Pair over USB, synced and skipped counts.
- **Settings.** Start with Windows, Appearance, edge hold, Hotkeys, Diagnostics, version, note about the tray.
- **Add a device.** iPad: Bluetooth, then Files (code), then Clipboard (USB). Phone: Bluetooth, then Files and clipboard.
  States: waiting, done, expired code, finished. Each step moves on by itself when the app sees it happen, and can be skipped.
- **Tray.** Unchanged in function. Wording follows this document.

**iPad and Android.** One screen, no tabs: the PC, Send files, Send clipboard, live progress, Recent, where received files go,
share status, Pair, Hold to forget. Tablet uses two columns. Android gains a Send clipboard button (tile stays). Share flows get
the same look.

## Behaviour

| What | Rule |
|---|---|
| Pick up | Starts after 5 px of movement. Device grows to 107% with a soft shadow. |
| Magnet | Nearest edge within 170 px lights in the device colour and pulls the device 35% of the way. |
| Put down | Settles in about 300 ms, saves at once, status line confirms with the hotkey. |
| Taken edge | The two devices swap. |
| Keyboard | Arrow keys place the focused device. Delete unplaces it. Enter selects it. |
| Unplaced device | Stays paired for files and clipboard. Switch is disabled and says why. |
| File over a device | Target within 34 px grows to 112%. Release flies the file in over 300 ms, then a ring shows progress. |
| Forget | Hold 900 ms. Letting go cancels. Removes the file pairing, placement and link. The Bluetooth bond lives in the OS: tell the user to remove it there too. |
| Saving | No Save or Discard anywhere. Every change applies at once. Slider drags save when released. |
| Hotkey numbers | Follow placement order, as today. |
| Device colour | Five choices, kept per device. Text on it is always `#10202E`. Offline devices lose their colour, so state never relies on hue. |
| Errors | Plain sentence plus what to do. Never `String(e)`. |

**Waiting versus failed.** Only sends to an iPad queue (`Node::send_or_queue`). PC to phone and phone to PC fail with Send again.
iPad to PC uses the app's outgoing queue and stays queued. The UI must say which one happened.

## Data the app needs that it lacks

1. **Transfer details.** `Event::Progress` carries only an id. Add an event when a transfer starts, with id, peer, direction, file
   name and size, through FFI, so rows show names. Local UI only: names must still never reach logs.
2. **Reachability for files.** Peers have no online state. Derive it from `Hello` (sent every 60 s) with a timeout, and expose it to
   the hub. Until then a device may only say Connected for Bluetooth.
3. **One device across pairings.** Bluetooth host ids and iroh peer ids are unrelated. Store a link in hub settings, written by the
   Add flow when both appear. Placement stays keyed on the Bluetooth host id. *(Claude's call.)* The hub's device list is the union:
   one entry per link, plus any unlinked Bluetooth host or file peer.
   - Bluetooth only: can be placed and switched to. Its Files row says "Not paired for files", with a button into Add a device.
   - Files only: sits in the tray, takes file drops, and cannot be placed or switched to, because that needs Bluetooth. Its
     Keyboard and mouse row offers the Bluetooth step.
   - Existing devices: the expected path is a one-time choice in the device panel ("Is this also your iPad Air?"), because
     Bluetooth and file names may differ. Link by itself only when the two names are identical.
4. **Device colour** per device in hub settings. iPad and Android show their own fixed accent in the base: no colour sync.
5. **History** is kept in memory for the session only. Windows keeps each failed send's file paths there for Send again.

**Reachability is hub-only.** *(Claude's call.)* A closed iPad sends no `Hello`, and mobile apps have no reason to poll, so the
new reachability state is not exposed through FFI. The mobile PC row says "Paired" at rest and "PC not reachable" only after a send
fails. It never says "Connected". The transfer-start event (item 1) does go through FFI, so Android bindings need a rebuild.

**Send again is Windows only.** *(Claude's call.)* Shared files on mobile can lose their access or temporary copy, so a failed phone
send tells you to send it again from the app you started in. iPad sends retry by themselves from the outgoing queue.

**Drops always try.** *(Claude's call.)* A file dropped on any device, offline or unplaced, is always attempted, because
reachability can be up to a minute stale. The result sets the message: an iPad queues, anything else fails with Send again.

## Visual system

Tokens, type, shapes, component states and motion are on the canvas page "Design system", board Foundations. Font: Rubik,
bundled in each app (Open Font License). Primary action `#FFC531`. Each screen on the canvas has named states (`drag`, `offline`,
`addExpired`...): use the same names in code and tests.

Windows stays React, Tailwind and Base UI, replacing the shadcn look through tokens. iPad stays SwiftUI, Android stays Compose.

## Order

1. Core additions (items 1 and 2), with tests.
2. Windows hub, shipped and used first, because it can be checked on this machine.
3. Android.
4. iPad, in one sideload round if possible.

## Done means

- Every state on the canvas exists in the app and matches it, in light and dark.
- Drag, keyboard placement, file drop, hold to forget, Add a device and the failure states work on real devices.
- Existing tests pass. New logic (placement and swap, state derivation, linking) has unit tests.
- Owner has used it and agrees it looks right.

## Left to the builder

Component and file structure, how the spring is implemented, the exact reachability timeout, event names, how many plans this
splits into, and the look of any small state the canvas does not draw, following Foundations.
