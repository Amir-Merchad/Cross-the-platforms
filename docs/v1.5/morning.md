# Final report, night of 2026-11-05

Branch `v1.5/input-27ztiw` (the cloud session's designated branch; `v1.5/input` is untouched). Nothing merged, no PR, no release. **Nothing here has run on a device.** The cloud has no iPad, phone, Windows desktop or Bluetooth, so every device claim below is "should", not "does". We test together tomorrow.

## In one minute

| Step | State |
|---|---|
| 1. iPad keyboard and mouse over Wi-Fi (plan Tasks 2 to 6) | **Built.** No Bluetooth, no stream, no iPad app needed for input |
| Bluetooth optional | **Built.** Everything starts without the adapter; Settings has a Bluetooth switch |
| 2. Free screen layout | **Built.** Engine: devices beside devices, any number. Desk: drag a device next to another device |
| 8. Direct sharing between all devices | **Built (core, iPad; Android unbuilt).** Phones and iPads pair with each other; clipboard goes straight to every paired device |
| 4. One-step pairing | **Partly built.** iPad: USB pairing, then the files code goes onto the iPad's clipboard: no camera |
| 3. Android over Wi-Fi | **Designed; key-code translator built.** The rest needs a phone and the Android SDK |
| 5. Control from any device | **Designed.** Not built: needs a tolerant message parser on every device first |
| 6, 7. Mirroring, extension | **Designed.** Not built: need devices; extension needs a signed driver |
| 9. v2 internet | **Designed.** The node already falls back to a relay; limits and a relay test are next |

## What was built

| Commit | What |
|---|---|
| `8b34e72` | **iPad pairing, mount, discovery.** Every Pair over USB uses a name the iPad has never seen and saves it beside the record, which fixes the "bad signature" that broke Bridge's Wi-Fi. The spike's working record is used until you pair again. USB mounts the disk image through lockdown, Wi-Fi through the tunnel (proof 6). Discovery: saved address, then multicast, then every address on your /24. |
| `29ab6a7` | **Reports and keys** (`core/src/ipadhid.rs`). Report 5 (motion, buttons; bit 1 right click; buttons on every step so drags hold), report 7 (scroll, 16 per notch), key snapshots to down/up, release on leave. Settings: `bluetooth_enabled`, `retired_hosts`. |
| `16eb7f7` | **iPad input service** (`platform/windows/src/ipadinput.rs`). Own thread; engine never waits. Virtual mouse and keyboard; keepalive every 500 ms; two misses or a stalled write mark the link down; motion summed while busy, clicks and keys never merged; reconnect 2 s → 30 s. `ipad-input-check` binary for the device test. |
| `244384f` | **Engine.** `ipad-wifi` routed to the service; Bluetooth starts in the background only when switched on; a dropped iPad link returns input to Windows. |
| `dd6af64` | **Hub.** iPad moves from its Bluetooth host to Wi-Fi once; plain states; Bluetooth switch; Add an iPad is two steps. |
| `13d3fb2` | Review fixes: Bluetooth off drops its hosts; a stale virtual mouse is removed on reconnect. |
| `57131db` | **Free layout, engine** (`core/src/space.rs`, router). Screens and devices as rectangles joined at touching edges; pointer estimated on both axes; a deliberate push at any edge goes to Windows, the next device, or nowhere. Old settings cross exactly as before. |
| `280ed29` | **Desk editor.** Drag a device next to any placed device; chains work; screen-edge moves keep the layout in step and refuse overlaps; hotkeys follow placement order. |
| `921ed14` | **Direct sharing.** `FfiNode.invite` (iPad shows its own code), `share_clip` to every paired device, a filter that drops repeats and echoes (`core/src/clipmesh.rs`). iPad app: "Show my code for another device", applies received clips, Send clipboard to every device. Android tile: every device (**unbuilt**). |
| `47a7019` | **One-step iPad files pairing.** "Put the code on the iPad" in Add a device writes the files code to the iPad's clipboard; the hub makes sure it never spreads to Windows or phones. |
| `94eccf0` | **Android key codes** (`core/src/androidkeys.rs`) for step 3, and designs for steps 3 to 9. |

Designs and plans: `docs/superpowers/specs/2026-11-05-free-layout-design.md`, `…-direct-sharing-design.md`, `…-v1.5-remaining-steps-design.md`; `docs/superpowers/plans/2026-11-05-free-layout.md`.

## CI

The `core` workflow (Windows runner: hub tests and build, `cargo test -p bridge-core -p ctp-windows`, clippy `-D warnings` on `bridge-core`, `ctp-windows`, `ctp-app`) compiles and tests the Windows crates. `ios` builds the Swift package, the Rust core for iOS and the unsigned app.

| Run | Commit | Result |
|---|---|---|
| core 79 | `29ab6a7` iPad reports and keys | **success** |
| core 82 | `13d3fb2` all iPad Wi-Fi work | **success** |
| core 83 | `57131db` free layout engine | **success** |
| ios 18 | `13d3fb2` | **success** |
| ios 19 | `921ed14` direct sharing (iPad app changes) | **success** |
| core 86 | `921ed14` Desk editor + direct sharing | **success** |
| core 88 | `94eccf0` everything, incl. one-step pairing and Android keys | **success** |
| core 80, 81, 84, 85, 87, 89 | | cancelled by me: superseded by a later run, or docs only |

Locally: `cargo test -p bridge-core --lib` 157 pass; the new node test `phone_and_ipad_share_the_clipboard_directly` passes; `pnpm test` 154 pass; `tsc` clean; Windows crates pass clippy through a cross-check (`--target x86_64-pc-windows-gnu`), which compiles but cannot run Windows tests. Two older network tests (`pair_then_restart_keeps_pairing`, `queue_survives_a_pc_restart`) time out in the cloud container only; CI passes them. **Android is unbuilt**: there is no Android SDK in the cloud.

## What needs a device

- Everything about the iPad link: connecting with the spike's record and after a new pairing, pointer feel, scroll speed, right click, drag, keys, release on leave, drop detection time (about 0.75 to 1.25 s), reconnect after sleep and restart.
- Whether the iPad removes our virtual keyboard when a tunnel dies (if not, a key held during a Wi-Fi drop may stay down until pressed again; the stale mouse is cleaned up, the keyboard cannot be by name).
- Bluetooth off at launch, and the live switch.
- Free layout crossings on real screens, including device to device, and the far-edge estimate on the iPad.
- The Desk editor's drag beside a device (never looked at in a browser: no screenshot was taken).
- Phone ↔ iPad pairing with the iPad's code, and clipboard both ways with no PC; the iPad's paste prompt.
- "Put the code on the iPad" and Paste code in Bridge on the iPad.
- Android: build first, then the tile to every device.

## Calls I made (please check)

**iPad over Wi-Fi**
1. Until you pair again, input and clipboard use `rp-pairing-spike.plist` and its name. Pairing again wins from then on.
2. Pairing name: `cross-the-platforms-` plus the time in hex. Old pairings stay on the iPad as extra peers.
3. No display stream at all, per the proofs.
4. Keepalive: `connectedServices` every 500 ms, 250 ms limit, two misses = down.
5. Scroll: 16 units per wheel notch; pan passes through with the Bluetooth path's sign (direction untested).
6. Unknown errors show "Connecting…" and retry; details go to `engine.log` (no keys or text).
7. The Wi-Fi host shows as "iPad" in the engine's status; the linked files peer's name shows on the Desk.
8. Migration keeps place, hotkey, link and colour; calibration starts fresh on purpose.
9. The device check is a binary in `platform/windows/src/bin/`, not a spike subcommand, so CI compiles it.
10. German layout not done: try iPad Settings → General → Keyboard → Hardware Keyboard → German first.

**Bluetooth**
11. The switch applies live: off stops advertising and drops Bluetooth hosts; on starts it again in the background.

**Free layout**
12. Device height defaults to 4:3 of its width; old attachments become placements covering the whole monitor edge.
13. Crossing into a device beside another needs no hold (the hold is only for leaving Windows).
14. `attachments` stays in the file for the older build; devices beside devices exist only in `layout`.
15. The Desk editor is "drop beside a device", not a to-scale free-drag editor (less risk to the working map). A device placed beside another fills that whole side of it.
16. Converting to a saved layout leaves out devices on an unplugged screen (they cannot get a rectangle); they keep their attachment.

**Direct sharing**
17. Repeat window 10 s: the same text again within 10 s is not resent.
18. Unreachable devices are skipped, not queued.
19. Text only between phones and iPads, as before.
20. The iPad writes received clips only while Bridge is open there (iOS limits).
21. Send clipboard on the iPad and the Android tile go to every paired device.

**One-step pairing**
22. The files code reaches the iPad through its clipboard and you tap Paste code; Bridge does not read the clipboard by itself (iOS asks every time).

**Steps 3 to 9**: each choice is marked "call" in `docs/superpowers/specs/2026-11-05-v1.5-remaining-steps-design.md`. The big ones: the scrcpy route and a uhid mouse for Android; input from a phone off by default per device; an existing signed virtual display driver rather than our own; text up to 1 MB and a confirmation above 100 MB over the internet.

## Tests, in order

Quit Bridge from the tray before each rebuild. Build: `pnpm tauri build --debug --no-bundle` in `apps/tauri`.

1. **Service alone.** iPad unlocked, same Wi-Fi, USB out, Bridge closed, Spotlight open on the iPad. `cargo run -p ctp-windows --bin ipad-input-check`. Expect `Ready` within about 5 s, a square, a context menu, scrolling, `hello`, then `A` with nothing repeating.
2. **If it fails verify or says NotSetUp:** Bridge → Add a device → iPad → plug in → Pair over USB → Trust. Repeat step 1 unplugged.
3. **Bluetooth off at launch.** Settings → Bluetooth off, quit, start. Hotkeys, Esc ×3, Ctrl+Alt+Q work; no Bluetooth banner.
4. **Desk.** The iPad shows once, in its old place, "Connected over Wi-Fi".
5. **Normal use** with Bluetooth off: cross by the edge and by Ctrl+Alt+n; move, click, right click, drag, scroll, Shift, Cmd+A, arrows, Esc; push back.
6. **Wi-Fi drop** mid-control: input back on Windows within about a second, nothing stuck; reconnects by itself within 30 s.
7. **Held key during a drop**: hold Shift, drop the Wi-Fi, reconnect, type: lowercase means fine.
8. **iPad restart**, unlock once: reconnects with no USB.
9. **Sleep**: PC sleep/wake, iPad locked 10 minutes: both reconnect by themselves.
10. **Clipboard** PC ↔ iPad still syncs after the new pairing.
11. **Old layout**: every crossing behaves as yesterday.
12. **Free layout**: drag the phone next to the iPad on the Desk. Cross PC → iPad → phone → iPad → PC by pushing at edges. Push at the phone's far edge: it stays.
13. **One-step files pairing**: Add a device → iPad → step 2 → "Put the code on the iPad", then on the iPad Bridge → Pair → Paste code.
14. **Phone ↔ iPad**: on the iPad, Pair → "Show my code for another device"; scan it with the phone's Bridge. Copy on the phone (tile): the iPad (Bridge open) shows "Clipboard from S24+". Send clipboard on the iPad: the phone gets it. The PC should not show it twice.
15. **Proof 4**: during a FaceTime call on the iPad, try 5 and 6.

## What was skipped, and why

- **Android build**: no Android SDK in the cloud. The Kotlin change is small (the tile calls `shareClip`).
- **Android "show my code" screen**: not built; only the iPad shows a code for now.
- **To-scale free-drag Desk editor**: replaced by the lighter "drop beside a device" (call 15).
- **Steps 3, 5, 6, 7, 9 beyond designs**: need devices (3, 6, 7), a signed driver (7), a parser change shipped to every device first (5), or a relay in CI (9). Building them blind would mean shipping unverifiable code as finished.
- **Visual check of the hub**: no screenshots; the hub logic is unit-tested, the drawing is not.

## Open questions

- **Proof 4** (a call during a session): test 15 settles it; with no stream it may not matter.
- **German layout**: call 10.
- **Bluetooth crash**: needs your recovery; with Bluetooth off nothing touches the adapter now, which also tests whether Bridge's advertising caused it.
- **Function keys** on the iPad: unproven.

## New ideas

For studying across devices, copying a lot and typing everywhere:

1. **Clipboard history with search**, on the PC and the iPad: the last 50 copies (text and images) from any device, pinned items for formulas or citations you reuse. Stored locally, never logged.
2. **Keep formatting when copying**: rich text and HTML alongside plain text, so a copy from a PDF or web page into notes keeps bold, lists and links.
3. **Send to the device under the pointer**: a hotkey that pastes the clipboard on whichever device you are controlling, or drops the selected file there.
4. **Drop a file onto a device on the Desk** to send it.
5. **Screenshot to clipboard everywhere**: a PC hotkey that takes a screenshot of the iPad (the spike's screen capture already works over the tunnel) and puts it on the PC clipboard; great for lecture slides on the iPad.
6. **Focus mode**: one switch that silences notifications on the phone while you study on the PC and iPad (needs the phone's Do Not Disturb permission).
7. **Quick notes inbox**: text you send from the phone lands in one running note on the PC instead of only the clipboard, so nothing is lost when the next copy overwrites it.
8. **Latency number**: a camera test (slow-motion video of a keypress and the iPad screen) to measure real input lag and tune the 8 ms pacing.
9. **Smoother scroll**: touchpad sub-notch scrolling mapped to smaller iPad scroll steps for reading long pages.
10. **Per-device keyboard layout**: German on the PC, translated for the iPad and phone automatically.
11. **Battery and link on the Desk**: the iPad's and phone's battery and link quality next to each device.
12. **Bluetooth fallback**: once the adapter is stable again, use Bluetooth automatically when the iPad's Wi-Fi route fails (an iPadOS update can break the private interface).
