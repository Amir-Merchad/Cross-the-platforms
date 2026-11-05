# Free screen layout

Step 2 of `2026-11-05-v1.5-v2-roadmap.md`. Owner's request, from the overnight brief: devices sit anywhere on the Desk, as in Windows display settings, beside each other, stacked, or next to other devices, with no limit of four.

## Model

- **One desktop space**, in Windows virtual-screen pixels. Monitors keep the rectangles Windows gives them. Each device gets a rectangle too: `Placement { device_id, rect }` in a new `Settings.layout` list. List order is hotkey order, as `attachments` order is today.
- **Size.** A device's rectangle is its screen in desk pixels: width from `remote_width` (pointer units, iPad Air 13" about 1366), height from a new per-device aspect, default 4:3 (iPad landscape). The owner can resize in Fine tuning later. *(Call: 4:3 for every device until a device reports its size. A phone in portrait is wrong by default.)*
- **Adjacency** comes from touching edges: two rectangles touch when one's right equals the other's left (or top and bottom) and their spans overlap. Nothing else connects screens. The Desk snaps a dragged device to the nearest edge within 24 px.
- **Valid layouts**: each device once, devices never overlap each other or a monitor. Saving an invalid layout fails, as `validate` does now.
- **Migration**: an older file with `attachments` and no `layout` gets one placement per attachment, centred on that monitor edge, touching it. `attachments` is kept, so the old build still reads the file. *(Call: kept rather than deleted, for going back one version.)*

## Crossing

- **Windows to a device**: unchanged rule, generalised. The cursor pushes against a monitor edge, a device rectangle touches that edge at the cursor's position, no monitor lies beyond it there, and the push lasts `entry_hold_ms`.
- **Inside a device**: the remote pointer cannot be read, so its position is estimated from sent motion (`gain`, clamped to the rectangle), on both axes now. On entry the pointer is pinned against the entry edge as today; the other axis starts at the entry point. *(Call: the perpendicular estimate drifts with the device's pointer acceleration. Crossing at a far edge is "about there", not exact. Fine tuning's gain knob is the fix.)*
- **Leaving a device**: a deliberate push against any edge, with today's rules: no return in the first second, a dwell, a minimum push, a first flick not counted. What touches that edge at the estimated point decides: a monitor returns to Windows and places the cursor there; another device is entered directly, with no hold; nothing keeps the pointer there.
- Esc three times and Ctrl+Alt+Q always return to Windows. Hotkeys 1 to 9 still jump to the n-th device.

## Not in this step

- The Desk editor for free placement (drag anywhere, snap, resize). The engine and settings work first, with migration, so the old edge editor keeps working on top of `attachments` until the editor lands. Planned as Task 4 below.
- Devices controlling each other (step 5), and screens of devices mirrored (step 6).

## Done means

- An old settings file gives the same crossings as before.
- With a layout that puts the phone to the right of the iPad, the pointer goes PC → iPad → phone → iPad → PC by pushing at edges.
- A device with nothing beyond an edge keeps the pointer there.
- Tests cover adjacency, migration, validation, entry, device-to-device crossing and return.
