# Morning report, 2026-11-05

Branch `v1.5/input-27ztiw` (the cloud session's designated branch; `v1.5/input` is untouched). Nothing merged, no PR, no release. **Nothing here has run on a device.** The cloud has no iPad, phone, Windows desktop or Bluetooth, so every device claim below is "should", not "does".

## In one minute

- **iPad keyboard and mouse over Wi-Fi, Tasks 2 to 6 of the plan: built.** Your keyboard and mouse should reach the iPad through the same kind of tunnel the clipboard uses, with no Bluetooth and no iPad app. Per the proofs, no display stream is used, so there should be no "viewed remotely" notification and no 1.3 MB/s of video.
- **Bluetooth is now optional.** Capture, hotkeys, Esc ×3 and the iPad start at once, even with the adapter crashed. A new Settings switch turns Bluetooth off completely.
- **Free screen layout, step 2: engine half built.** Devices can sit beside devices (PC → iPad → phone), with no limit of four. The Desk editor to drag them anywhere is **not built yet**; until it is, the existing edge editor keeps working and old settings cross exactly as before.
- Steps 3 to 9 (Android Wi-Fi, one-step pairing, control from any device, mirroring, extension, direct sharing, internet): **not started tonight.** See "What is left" for my suggested order, given how you use the app.

## What was built

| Commit | What |
|---|---|
| `8b34e72` | **Pairing, mount, discovery** (`platform/windows/src/ipad.rs`). Every Pair over USB uses a name the iPad has never seen (`cross-the-platforms-<time>`) and saves the name beside the record in `rp-pairing-host.txt`, which fixes the "bad signature" that broke Bridge's Wi-Fi. Until you pair again, the spike's working record `rp-pairing-spike.plist` is used. USB mounts the disk image through lockdown; Wi-Fi mounts it through the tunnel (proof 6). Discovery tries the saved address, then a multicast browse, then asks every address on your /24, 32 at a time. |
| `29ab6a7` | **Reports and keys** (`core/src/ipadhid.rs`). Report 5 for motion and buttons (bit 1 is right click), split into signed 8-bit steps with the buttons on every step so drags hold. Report 7 for scroll, 16 units per wheel notch. A translator turns held-key snapshots into key down and key up, modifiers as `0xE0` to `0xE7`, and releases everything on leave. Settings gain `bluetooth_enabled` and `retired_hosts`. |
| `16eb7f7` | **iPad input service** (`platform/windows/src/ipadinput.rs`). Own thread; the engine never waits on it. Opens the tunnel, mounts the disk image if needed, creates the virtual mouse and keyboard, and checks the link with a round trip every 500 ms. Two misses, a write error or a 500 ms write stall mark it down. Queued motion is summed while the link is busy; clicks and keys are never merged away. Reconnects after 2 s, backing off to 30 s. Also `ipad-input-check`, a small binary for testing on the device. |
| `244384f` | **Engine routing, optional Bluetooth** (`engine.rs`). `ipad-wifi` reports go to the service; Enter and Leave follow the active device. The iPad is "connected" only while its link is Ready, so a drop returns input to Windows through the router's existing tick. Bluetooth starts in the background and only when switched on. |
| `dd6af64` | **Hub.** The iPad moves from its Bluetooth host to `ipad-wifi` once (same place, hotkey, files link and colour; calibration starts fresh) and the old host is hidden. Plain status lines. Settings → Bluetooth switch. Add a device → iPad is now two steps: USB pairing (keyboard, mouse and clipboard), then files. |
| `13d3fb2` | Fixes from my own review: Bluetooth off now drops its hosts (input returns if one had it); a reconnect removes a virtual mouse a dead session left behind, so a held button cannot stay stuck. |
| `57131db` | **Free layout** (`core/src/space.rs`, `router.rs`). Monitors and devices as rectangles in one space, joined where edges touch. The pointer is estimated on both axes, and a deliberate push at any device edge goes to Windows, into the next device, or nowhere. |

## CI

The `core` workflow (Windows runner: UI tests and build, `cargo test -p bridge-core -p ctp-windows`, clippy with `-D warnings` on `bridge-core`, `ctp-windows`, `ctp-app`) is the compiler for the Windows crates.

| Run | Commit | Result |
|---|---|---|
| core 79 | `29ab6a7` Tasks 2 and 3 | **success** |
| core 80, 81 | `244384f`, `dd6af64` | cancelled by me: superseded by 82 |
| core 82 | `13d3fb2` all iPad work | see the update at the end |
| core 83 | `57131db` free layout | see the update at the end |
| ios 18 | `13d3fb2` | started by hand (the workflow only runs by itself on `v1/**`). See the update at the end |

Locally: `cargo test -p bridge-core --lib`: 145 pass. `pnpm test`: 144 pass; `tsc` is clean. Windows crates: `cargo clippy --target x86_64-pc-windows-gnu -D warnings` on `ctp-windows` (all targets) and `ctp-app`: clean. That cross-check compiles but cannot run Windows tests; CI runs them. Two older integration tests in `core/tests/node.rs` (`pair_then_restart_keeps_pairing`, `queue_survives_a_pc_restart`) time out in this cloud container. They are network tests, I did not touch that code, and CI does not run them.

## What needs a device

Everything that talks to the iPad or Windows input:

- Whether the service connects over Wi-Fi with the spike's record, then with a new Pair over USB.
- Pointer feel at 8 ms pacing, scroll speed (16 per notch is a guess from proof 1), right click, drag.
- Keys through `main_keyboard_key_down`/`up`, including held modifiers and release on leave.
- That a link drop returns input to Windows within about a second (two 250 ms misses on a 500 ms beat: roughly 0.75 to 1.25 s).
- Whether the iPad removes our virtual keyboard and mouse by itself when a tunnel dies. If it does not, a key held during a Wi-Fi drop may repeat on the iPad until you press and release it. The stale mouse is removed by name on reconnect; the keyboard is not, because idevice does not expose its name.
- Bluetooth off at launch: hooks, hotkeys, Esc ×3 and the iPad should all work.
- Free layout crossings on real screens, and the far-edge estimate on the iPad.

## Calls I made (please check)

1. **Spike record as a bridge.** Until you pair again, the iPad input and the clipboard use `rp-pairing-spike.plist` with its host name. Pairing again replaces `rp-pairing.plist` and wins from then on. The old record under the old name is still tried last, and its failure now says "pair again" instead of a raw error.
2. **Pairing name** is `cross-the-platforms-` plus the time in hex. Each pairing adds a peer on the iPad; old ones are not removed.
3. **No display stream at all**, per the proofs and the design header. If proof 4 or a first-use-after-boot case shows the iPad needs one, the stream code is still in the spike to port.
4. **Keepalive** is a `connectedServices` round trip every 500 ms with a 250 ms limit; two misses in a row mark the link down.
5. **Scroll** sends 16 units per wheel notch and passes pan through with the same sign as the Bluetooth path. The pan direction is untested.
6. **Unknown errors** show "Connecting…" and keep retrying, rather than a raw error. They are written to `engine.log` (no keys, no clipboard text).
7. **The iPad's name** shows as "iPad" in status for the Wi-Fi host; the files peer's name still shows on the Desk once linked.
8. **Migration** keeps the place, hotkey, link and colour, and drops calibration on purpose (the Wi-Fi pointer behaves differently), as the design said.
9. **Check binary** lives in `platform/windows/src/bin/ipad-input-check.rs` rather than as a spike subcommand, so CI compiles it and it uses the real service.
10. **Free layout**: device height defaults to 4:3 of its width; old attachments become placements covering the whole monitor edge; a push into a device beside another device needs no hold (the hold is only for leaving Windows); `attachments` stays in the file so an older build still reads it.
11. **Bluetooth switch** applies live: off stops advertising and drops Bluetooth hosts; on starts the peripheral again in the background.
12. **German layout**: not done. The iPad treats the keyboard as US. Simplest fix, to try first: iPad Settings → General → Keyboard → Hardware Keyboard → German. If that is not offered, I will add a translator.

## Tests, in order

Quit Bridge from the tray before each rebuild. Build with `pnpm tauri build --debug --no-bundle` in `apps/tauri`.

1. **Service alone.** iPad unlocked on the same Wi-Fi, USB out, Bridge closed. Open Spotlight on the iPad. Run `cargo run -p ctp-windows --bin ipad-input-check`. Expect state `Ready` within about 5 s, then a square, a context menu, scrolling, `hello` typed, then `A` with nothing repeating.
2. **If it says NotSetUp or keeps failing verify:** open Bridge, Add a device → iPad → plug in, Pair over USB, tap Trust. Run step 1 again with the cable out.
3. **Bluetooth off at launch.** Settings → Bluetooth off, quit, start Bridge. Hotkeys, Esc ×3 and Ctrl+Alt+Q work; no Bluetooth banner.
4. **Desk.** The iPad shows once (not twice), in the same place, "Connected over Wi-Fi" in its panel.
5. **Normal use** with Bluetooth off: cross onto the iPad by the edge and by Ctrl+Alt+n. Move, click, right click, drag an icon, scroll, type with Shift, Cmd+A, arrows, Esc. Cross back by pushing.
6. **Wi-Fi drop.** While controlling the iPad, turn the iPad's Wi-Fi off. Input should come back to Windows within about a second, nothing stuck. Turn it on: the Desk should go back to Connected by itself within 30 s.
7. **Held key during a drop.** Hold Shift on the iPad, drop the Wi-Fi as in 6, reconnect, type: letters should be lowercase. If they are uppercase, the stale keyboard needs cleaning; tell me.
8. **iPad restart.** Restart the iPad, unlock once. Bridge should mount the disk image over Wi-Fi and reconnect with no USB.
9. **Sleep.** PC sleep and wake; iPad lock for 10 minutes and unlock. Both should reconnect without you doing anything.
10. **Clipboard** still syncs over Wi-Fi after the new pairing.
11. **Old layout unchanged.** With your existing Desk, every crossing behaves as yesterday.
12. **Proof 4.** During a FaceTime call on the iPad, try steps 5 and 6.

## Open questions

- **Proof 4** (a call during a session): open. With no stream it may not matter at all; test 12 settles it.
- **German layout**: call 12.
- **Bluetooth crash**: the adapter still needs your recovery. With Bluetooth off, nothing should touch it now, which also tests whether Bridge's advertising was what crashed it.
- **Function keys**: unproven on the iPad (proof 8). F5 to F10 were never sent.

## What is left

The brief's order was steps 3 to 9 after this. Since you mainly study across devices, copy content and type on every device, here is what I would do next, in order, and why:

1. **Desk editor for free layout** (step 2, Task 4). Small, finishes what the engine already does.
2. **Direct sharing, clipboard first** (step 8). Phone ↔ iPad copy without the PC on. Highest value for studying; the node already has keys and a relay.
3. **Android over Wi-Fi** (step 3). Removes Bluetooth from the phone too. Needs wireless debugging and a helper process, as scrcpy does; setup is fiddly, so it needs a careful guide.
4. **One-step pairing** (step 4). With the iPad on USB pairing plus files, and Android on Wi-Fi, fold each device into one guided flow.
5. **Control from any device** (step 5), then **mirroring** (6) and **extension** (7, needs a signed Windows virtual display driver), then **v2 internet** (9).

### Could be better (not done, suggestions)

- **Clipboard history and pinned snippets**: a short searchable history (text and images) on the PC and iPad, so a copy from earlier in a study session is not lost. Local only, never logged.
- **Clipboard formats**: rich text and HTML alongside plain text, so copying from a PDF or web page into notes keeps bold and lists; links as links.
- **Drop to send**: drag a file from Explorer onto a device on the Desk to send it, next to the existing Send to menu and the iPad share extension.
- **Transport**: the node runs on iroh (QUIC, hole punching, relay fallback), which is a good choice and already covers v2. For input, the iPad route is a private Apple interface; an iPadOS update can break it, so keeping Bluetooth as a fallback is worth it once the adapter is stable.
- **Latency to the screen** has never been measured. A camera test (high-speed phone video of the PC keyboard and the iPad screen) would give the real number and show whether the 8 ms pacing should change.
- **Scroll**: precise scrolling from a touchpad (sub-notch) could map to smaller report 7 steps than 16 per notch, for smoother reading of long pages.
