# iPad pointer with no Bluetooth

2026-11-04, iPad Air 13" M2, iPadOS 27.2, disk image 27A5228h, idevice 0.1.68. Tool: `spikes/ipad-input`.

**Verdict: GO over USB. Not tested over Wi-Fi.** A virtual mouse moved the real iPad pointer corner to corner, hovered, and its click opened Files. No Bluetooth was involved.

## The report

Create a mouse service with `createService`, keep a display stream running, and send report ID 5 with `send`:

```
05 dx dy 00 00 00 00 00 00 00 00 buttons 00 00 00 00 00 00 00      19 bytes
```

`dx` and `dy` are signed 8-bit. `buttons` is a bitmask, bit 0 is the primary click. 17 bytes is the shortest length that decodes. At 16 units per report every 8 ms the pointer moves about 18 px, with iPadOS acceleration on top. Screen space is 2732 x 2048 pixels in landscape.

## Why every earlier stage failed

`dtuhidd` does not decode reports from the descriptor. It holds a fixed list of UniversalHID gestures and picks one by report ID (disassembly of `usr/libexec/dtuhidd`, from `Image.dmg`). All earlier stages sent ID 1, which is `KeyboardReport`. It needs at least 31 bytes, so the daemon logged `Failed to decode report for gesture` and dropped them. A `DeviceUsagePairs` mouse is still enough for the system to add a pointer device, which is why a pointer could appear but never move.

Surface 1281 `touchscreenGesture` is itself a mouse, with `_CoreDevice_suppressMousePointer` set, so stages B and C could never show a pointer.

## Report IDs

From a sweep of every ID and length, zero-filled, judged by `dtuhidd` log lines. Names in quotes are inferred.

| ID | Shortest length | Meaning |
|---|---|---|
| 01 | 31 | Keyboard |
| 02 | 9 | Consumer: backboardd logged `page:0xC`, usage from the payload. Do not fuzz, it presses keys |
| 03 | 3 | unknown |
| 04 | 5 | unknown |
| 05 | 17 | **Pointer, proven** |
| 07 | 13 | "Scroll" (three 32-bit values) |
| 09 | 40 | "Digitizer" |
| 0B, 0C, 0D, 13 | 19 | unknown, 19-byte family |
| 0E | 2 | unknown |
| 0F, 10 | 4 | unknown |
| 11 | 6 | unknown |
| 12 | 38 | unknown |
| 81 | 6 | unknown |

Any other ID logs `No gesture for report ID`. The daemon links gestures for absolute pointer, scroll, dock swipe, navigation swipe, translation, fluid touch, scale, rotation and zoom toggle, so several of the unknown IDs are probably these.

## Routes

| Route | Sent | Evidence | Result |
|---|---|---|---|
| Screenshots (step 1) | `ScreenCaptureServiceClient` during a stream, about 2 s per PNG | Clock and swipe show up; the pointer shows up once it has moved | **Works**, and includes the pointer. Capture is a 2048 x 2732 buffer: landscape screen at y offset 684, right 684 px cropped |
| iPad system log (added) | `os_trace_relay` over USB | Debug lines from `dtuhidd` and `backboardd`, including `Pointer x,y (global) buttonMask:n` | **Works**, the best oracle here, but lags by a second or more and misses lines |
| E to L, M to P | ID 1 reports | Stage `e-id1`: 0 px diff, no reply. ID-1 reports under 31 bytes fail to decode | **Fail**, explained above. Not rerun one by one; the log explains all stages with short ID-1 reports. Why M to P showed no pointer at all was not investigated |
| Replies to reports | `send` with reply requested | Nothing within 500 ms | The iPad does not answer reports |
| Connected services after create | `connectedServices` | Service is listed, 6 services | Works, proves nothing about delivery |
| 3a Button click | Report 05 with `buttons = 1` over the Files icon | `button number:1 down:YES`, then `NO`; Files opened, 3.1 M px changed | **Pass** |
| Motion | Report 05, signed `dx`, `dy` | Logged range x 1 to 2730, y 9 to 2046, both directions; pointer drawn in shots where it was sent | **Pass** |
| 3d Report format | ID 5, and every other ID and length | Table above | **Pass** with ID 5. The service used the old ID-1 descriptor, so the descriptor need not match the report |
| Report 13 on own service and on 1281, pointer up | `r13` stages | Diffs were only the Spotlight caret and an indicator | No pointer effect seen; its gesture is unknown |
| 3b Device properties | Not sent | | Not needed, stopped when the pointer moved |
| 3c Virtual trackpad | Not sent | | Not needed |
| 3e Other Indigo events | Not sent beyond stage D | `dtuhidd` routes Indigo vendor events to `avpCustom` and has no Indigo pointer event | Not needed |
| 3f AssistiveTouch Mouse Keys | Not sent | | Not needed; no `mousekeys` command was built |
| Wi-Fi tunnel | `demo wifi` | `iPad not found on Wi-Fi` twice, iPad awake; PC on Ethernet at 192.168.1.4 | **Not tested**: the Bonjour lookup found nothing, so no report was sent. Cause unknown |

## Evidence

Local only, not in the repo, because the full shots show personal notifications, calendar and file names.

- `C:\dev\_ref\tmp\pointer\shots\demo-2clicked.png`: Files open after the virtual click, pointer visible at bottom right.
- `C:\dev\_ref\tmp\pointer\shots\demo-1hover.png`, `demo-waypoint0.png` to `demo-waypoint2.png`: pointer at each destination.
- `C:\dev\_ref\tmp\pointer\demo-waypoint1-crop.png`: crop of the pointer over an icon, no personal content.
- `C:\dev\_ref\tmp\pointer\demo-trace.log`, `corner-to-corner-trace.log`, `first-motion.log`: `backboardd` position lines.

## Costs and limits

- Needs the disk image mounted, Developer Mode on, and a display stream running, exactly like the keyboard route.
- Starting the stream leaves a "This device was viewed remotely from Mac" notification on the iPad.
- Reports sent after a long idle can be lost or unlogged at first. Seen several times, cause not established.
- Touching the top edge pulls down Notification Centre, as a real mouse would.
- Only one service, one speed and click tested. Scroll, right click and drag are untested. Latency to the screen was not measured.

## To reproduce

With Apple Devices open and the iPad unlocked on USB: `cargo run -p ipad-input -- demo mytag --usb`. It opens Files, then presses Home.
