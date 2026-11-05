# Overnight brief: v1.5 and v2

Working order for an unattended session. The owner tests in the morning, so what matters is honest, working, reviewed code on `v1.5/input`, and a clear list of what needs a device.

## Limits of a cloud session

- No iPad, phone, Windows desktop or Bluetooth. Never claim device behaviour is proven.
- `platform/windows` and `apps/tauri/src-tauri` may not compile on Linux. Push to `origin/v1.5/input` and read the `core` workflow (Windows runner) and `ios` workflow as the compiler. `core` and `apps/tauri/src` tests run locally.
- Android: Gradle may be unavailable, so build if you can, otherwise say it is unbuilt.
- Push only to `v1.5/*` branches. No merge to `main`, no PR, no release, no publishing.
- Never commit `.idea/`, `.red.txt`, `apps/tauri/vite.mock.config.ts`. Never log file names, keys or clipboard text.

## Order

1. **Finish `docs/superpowers/plans/2026-11-05-ipad-wifi-input.md`, Tasks 2 to 6.** The design's header lists what the proofs changed: no stream. Task 7 is the owner's.
2. **Free screen layout.** Roadmap step 2, as below.
3. **Wi-Fi input for Android.** Roadmap step 3.
4. **One-step pairing.** Step 4. With the iPad on USB pairing plus files, and Android on Wi-Fi, reduce every device to one guided flow.
5. **Control from any device.** Step 5.
6. **Direct sharing between all devices.** Step 8. Moved ahead of screens because it needs no new media work.
7. **Screen mirroring,** then **extension.** Steps 6 and 7. Designs and plans first. Build only what can be checked without a device, and stop rather than guess.
8. **v2 internet.** Step 9. Design and plan, then the parts that can be tested with core integration tests.

For each step: write a design in `docs/superpowers/specs/`, a plan in `docs/superpowers/plans/`, build with tests, review, commit, push. Do not wait for approval overnight: the owner said to continue and will read them tomorrow. Mark every design choice as a call for the owner to check. Stop a step rather than ship something unverifiable as finished.

## Free screen layout (owner's request)

Devices sit anywhere on the Desk, as in Windows display settings: beside each other, stacked, or next to other devices, with no limit of four. Suggested model: each device has a rectangle in one shared desktop space. Adjacency comes from touching edges, with snapping when dragged. The pointer crosses screen to device and device to device. Older `attachments` migrate. Far-edge crossing needs the edge tracker on both axes. Keep Esc three times and Ctrl+Alt+Q.

## Morning report

End with one file, `docs/v1.5/morning.md`: what was built, CI results, what needs a device, every call made, and the test list in the order to run it. Open questions: proof 4 (a call during a session, now likely irrelevant without a stream), German layout, and the Bluetooth crash.
