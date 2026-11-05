# Two device bugs: iPad shown twice, Wi-Fi link drops when Bluetooth comes on

Status: fixed in code, all checks pass. Not yet tried on the devices.

## Bug 2: the Wi-Fi link "disconnects" when Bluetooth comes up

**Cause: a Bridge bug. The iPad did not drop anything.** The engine polled `ipad-wifi` as if it were a Bluetooth host.

`watched` was `settings.placed_ids()`, which includes `ipad-wifi` once it is placed. The watcher adds every watched id to `Snapshot.ids` and asks the peripheral whether it is connected. The peripheral does not know `ipad-wifi`, so the answer is no, and the engine's poll runs `connected.remove("ipad-wifi")`. Only an `IpadInputState` change puts it back, and the state was still `Ready`, so it stayed out. The router then treated the iPad as gone. With Bluetooth off there is no watcher, which is why the Bluetooth-off test passed.

Evidence from `engine.log`, run started at `clock unix=1793874146`:

| Time | Line | Meaning |
|---|---|---|
| 188157 ms | `bluetooth peripheral started` | The watcher starts |
| 188514 ms | `ipad-wifi disconnected` | One poll later. Only the Bluetooth poll writes this line |
| 188514 to 377757 ms | no `iPad input` line at all | The service stayed `Ready`, with no keepalive miss or stream error, for 189 s |
| after 188514 ms | no `active -> Some("ipad-wifi")` | The iPad could not be entered again. It was entered 7 times before this |

The previous run shows the same thing without any Bluetooth change: peripheral up at 128 ms, then `ipad-wifi disconnected` at 20687 ms. That timing fits the owner placing the iPad, which updates `watched`.

Ruled out:

- **Settings Reload restarting the service:** Reload does not touch `ipadinput`, and there was no `Connecting` line.
- **The iPad dropping the tunnel on Bluetooth connect:** no link-down line near any Bluetooth connect. The real `link down` at 377757 ms came 4 s after the iPad's HID control point wrote suspend (`[0]` at 373410 ms), and the clipboard dropped soon after, so that looks like the iPad going to sleep.

Same fault, second symptom: `status.hosts` listed `ipad-wifi` from the snapshot (offline, no name) and again from `ipad_host`, so the Desk received two `ipad-wifi` rows.

The log has no `bluetooth switched` line, although the running exe contains that string. So Bluetooth came up through the start retry, not through Bridge's switch, which suggests the owner used Windows Settings. The fault is the same either way.

Separate and not fixed: after 7865 s in the last run the Wi-Fi link flapped several times with Bluetooth down (`hosts=0`, no peripheral). Input and clipboard fell together, with `_remotepairing._tcp not found`. That points at the iPad or the network, not this bug.

**Fix (`platform/windows/src/engine.rs`):** new `is_ble_host` is false for `ipad-wifi` and for retired hosts. `watch_list` and `ble_ids` use it, so neither is watched, polled or listed from Bluetooth. A host retired while connected leaves `connected`, and the next tick returns input to Windows.

**Retired host reconnecting:** it is no longer counted, listed or logged as a host, and it has no placement, so it gets no reports. It can still connect at the Bluetooth level. The peripheral has no call to refuse one bonded central, and stopping advertising would cut off the phone. The owner can remove the Bridge pairing on the iPad to end that. I changed no Windows or iPad setting.

## Bug 1: two iPads on the Desk

**Cause:** `migrateIpad` only knew one way to find the host, and could not run in the owner's state.

1. It returned at once if `device_links["ipad-wifi"]` existed.
2. It needed a `device_links` entry from the Bluetooth host to an iPad peer. Once `ipad-wifi` holds the peer, `nameLinks` treats that peer as taken, so the Bluetooth host can never get one.
3. `hub.tsx` stopped after the first migration in a run (`migrated` ref) and never looked at `status.hosts`, so a host that connects later was missed.

Evidence: `active -> Some("…5b:95:30:4d:1c:f5")` at 223164 ms, after `ipad-wifi` had been in control. That id is the iPad's Bluetooth host (`docs/HANDOFF.md`), so it was still placed and not retired, with `ipad-wifi` placed as well.

**Limit on this evidence:** I could not read the owner's real settings. The `settings.json` my tools see is a stale copy dated 2026-11-03, with no `device_links`, `bluetooth_enabled` or `ipad-wifi`. It sits in Claude's package folder (`%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\LocalCache\Roaming\CrossThePlatforms`) and appears to hide the real file from anything Claude starts. The log proves the live settings differ, because `ipad-wifi` was placed. So which of causes 1 and 2 applied is inferred from the code and log. The fix covers both. I did not go around that redirect, and I did not read `peers.json`.

**Fix (`apps/tauri/src/lib/devices.ts`, `hub.tsx`):**

- `ipadBluetoothHosts` finds linked hosts as before, plus connected hosts with no link whose name contains "iPad" or equals an iPad peer's name. The name rule applies only once `ipad-wifi` is listed, so an iPad that has only Bluetooth is left alone.
- `migrateIpad` no longer stops on an existing link. What `ipad-wifi` already has stays and the old place is dropped, so nothing is placed twice. If `ipad-wifi` has no place, the old one moves as before. If it has no link, it takes the host's link, or the only free iPad peer.
- `hub.tsx` now checks whenever status, peers or settings change, with a per-host guard so a failed save does not loop.
- `linkableHosts` stops a retired or retiring host taking a name link.
- `engine.rs`: the watcher kept a first empty name forever. It now keeps asking, because the match depends on the name. This line runs on the watcher thread, not the input thread, and has no unit test because it calls WinRT.

Known limit, marked `ponytail:` in the code: a second, different iPad on Bluetooth with "iPad" in its name would also be retired. Nothing in the hub brings a retired host back yet.

## Tests

| Test | RED | GREEN |
|---|---|---|
| `migrate_retires_a_bluetooth_host_with_no_link_by_its_name` | `expected [] to deeply equal ['BT:IPAD']` | pass |
| `migrate_matches_the_ipad_peers_name_and_links_ipad_wifi` | same | pass |
| `migrate_moves_the_place_when_ipad_wifi_has_none` | attachment stayed on `BT:IPAD` | pass |
| `a_linked_host_still_migrates_when_ipad_wifi_is_already_linked` | `expected [] to deeply equal ['BT:IPAD']` | pass |
| `name_match_leaves_other_hosts_alone` | function missing | pass |
| `a_retiring_or_retired_host_never_takes_a_name_link` | function missing | pass |
| `a_placed_ipad_wifi_is_not_a_bluetooth_host` | `["ipad-wifi", "AA:BB"]` vs `["AA:BB"]` | pass |
| `a_retired_host_is_not_watched_or_counted` | `["OLD", "AA:BB"]` vs `["AA:BB"]` | pass |

The engine tests went RED against seams that kept the old behaviour, so they failed on the assertion, not on a compile error.

Full run:

- `cargo test -p bridge-core`: 204 passed, 1 ignored
- `cargo test -p ctp-windows --lib`: 65 passed, 7 ignored
- `cargo clippy -p bridge-core -p ctp-windows -- -D warnings`: clean
- `cargo check -p ctp-app`: clean
- `pnpm test`: 168 passed
- `pnpm exec tsc --noEmit -p tsconfig.app.json`: clean

`hub.tsx` effects are not unit-tested. The pure functions they call are.

## Needs a device

The owner's running `ctp-app.exe` is the old build, so it must be rebuilt and restarted first.

1. With `ipad-wifi` placed and Ready, turn Bluetooth on. The log must show no `ipad-wifi disconnected`, and pushing into the iPad must still work.
2. Once the iPad's Bluetooth connects, the Desk shows one iPad, and it stays one after a restart.
3. Confirm the phone over Bluetooth still works and keeps its place. Its hotkey number can shift when the iPad's old place is dropped.
4. See whether the retired iPad keeps reconnecting at the Bluetooth level. It should do no harm. If the owner wants it gone, remove the pairing on the iPad.
