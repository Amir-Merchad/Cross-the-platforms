# iPad pointer with no Bluetooth

2026-11-04, iPad Air 13" M2, iPadOS 27.2, disk image 27A5228h, idevice 0.1.68. Tool: `spikes/ipad-input`.

**Verdict: GO over USB and Wi-Fi.** A virtual mouse moved the real iPad pointer corner to corner, hovered, and its click opened Files, on both links. Touch and the virtual keyboard also work over Wi-Fi. No Bluetooth was involved. Wi-Fi was not tested with the USB cable pulled out; see [Wi-Fi](#wi-fi).

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
| Wi-Fi tunnel | `check`, `demo wifi`, `typed`, `latency pointer` | Below | **Pass** with a new pairing record |

## Wi-Fi

2026-11-04, PC Wi-Fi 192.168.1.3, iPad 192.168.1.5, USB cable left in for the log.

**Why pair verify failed.** `early eof` was the second failure, not the first. `remotepairingdeviced` logged, for the record in `rp-pairing.plist`:

```
Found paired peer matching query
### PairVerify server bad signature: -146/0xFFFFFF6E
```

So the iPad holds a different public key for that identifier than the record has. `RemotePairingClient::connect` then falls back to pair setup on the same socket. The Wi-Fi handshake says `allowsPairSetup: false`, so the iPad closes the connection, and that is the eof. Running `attempt_pair_verify` and `validate_pairing` separately shows the real error, `PairVerifyFailed`.

**Why the key differs.** `RpPairingFile::generate` makes the identifier a v3 UUID of the host name, so every pairing as `cross-the-platforms` is peer `17372F0B`, whose key was set by the first pairing. Nov 3 worked because that record was the first. Later pairings under the same name, such as Bridge's Pair over USB at 23:30, get a fresh key and report success, but that key does not verify. Inferred from the log and the code: pairing twice under one name was not run as a controlled test, because each try costs a Trust tap and another peer on the iPad. The handshake itself is the same as Nov 3 in `ipad-clip`.

**Fix in the spike.** `ipad-input rppair` pairs over USB as `cross-the-platforms-input`, a new identifier, into `rp-pairing-spike.plist`. It refuses to overwrite a record. The iPad showed Trust This Computer once and it was tapped. The spike prefers that file when it exists. Verify then reached `PairVerify server done`. Bridge still uses `rp-pairing.plist`, so **Bridge's own Wi-Fi is still broken**, and it falls back to USB (`wifi=false`).

| Check over Wi-Fi | Evidence | Result |
|---|---|---|
| `check` | Tunnel up, 5 surfaces, 1229 media packets in 3 s | **Pass** |
| Motion | `demo wifi`: 755 `backboardd` positions, x 1 to 1963, y 111 to 2046, pointer in the waypoint shots | **Pass** |
| Click | `button number:1 down:YES`, then `NO`; Files opened, 3.1 M px changed; Home returned afterwards | **Pass** |
| Touch | `typed`: a drag opened Today View, then Spotlight. Touch axes are portrait: landscape down is touch X | **Pass** |
| Keyboard | `typed`: `hello bridge` appears in Spotlight (virtual keyboard route only) | **Pass** |
| Cleanup | After the runs: 5 surfaces, virtual services gone, stream stopped, Home | **Pass** |

**Is it really Wi-Fi?** During a session the process had one link to the iPad, TCP 192.168.1.3 to 192.168.1.5:63478. The Wi-Fi adapter moved 27 MB in and 4.4 MB out over about 20 s and the Ethernet counters did not change. Input goes through that tunnel, and `check` and `latency` do not open usbmux at all. USB only read the system log. Not proven by unplugging: nobody could pull the cable during this run.

**Latency**, `latency pointer`, 200 reports then 100 round trips, display stream running. The round trip is `connectedServices` to the HID daemon and back.

| Link | Send median | Round trip min | Median | p95 | Max |
|---|---|---|---|---|---|
| Wi-Fi, TCP_NODELAY | 0.18 ms | 4.1 ms | 8.1 ms | 11.0 ms | 17.5 ms |
| Wi-Fi, Nagle on | 0.18 ms | 5.3 ms | 8.0 ms | 15.4 ms | 26.5 ms |
| USB | 0.18 ms | 2.8 ms | 5.3 ms | 7.6 ms | 28.5 ms |

One run each, so the p95 and max gaps are weak. `ping` beside it ranged from 3 to 92 ms, so expect worse spikes than 100 samples show. Latency to the screen is still not measured: the log has whole-second timestamps here.

**Discovery.** The multicast browse still finds nothing on this PC, cause unknown (Hyper-V and VMware adapters are suspected, not shown). The spike now asks `ipad-last-ip.txt`, beside the record, with a unicast mDNS query to port 5353, and browses only if that fails. All runs without `IPAD_ADDR` found the iPad this way. The first run on a PC with no saved address still needs `IPAD_ADDR` or a working browse.

## Evidence

Local only, not in the repo, because the full shots show personal notifications, calendar and file names.

- `C:\dev\_ref\tmp\pointer\shots\demo-2clicked.png`: Files open after the virtual click, pointer visible at bottom right.
- `C:\dev\_ref\tmp\pointer\shots\demo-1hover.png`, `demo-waypoint0.png` to `demo-waypoint2.png`: pointer at each destination.
- `C:\dev\_ref\tmp\pointer\demo-waypoint1-crop.png`: crop of the pointer over an icon, no personal content.
- `C:\dev\_ref\tmp\pointer\demo-trace.log`, `corner-to-corner-trace.log`, `first-motion.log`: `backboardd` position lines.
- `C:\dev\_ref\tmp\pointer\wifi-first.log`: full iPad log around the bad signature. `wifi-rppair.log`: the new pairing. `wifi-second.log`: verify passing.
- `C:\dev\_ref\tmp\pointer\wifi-trace.log`, `shots\wifi-*.png`, `shots\wifi2-typed.png`: Wi-Fi demo and typing.

## Costs and limits

- Needs the disk image mounted, Developer Mode on, and a display stream running, exactly like the keyboard route.
- Starting the stream leaves a "This device was viewed remotely from Mac" notification on the iPad.
- Reports sent after a long idle can be lost or unlogged at first. Seen several times, cause not established.
- Touching the top edge pulls down Notification Centre, as a real mouse would.
- Only one service, one speed and click tested. Scroll, right click and drag are untested. Latency to the screen was not measured.
- Wi-Fi was not tested with the iPad unplugged, on battery, or after it slept.

## To reproduce

With Apple Devices open and the iPad unlocked on USB: `cargo run -p ipad-input -- demo mytag --usb`. It opens Files, then presses Home.

Over Wi-Fi, once: `cargo run -p ipad-input -- rppair` over USB and tap Trust. Then `IPAD_ADDR=<ip>:<port> cargo run -p ipad-input -- demo mytag`, with the port from the `_remotepairing._tcp` SRV record. Later runs do not need `IPAD_ADDR`. Without USB the demo still runs, but the log tap fails, so only screenshots are left as proof.
