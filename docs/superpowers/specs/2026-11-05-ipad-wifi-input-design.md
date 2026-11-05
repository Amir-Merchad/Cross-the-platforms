# iPad keyboard and mouse over Wi-Fi

> **Results from the proofs override this document.** See `docs/spikes/ipad-pointer.md`. In short: no display stream is needed, so remove stream handling and the stream lifetime rule. Disk image mounts over Wi-Fi through the tunnel, so "Plug in the iPad once" is only a fallback. Two tunnels work. Scroll is report 7: `07 00 00 x y` plus 8 zeros, signed 8-bit, about +-16 per notch. Right click is `buttons` bit 1. Keep the 500 ms keepalive. Function keys are unproven. The iPad layout is US until set to German on the iPad; if the Hardware Keyboard setting is not offered, add a translator.

Step 1 of `docs/superpowers/specs/2026-11-05-v1.5-v2-roadmap.md`. Facts and measurements: `docs/spikes/ipad-pointer.md`. Working code to port: `spikes/ipad-input`.

## Why

The owner's Bluetooth adapter crashes and vanishes, and recovery has needed a BIOS reset. The owner will not keep the iPad pointer on Bluetooth. A spike proved the real iPad pointer, click, keyboard and touch work over Wi-Fi with no Bluetooth and no iPad app, at about 8 ms round trip. This design builds that into Bridge so crossing a screen edge onto the iPad works exactly as it does now.

## Goals

- Windows mouse and keyboard reach the iPad over Wi-Fi, with edge crossing, hotkeys and return unchanged.
- Move, left and right click, drag, scroll, and the full keyboard including modifiers.
- Bridge starts this by itself and reconnects after sleep or a network change.
- One setup for the iPad: a single USB step covers input and clipboard. This replaces Bluetooth pairing for the iPad.
- Bluetooth can be switched off entirely in Bridge.
- The clipboard's stale pairing failure goes away.

## Non-goals

- Android over Wi-Fi: step 3. Free layout: step 2. Showing the iPad screen on Windows: step 6.
- Extra hardware.
- Removing the Bluetooth code. The phone still uses it.

## Anti-goals

- Windows left without a working mouse or keyboard. A dropped link must return input to Windows quickly, and Esc three times must always work.
- Input feeling worse than it does over Bluetooth.
- The input thread waiting on the network.
- Silent failure. If the iPad cannot take input, the Desk says why and input stays on Windows.

## Constraints from the spike

- Needs Developer Mode, the disk image mounted, and a display stream running. Without a stream the iPad drops every event, and the video it sends must be read and thrown away.
- The stream cannot start during a call. Behaviour when a call starts mid-session is unknown.
- Starting the stream leaves a "viewed remotely from Mac" notification on the iPad. Accepted.
- The disk image unmounts on every iPad reboot. The mount worked over USB through lockdown and timed out over the tunnel. Over Wi-Fi it is untested by that route.
- `dtuhidd` ignores the descriptor and picks a fixed format by report ID. Pointer is report 5, 19 bytes: `05 dx dy`, 8 zeros, `buttons`, 7 zeros. `dx` and `dy` are signed 8-bit. Bit 0 of `buttons` is the primary click.
- Scroll is believed to be report 7 (three 32-bit values). Right click is believed to be button bit 1. Neither is proven.
- Reports get no reply, so delivery cannot be confirmed per report. Reports after a long idle can be lost at first.
- Each re-pair under the same name makes a new key that the iPad rejects (`bad signature`), so the pairing identifier must be new each time.
- Multicast discovery fails on this PC. Unicast mDNS to the last known IP works.
- This is a private Apple interface. An iPadOS update can break it.
- Proven with the USB cable out on 2026-11-04: pointer moved and a click opened Files, with Windows seeing no iPad on USB. Battery and sleep are untested.

## Before building: finish proving it

Extend the spike and settle these first. A failure changes the design, so report it before going on.

1. Scroll: report 7, in both axes, at a usable speed.
2. Right click, and drag with the button held.
3. Stream start time, from nothing to first accepted report. This decides stream lifetime below.
4. What happens if a call starts during a session, and when it ends.
5. Whether clipboard and input can each hold a tunnel at once.
6. Disk image mount over Wi-Fi through the lockdown route.
7. Idle behaviour: which keepalive stops the first reports being lost.
8. Keyboard: modifiers, Esc, arrows, function keys, a held key repeating, and German layout, through idevice's `main_keyboard_key_down` and `key_up`. Only lowercase letters and space are proven.

Plan these proofs as the first task, and write the build tasks after their results. If proof 5 fails, `ipadinput` owns the one tunnel and clipboard uses it through that service. If proof 6 fails, the iPad needs USB once after each reboot, and the Desk says so.

## Design

**Identity.** The Wi-Fi iPad is a host with the fixed id `ipad-wifi`, and Bridge supports one iPad. It appears in `Status.hosts`, named from the iPad, `connected` true when the link is ready. `layout`, `router` and hub code treat it as any other host.

**Migration.** The iPad's Bluetooth host is the `device_links` key whose peer has kind Ipad. If one exists, on first use its attachment moves to `ipad-wifi`, keeping position and hotkey order, and its link moves too. That host id goes in a new `retired_hosts` setting, which the hub leaves out of the device list, so one iPad never shows twice. With no such link, nothing migrates and the owner places the iPad on the Desk. Calibration does not carry over: `ipad-wifi` starts from defaults and is tuned in Fine tuning, because the iPad's pointer behaves differently from the Bluetooth path.

**A separate service.** New `platform/windows/src/ipadinput.rs`, on its own thread and single-threaded tokio runtime like `clipsync`, because `idevice` futures are not `Send`. It owns the tunnel, display stream, video discard, virtual mouse and keyboard, and keepalive.

**Engine.** `Action::Mouse` and `Action::Keyboard` for `ipad-wifi` go to that service through a non-blocking channel. Everything else still goes to the peripheral. The engine never awaits it. Mouse motion is summed while the channel is busy, so a slow link loses detail, not time. Buttons and keys are never dropped. The router's existing pacing and `split_delta` apply.

**Report building.** A pure function in `core` turns `MouseReport` into report 5, splitting motion to signed 8-bit steps. `KeyboardReport` is a snapshot of held keys, and the iPad takes key down and up events, so a small stateful translator in `core` compares snapshots and yields events, including modifiers as usages `0xE0` to `0xE7`. It also yields the releases for everything held, which is how leaving lets go of keys. Both are unit-tested. The raw 31-byte keyboard report is not needed.

**Stream lifetime.** If start time from proof 3 is under about 250 ms, the stream runs only while the iPad is in control. Otherwise it runs while the iPad is awake and placed, and stops after 5 idle minutes. *(Claude's call. It uses about 1.3 MB/s of Wi-Fi while running.)*

**Failure.** A keepalive round trip every 500 ms. Two misses, a write error or a stream error mark the link down. `connected` becomes false, and the router's existing `tick` returns input to Windows. Target: back on Windows within about a second. On leaving, all keys and buttons are released on the iPad.

**Status.** New hub states: "iPad is on a call", "Plug in the iPad once" (disk image after a reboot, if proof 6 fails), "iPad not found on Wi-Fi", "Turn on Developer Mode". Plain sentences on the Desk and device panel, per the base UI rules.

**Pairing.** `pair_usb` uses a new identifier for every pairing, so a re-pair always works. Clipboard and input share the one record. Add a device for iPad becomes: plug in by USB and tap Trust (input and clipboard), then scan the code for files. Bluetooth is no longer a step for the iPad. Developer Mode is checked and explained at the first step.

**Discovery.** Saved last IP by unicast mDNS first, then multicast, then a unicast sweep of the local subnet. The found address is saved.

**Bluetooth switch.** A new `bluetooth_enabled` setting, default on, with a Settings row. Off means the peripheral is never started, and the engine, hooks, router and `ipad-wifi` keep running. The engine therefore treats the peripheral as optional, and a failed start no longer blocks it. The Bluetooth banner shows only when Bluetooth is on and unavailable.

**Disk image.** Mount over lockdown when on USB. Mount over Wi-Fi if proof 6 passes. Otherwise show "Plug in the iPad once".

**Hub scope.** Targeted changes on the base UI, not a rebuild: new status text in the existing device rows and panel, the Settings row, the iPad steps in Add a device, and `retired_hosts` in `devices.ts`.

## Done means

- The owner crosses onto the iPad over Wi-Fi with Bluetooth off, and moves, clicks, right-clicks, drags, scrolls and types for a normal working session.
- Pulling the Wi-Fi mid-control returns input to Windows within about a second, with no stuck keys.
- A call shows the right message and input stays on Windows.
- After PC sleep and iPad sleep it reconnects with no action. After an iPad reboot it does too, or asks for USB once if proof 6 fails.
- Clipboard works over Wi-Fi after a re-pair.
- New logic has tests. The engine thread has no network wait.

## Left to the builder

Module split, channel type, exact keepalive call, descriptor for the virtual services, and how Status carries the new states.
