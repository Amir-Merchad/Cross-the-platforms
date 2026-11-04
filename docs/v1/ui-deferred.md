# Base UI: rulings and deferred minors

Kept from the plan ledger when it closed on 2026-11-04. Device checks in `docs/v1/checklist-ui.md` were still open, and the owner moved on to v1.5 while using the app daily.

## Rulings (with their cost if wrong)

- Task 1: Ruling: reviewer's Failed-before-Started point is covered by T3 (`Failed` with unknown id adds a "A file" row) — no core change — cost if wrong: one invisible failure
- Task 1: Ruling: sender success ends a row on final Progress (done==total) per T3; pop-fail break stays silent — disk error, batch resends — cost if wrong: duplicate file after a disk failure
- Task 3: Ruling: spring constants k=0.16 f=0.66 instead of the plan defaults 0.13/0.74 — the plan values overshoot 21% and the spec forbids wobble — cost if wrong: one constant
- Task 3: Ruling: reviewer minors 1 and 2 touch Review Focus 5, so T4 fixes them (total 0 ends a row; Failed only changes moving rows) — small and on the plan's stated focus — cost if wrong: none
- Task 4: Ruling: tray tooltip is "Bridge: <status>" instead of the plan's plain "Bridge" — the live status there is useful — cost if wrong: one string
- Task 5: Ruling: re-graded reviewer minors 6 (hold race, Review Focus 1) and 7 (frame-rate spring, spec's 300 ms) to Important and fixed them — both change what the user gets — cost if wrong: small diff
- Task 5: Ruling: an edge closes only when neighbours cover all of it, matching layout::entry at the cursor — cost if wrong: one line
- Task 6: Ruling: supersedes the pre-flight note: T6 keeps Transfer.tsx as a hidden source for pairing and T7 deletes it — pairing never disappears between tasks — cost if wrong: none
- Task 7: Ruling: re-graded the load-race wrong link and ambiguous name links to Important and fixed them — they write wrong settings silently — cost if wrong: none
- Task 8: Ruling: owner said "it looks amazing" and gave no row results; row 1 recorded PASS, rows 2-25 left blank and carried to the final message — not claiming untested behaviour — cost if wrong: functional bugs found later
- Task 9: Ruling: Home gets a PC chooser row when several PCs are paired, and Send clipboard on Home goes to the shown PC only; the tile keeps send-to-all — restores what old per-PC cards allowed — cost if wrong: one small row
- Task 9: Ruling: phone install and check move to one combined device session with iPad after Task 10 — saves the owner a round — cost if wrong: Android device bugs found later
- Ruling: Task 11 and the final review run before the device session, so the owner sideloads the final build once — cost if wrong: device bugs found after the review
- Final: Ruling: status.notice stays out of the hub — engine text can be raw and the tray still shows it — cost if wrong: a hotkey failure gives no hub message
- Final: Ruling: Switch is enabled for a missingScreen device — the engine, hotkey and tray still switch, and the spec disables it only for unplaced — cost if wrong: one condition
- Final: Ruling: iPad Share extension keeps its current look in this round — it is brief and each iPad round is costly; spec item left open for later — cost if wrong: one screen off-style
- Final: Ruling: iPad retries only on foreground or after pairing, forgotten peers' outbox on disk, drag stuck after hotkey mid-drag, tray overlap at minimum size: stand as they are — pre-existing or rare — cost if wrong: small
- Final: Ruling: receive_on renames .part after the ack, so a rename failure on the receiver still reads Sent — needs a wire-order change, one fix wave only, disk failure is rare — cost if wrong: a file reported sent is missing after a disk error
- Final: Ruling: engine.rs:139 and clipsync.rs:154 keep raw error text in status.notice, tray only, no device ids — cost if wrong: a technical line in the tray

## Deferred minors

- Task 1: minor (deferred): Started precedes Failed for direct sends that fail at the offer (UI handles it)
- Task 2: minor (deferred): forget leaves peer in `heard` up to 150 s (UI looks up paired peers only); no test for the receive_dir guard (needs AppHandle); long Inner initializer line
- Task 3: minor (deferred): Received matched to oldest moving row (no id on Received); hosts missing from status not shown; displayName not truncated (CSS); screenBoxes zero-size monitor
- Task 4: minor (deferred): only default Button has the lip; no .catch on initial loads; A's error is hidden when a later save succeeds; TopBar hand-rolls the lip
- Task 5: minor (deferred): say() outranks a control change for 8 s; long unbroken name in hint and idle lines; status line defaults to first device; no automated order check for switchTo before save; saved attachments on closed edges left as they are
- Task 6: minor (deferred): Tune imported from DevicePanel by Settings; dead data-[hidden] classes; ui/alert, badge, card unused; canSendAgain device:unknown; generic send failure line
- Task 7: minor (deferred): focus not moved on step change; StrictMode double code in dev; Add files on a Bluetooth-only device defaults to iPad; colour not carried over on link; Looking for your devices stays if pairedPeers keeps failing
- Task 9: minor (deferred): no test for the hold gesture; TalkBack forget action has no hold; failed clipboard send leaves no row; history never pruned; camera permission read once; Paste can double-join; 44dp targets; Typography rebuilt per recomposition
- Task 10: minor (deferred): success notices use the warning card and never clear; failed with unknown id makes an empty-peer row; declined drives "PC not reachable"; font weights unverified until sideload
- Final: minor (deferred): multi-file hub rows show first name; cold-start Send to loses its row; Android toasts use raw peer name; Requeued in success green; linkChoices can offer a Windows peer; say() outranks control change; colour not carried on link
- Final: minor (deferred): newHost misses a re-added host auto-linked by name during the guide; id sort can shift default colours; placed host with no link reads Unnamed device while Bluetooth is off
