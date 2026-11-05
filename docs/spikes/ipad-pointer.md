# iPad pointer with no Bluetooth

2026-11-04, iPad Air 13" M2, iPadOS 27.2, disk image 27A5228h, idevice 0.1.68. Tool: `spikes/ipad-input`.

**Verdict: GO over USB and Wi-Fi.** A virtual mouse moved the real iPad pointer corner to corner, hovered, and its click opened Files, on both links. Touch and the virtual keyboard also work over Wi-Fi. No Bluetooth was involved. Scroll, right click, drag and the keyboard are proven with the USB cable out; see [Proofs](#proofs-for-the-build). Calls and the mount after a restart are still open.

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
| 07 | 13 | **Scroll, proven**: `07 00 00 x y`, then 8 zeros. See [Proofs](#proofs-for-the-build) |
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

## Proofs for the build

2026-11-05, Wi-Fi, USB unplugged, iPad charging and unlocked, Bridge closed. No system log, so every result is judged from screenshots. Sizes are in buffer pixels (see Routes). One run each unless stated.

| # | Command | Sent | Evidence | Result |
|---|---|---|---|---|
| 1 | `scroll` | Report 7 in Weather | Page moved 320 px for 10 reports of `y = -16` and back for `+16`; hourly strip moved 180 px for 30 of `x = -1` | **Pass** |
| 2 | `rightdrag` | Report 5, `buttons = 2` on Files; `buttons = 1` held over 60 motion reports | Context menu opened; Home Screen followed the pointer to page 2 | **Pass** |
| 3 | `streamtime` | Letters a to z, 50 ms apart, into Spotlight; one report after `createService` | All 26 letters landed in all 10 runs; pointer moved at every delay | **Pass**, under 250 ms with the tunnel up |
| 4 | `callcheck` | Not run, needs the owner | | **Open** |
| 5 | `twotunnels` | Pointer on tunnel one, 120 pasteboard reads on tunnel two | 0 failed reads, pointer moved, HID round trip median 8.3 ms, stream restarted with both up | **Pass** |
| 6 | `wifimount` | Not run, needs an iPad restart | | **Open** |
| 7 | `idle` | One report after 5 idle minutes, three ways | The first report moved the pointer each time | **Loss not reproduced** |
| 8 | `keyboard` | Keys below, into Spotlight | Read from crops of the shots | **Pass**, except F keys and German, below |

### Is a stream needed?

**Apparently not, which contradicts the design.** The spec says the iPad drops every event without a stream. Here it did not:

- After `stop_media_stream`, the server reported `running: false`, media stopped, and keys still landed 1.5 s, 12 s and 3 s later. A session with no stream then created a new virtual mouse, and it moved.
- With `NOSTREAM=1`, a session that never starts a stream: typing, motion, click, right click, held drag, and scroll in both axes all worked. Screenshots worked too.
- That held 5 minutes after the last stream ended (`idle none2`).

Not proven: every test followed a stream started earlier in the same iPad boot, so a stream may still be needed once per boot. Run `NOSTREAM=1` first thing after the restart in proof 6. The older claim's evidence was not rechecked.

If this holds, the build can drop the stream, its 1.3 MB/s and the "viewed remotely" notification, and calls may stop mattering. Decide after proofs 4 and 6.

### 1. Scroll

```
07 00 00 x y 00 00 00 00 00 00 00 00      13 bytes
```

`x` and `y` are signed 8-bit. The layout comes from `dtuhidd`'s call to `IOHIDEventCreateScrollEvent`, which reads bits 0x10 and 0x18 of the collection after the ID byte. The earlier "three 32-bit values" was wrong. Bytes 5 to 12 hold precise values that the daemon reads only under a flag; 1.0 to 16.0 in 16.16 had no effect there.

Negative `y` scrolls down the page, the same sign as a Windows wheel. Negative `x` moves content left. The plan's `scroll_report(wheel, pan)` puts `pan` in byte 3 and `wheel` in byte 4.

| `y` | Reports | Moved | Per report |
|---|---|---|---|
| -1 | 1, 3, 30 | 6, 18, 180 px | 6 px |
| -4 | 30 | 240 px | 8 px |
| -16 | 1, 5, 10 | 32, 160, 320 px | 32 px |

So about 2 px per unit, with a floor of 6 px. No momentum followed. A usable start is one report of `y = ±16` per wheel notch, tuned in Fine tuning. Sizes above 16 were not measured cleanly. In Weather, 40 reports of `x = -127` paged to the next city.

### 2. Right click and drag

Bit 1 of `buttons` is the secondary click. Holding bit 0 across motion reports is a drag, so every report must carry the bit while it is held.

### 3. Stream start

| Path | Median | Range |
|---|---|---|
| Stream start, tunnel kept | 163 ms | 119 to 206 ms |
| From nothing: tunnel | 332 ms | 290 to 381 ms |
| From nothing: tunnel, stream, HID clients | 632 ms | 578 to 733 ms |
| From nothing: all that plus keyboard, so first report | 867 ms | 800 to 1039 ms |

Five runs each. The cold runs used `SETTLE_MS=0` and lost no letter, so the spike's 300 ms wait is not needed. With the tunnel kept, letters started the moment the stream was requested and `a` still landed, which is how the finding above surfaced. So 163 ms is the stream's own start, and input did not wait for it.

`createService` takes about 30 ms, or 650 ms when it also opens the raw client. A report sent 0 ms after it moved the pointer, so the spike's 1200 ms wait is not needed either.

**Rule applied:** under 250 ms, so if a stream is kept at all, it runs only while the iPad is in control.

### 5. Two tunnels

Input and clipboard can each hold a tunnel, using one pairing record. Pasteboard reads had a median of 60 ms beside a running stream. Tunnel one kept working after tunnel two closed. Only one order was tested, tunnel one first, though the stream was also restarted with tunnel two up. **No fallback needed: Task 4 stays as planned.** Only change counts were printed, never pasteboard content.

### 7. Idle

| Keepalive | Stream | First report after 5 minutes |
|---|---|---|
| `connectedServices` every 500 ms, worst trip 61 ms | Yes | Landed |
| None | Yes | Landed |
| None | No | Landed |

The loss seen over USB did not appear, so nothing here needs a keepalive. Keep the planned 500 ms `connectedServices` round trip, which the link check needs anyway. A zero report was not tried, since nothing needed fixing. The iPad was charging and did not lock during these runs, so a locked or sleeping iPad is still untested.

### 8. Keyboard

| Sent | Appeared | Result |
|---|---|---|
| Shift `0xE1` + b | `aBc` | **Pass** |
| Left and Right `0x50`, `0x4F` | `axbcw` | **Pass** |
| Ctrl `0xE0` + a | Caret to line start, `qxv` | **Pass** |
| Cmd `0xE3` + a | All selected, next letter replaced it | **Pass** |
| Esc `0x29` | Cleared the search | **Pass** |
| Left held 1000 ms | Caret moved 6 places | **Pass**, the iPad repeats by itself |
| Backspace held 700 ms | 4 characters gone | **Pass** |
| k held 2000 ms | Accent popup, no repeat | Letters do not repeat, as on a Mac |
| Option `0xE2` + l, 5 | `¬ ∞` | **Pass**, US layout |
| Right Option `0xE6` + q, e | `œ ´` | Same as Option, no AltGr |
| `0x34 0x33 0x2F 0x2D`, the German ä ö ü ß keys | `' ; [ -` | **German layout not applied** |
| The first three with Shift | `" : {` | Same |
| `z`, `y`, `0x64` | `z y §` | No Y and Z swap |
| Option+u then a, o, u; Option+s | `ä ö ü ß` | **Works** as dead keys |
| Option+u then Shift+a, o, u | `Ä Ö Ü` | **Works** |
| F1 to F4, F11, F12 | Nothing visible on Home | **Not proven**: sent without error, nothing showed |

F5 to F10 were not sent, because they could start dictation, change Focus or play media.

The iPad treats the virtual keyboard as US English, so keys follow a US map. Two ways to give the owner a German keyboard:

1. The owner sets the hardware keyboard layout to German on the iPad. Usages would then probably just work. Not tested, because iPad settings were off limits.
2. The translator maps German keys to US usages and dead-key sequences. This needs nothing on the iPad but costs a table for every symbol.

This is the owner's call.

### Still open

**Proof 4.** Run it twice, with a stream and with `NOSTREAM=1`. The second result decides whether calls matter at all.

**Proof 6.** After a restart and one unlock, `wifimount` tries lockdown on the Wi-Fi address with the record Apple Devices keeps, then the tunnel, up to 180 s each. Lockdown over Wi-Fi may need Wi-Fi sync, which is unverified. `wifimount dry` reaches the mounter and stops. Neither has been run.

## Evidence

Local only, not in the repo, because the full shots show personal notifications, calendar and file names.

- `C:\dev\_ref\tmp\pointer\shots\demo-2clicked.png`: Files open after the virtual click, pointer visible at bottom right.
- `C:\dev\_ref\tmp\pointer\shots\demo-1hover.png`, `demo-waypoint0.png` to `demo-waypoint2.png`: pointer at each destination.
- `C:\dev\_ref\tmp\pointer\demo-waypoint1-crop.png`: crop of the pointer over an icon, no personal content.
- `C:\dev\_ref\tmp\pointer\demo-trace.log`, `corner-to-corner-trace.log`, `first-motion.log`: `backboardd` position lines.
- `C:\dev\_ref\tmp\pointer\wifi-first.log`: full iPad log around the bad signature. `wifi-rppair.log`: the new pairing. `wifi-second.log`: verify passing.
- `C:\dev\_ref\tmp\pointer\wifi-trace.log`, `shots\wifi-*.png`, `shots\wifi2-typed.png`: Wi-Fi demo and typing.
- `C:\dev\_ref\tmp\pointer\shots`: `p1*` scroll, `p2*` clicks, `st-*` stream time, `gate*` and `ns*` no stream, `tt-*` tunnels, `idle-*` idle, `kb*` keyboard. `idle.log` and `p1-scroll.log` beside it: run output.

## Costs and limits

- Needs the disk image mounted and Developer Mode on. A display stream was believed to be needed too; see [Is a stream needed?](#is-a-stream-needed).
- Starting the stream leaves a "This device was viewed remotely from Mac" notification on the iPad.
- Reports sent after a long idle can be lost or unlogged at first. Seen several times over USB; see proof 7.
- Touching the top edge pulls down Notification Centre, as a real mouse would.
- Latency to the screen was not measured.
- Battery and iPad sleep are untested. Unplugged from USB is proven: every proof below ran that way.

## To reproduce

With Apple Devices open and the iPad unlocked on USB: `cargo run -p ipad-input -- demo mytag --usb`. It opens Files, then presses Home.

Over Wi-Fi, once: `cargo run -p ipad-input -- rppair` over USB and tap Trust. Then `IPAD_ADDR=<ip>:<port> cargo run -p ipad-input -- demo mytag`, with the port from the `_remotepairing._tcp` SRV record. Later runs do not need `IPAD_ADDR`. Without USB the demo still runs, but the log tap fails, so only screenshots are left as proof.


## Disk image after an iPad restart

2026-11-05, owner restarted the iPad, USB out. `wifimount`: the lockdown route failed at `copy devices` after about 1 s, and the **tunnel route mounted the disk image over Wi-Fi in about 2 s**. No USB is needed after a restart on this network. A no-stream `do` session then opened the tunnel and created the mouse.

Proof 4, a call during a session, is still open. A 55 s run with the stream stayed healthy at about 8 ms, but no call was active in it.

**No stream, from a fresh restart: confirmed.** After that restart and mount, with no display stream ever started, `NOSTREAM=1 do home goto:1000,1000` left the pointer on screen at the target (crop `C:\dev\_ref	mp\pointereboot-ns2-crop.png`). The stream is not needed for pointer input on this iPad.
