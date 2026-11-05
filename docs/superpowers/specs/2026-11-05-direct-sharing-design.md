# Direct sharing between all devices

Step 8 of the roadmap. Clipboard and files go device to device, not through Windows first.

## What already worked

The core node is symmetric: any node can show an invite, any node can join one, and paired nodes send files and clips to each other over iroh (LAN first, relay when needed). Only the PC ever showed an invite, so phones and iPads could pair only with the PC.

## Design

- **Pairing between any two devices.** `FfiNode.invite()` is exported, so the iPad (and later Android) can show a QR code; the other device scans it with the existing scanner. One code, 5 minutes, works once, as on the PC.
- **Clipboard mesh, no hub.** The device where something was copied sends it to every paired device itself (`Node::share_clip`, `FfiNode.shareClip`). Receivers never forward. `core::clipmesh::ClipMesh` drops a clip seen in the last 10 s, which covers both the PC's hub relaying what a phone already sent straight to the iPad, and a received clip coming back as a "local change" when it is written to the clipboard. Only a hash and a time are kept, never the text.
- **Files**: already direct once paired (`send_files`). Nothing new in core.
- **The PC is unchanged**: it still syncs the iPad's pasteboard through the CoreDevice service and sends Windows text to phones. Its incoming clips pass through the same duplicate filter.

## Calls (owner to check)

1. *10-second repeat window.* Copying the same text again within 10 s on any device does not resend it.
2. *Unreachable devices are skipped, not queued.* The next copy anywhere wins, as on the PC.
3. *Text only between phones and iPads*, as phones already get text only. Images directly between them are a later step.
4. *The iPad writes received clips to its pasteboard while Bridge is open*: iOS does not let an app read or write the clipboard in the background for long, so with Bridge closed on the iPad, a phone's copy reaches the iPad only through the PC.
5. *Send clipboard on the iPad and the Android tile now go to every paired device*, not only the PC.

## Built

- `core/src/clipmesh.rs` with tests; `Node::share_clip`; the filter on incoming clips; `FfiNode.invite`, `FfiNode.shareClip`.
- `core/tests/node.rs::phone_and_ipad_share_the_clipboard_directly`: a phone shows the code, an iPad joins, the clipboard goes both ways with no PC, and the echo does not come back.
- iPad app: "Show my code for another device" in the pairing sheet (Core Image QR), received clips applied to the pasteboard, Send clipboard to every paired device.
- Android: the clipboard tile shares with every paired device. **Unbuilt**: no Android SDK in the cloud session.

## Not built

- Android "show my code" screen (it has zxing for scanning; showing a code needs a small screen).
- A Desk view of which phones and iPads are paired with each other (the PC cannot see pairings it is not part of).
