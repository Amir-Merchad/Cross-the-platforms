# Free Layout Implementation Plan

**Goal:** Devices sit anywhere in one desktop space, and the pointer crosses screen to device and device to device.

**Spec:** `docs/superpowers/specs/2026-11-05-free-layout-design.md`.

## Global Constraints

- Older settings files cross exactly as before: a layout made from `attachments` covers each monitor edge.
- Esc three times and Ctrl+Alt+Q always return to Windows. Hotkeys follow placement order.
- No overlap between devices or with monitors; each device once.

### Task 1: Space model — done (`core/src/space.rs`)

Placements, `touching`, `validate`, `effective` (made from attachments), `beside`, `entry`, `what_is_beyond`, and a two-axis `Tracker` with the edge tracker's push rules. 19 tests.

### Task 2: Router on the space — done (`core/src/router.rs`)

Entry from a monitor edge maps the cursor along the device edge. A deliberate push leaves by any edge: a monitor returns to Windows at that point, a connected device is entered directly (the old device is released, Windows never gets input in between), nothing keeps the pointer. Old router tests pass unchanged; new tests walk PC → iPad → phone → iPad → PC.

### Task 3: Settings and engine — done

`Settings.layout`, validated on load and save. `placed_ids()` gives hotkey order, watched hosts and hotkey count. `Command::Reload` boxes its settings.

### Task 4: Desk editor — done, in a lighter form

Built as an extension of today's map rather than a to-scale editor: drag a device next to any placed device to put it beside it (`apps/tauri/src/lib/space.ts`, `nearestDevice`, `deskPlan` hangs devices from their neighbour, chains included). The first such drop writes `layout` from the current edges, so nothing moves. Screen-edge moves keep the layout in step and refuse a drop that would overlap. Hotkeys, panel lines and the switch-back check follow placement order. Tests in `space.test.ts`, `desk.test.ts`, `devices.test.ts`.

Not built: a to-scale view with free dragging anywhere, resizing, and an aspect slider. *(Call: the beside-a-device drop covers stacking and chains, which is what the owner asked for, with much less risk to the working map.)*

### Task 5: Device check

- [ ] Old settings: every existing crossing behaves as before.
- [ ] iPad right of the PC, phone right of the iPad: walk across and back.
- [ ] Far-edge crossing on the iPad lands roughly where expected; tune gain if not.
