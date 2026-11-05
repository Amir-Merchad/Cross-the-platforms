import { expect, test } from "vitest";
import type { Row } from "./activity";
import type { ClipStatus, HostStatus, Monitor, Settings, Status } from "./api";
import type { Peer } from "./transfer";
import { IPAD_HOST, buildDevices, hiddenDevices, ipadBluetoothHosts, linkableHosts, showHost, deskLine, deviceLook, displayName, forgetDevice, idleLine, ipadLine, linkByName, migrateIpad, panelLines, type Device } from "./devices";

const host = (id: string, name: string | null, connected = true): HostStatus => ({ id, name, connected, interval_ms: 0, notify_avg_ms: 0, notify_max_ms: 0 });
const status = (hosts: HostStatus[]): Status => ({ advertising: true, hosts, active: null, notice: null });
const settings = (o: Partial<Settings> = {}): Settings => ({
  version: 1, attachments: [], calibration: {}, entry_hold_ms: 0, start_with_windows: false, remote_width: {},
  clipboard_sync: true, receive_dir: null, device_links: {}, device_colors: {}, appearance: "system", bluetooth_enabled: true, retired_hosts: [], kept_hosts: [], retired_names: {}, layout: [], ...o,
});
const peer = (id: string, name: string): Peer => ({ id, name, kind: "Ipad" });
const mon: Monitor[] = [{ id: "M1", rect: { left: 0, top: 0, right: 100, bottom: 100 } }];

test("a link merges a host and a peer", () => {
  const d = buildDevices(status([host("AA:1", "iPad")]), [peer("p1", "iPad")], settings({ device_links: { "AA:1": "p1" } }), ["p1"], { p1: 2 }, mon);
  expect(d).toHaveLength(1);
  expect(d[0]).toMatchObject({ hostId: "AA:1", peerId: "p1", reachable: true, waiting: 2, connected: true, kind: "Ipad" });
});

test("unlinked host and peer stay separate", () => {
  const d = buildDevices(status([host("AA:1", "Pixel")]), [peer("p1", "iPad")], settings(), [], {}, mon);
  expect(d.map((x) => [x.hostId, x.peerId])).toEqual([["AA:1", null], [null, "p1"]]);
});

test("blank names show Unnamed device and never an id", () => {
  expect(displayName("  ")).toBe("Unnamed device");
  expect(displayName(null)).toBe("Unnamed device");
  const d = buildDevices(status([host("AA:1", null)]), [peer("p1", "")], settings(), [], {}, mon);
  expect(d.map((x) => x.name)).toEqual(["Unnamed device", "Unnamed device"]);
});

test("attachment to a missing monitor flags missingScreen and keeps placement", () => {
  const att = { device_id: "AA:1", monitor_id: "GONE", edge: "Left" as const };
  const [d] = buildDevices(status([host("AA:1", "Pixel")]), [], settings({ attachments: [att] }), [], {}, mon);
  expect(d.missingScreen).toBe(true);
  expect(d.placement).toEqual(att);
});

const placed = { device_id: "AA:1", monitor_id: "M1", edge: "Left" as const };

test("Bluetooth off at launch keeps placed devices on the Desk, showing No Bluetooth", () => {
  const d = buildDevices(status([]), [], settings({ attachments: [placed] }), [], {}, mon);
  expect(d).toHaveLength(1);
  expect(d[0]).toMatchObject({ hostId: "AA:1", connected: false, placement: placed, name: "Unnamed device" });
  expect(deviceLook(d[0], null, true)).toEqual({ sub: "No Bluetooth", off: true });
});

test("a linked device keeps its host while the engine does not list it", () => {
  const d = buildDevices(status([]), [peer("p1", "iPad")], settings({ device_links: { "AA:1": "p1" } }), ["p1"], {}, mon);
  expect(d).toHaveLength(1);
  expect(d[0]).toMatchObject({ hostId: "AA:1", peerId: "p1", name: "iPad", connected: false, reachable: true });
});

test("a host in status, attachments and links is one device, and the order does not depend on who is listed", () => {
  const s = settings({ attachments: [{ ...placed, device_id: "b" }, { ...placed, device_id: "a", edge: "Right" }], device_links: { a: "p1" } });
  expect(buildDevices(status([host("a", "A")]), [peer("p1", "A")], s, [], {}, mon).map((x) => x.key)).toEqual(["a", "b"]);
  expect(buildDevices(status([]), [peer("p1", "A")], s, [], {}, mon).map((x) => x.key)).toEqual(["a", "b"]);
});

test("colours come from settings, else by index", () => {
  const d = buildDevices(status([host("a", "A"), host("b", "B")]), [], settings({ device_colors: { b: "#000000" } }), [], {}, mon);
  expect(d.map((x) => x.colour)).toEqual(["#FF7A4D", "#000000"]);
});

test("linkByName matches identical names only", () => {
  const ps = [peer("p1", "iPad Air"), peer("p2", "iPad")];
  expect(linkByName("iPad", ps)).toBe("p2");
  expect(linkByName("ipad", ps)).toBeNull();
  expect(linkByName(null, ps)).toBeNull();
});

test("forgetDevice removes attachment, link and colour", () => {
  const s = settings({
    attachments: [{ device_id: "AA:1", monitor_id: "M1", edge: "Left" }],
    device_links: { "AA:1": "p1" }, device_colors: { "AA:1": "#111111", other: "#222222" },
  });
  const [d] = buildDevices(status([host("AA:1", "iPad")]), [peer("p1", "iPad")], s, [], {}, mon);
  const out = forgetDevice(s, d);
  expect(out.attachments).toEqual([]);
  expect(out.device_links).toEqual({});
  expect(out.device_colors).toEqual({ other: "#222222" });
});

const dev = (o: Partial<Device> = {}): Device => ({
  key: "h", hostId: "h", peerId: "p", name: "iPad Air", kind: "Ipad", connected: true, reachable: true, waiting: 0,
  placement: { device_id: "h", monitor_id: "M1", edge: "Right" }, missingScreen: false, colour: "#FF7A4D", beside: null, ...o,
});
const look = (d: Device, active: string | null = null, btOff = false) => deviceLook(d, active, btOff);

test("deviceLook follows the canvas states", () => {
  expect(look(dev())).toEqual({ sub: "Connected", off: false });
  expect(look(dev(), "h")).toEqual({ sub: "In control", off: false });
  expect(look(dev({ connected: false, reachable: false }))).toEqual({ sub: "Offline", off: true });
  expect(look(dev({ reachable: false, waiting: 3 }))).toEqual({ sub: "Bridge closed", off: true });
  expect(look(dev(), null, true)).toEqual({ sub: "No Bluetooth", off: true });
  expect(look(dev({ placement: null }))).toEqual({ sub: "Not placed", off: false });
  expect(look(dev({ missingScreen: true }))).toEqual({ sub: "Screen not connected", off: false });
});

test("deviceLook keeps a Bluetooth device in colour with Bridge closed and nothing waiting", () => {
  expect(look(dev({ reachable: false }))).toEqual({ sub: "Connected", off: false });
});

test("deviceLook for devices with files only", () => {
  const files = dev({ hostId: null, placement: null, connected: false });
  expect(look(files)).toEqual({ sub: "Files only", off: false });
  expect(look(files, null, true)).toEqual({ sub: "Files only", off: false });
  expect(look({ ...files, reachable: false })).toEqual({ sub: "Bridge closed", off: true });
  expect(look(dev({ hostId: null, connected: false, placement: null, waiting: 2, reachable: true }))).toEqual({ sub: "Files only", off: false });
});

const clip = (o: Partial<ClipStatus> = {}): ClipStatus => ({ enabled: true, ipad: { Connected: { wifi: true } }, synced: 0, skipped_too_large: 0, notice: null, history_rev: 0, ...o });
const lines = (d: Device, o: { active?: string | null; btOff?: boolean; clip?: ClipStatus | null } = {}) =>
  panelLines(d, { active: o.active ?? null, btOff: o.btOff ?? false, clip: o.clip === undefined ? clip() : o.clip, where: "right of Screen 1", hotkey: 1 });

test("panelLines: ready to switch", () => {
  expect(lines(dev())).toEqual({ key: "Ctrl+Alt+1", button: "Switch to iPad Air", canSwitch: true, why: "", hid: "Bluetooth, right of Screen 1", files: "Ready", clip: "Connected over Wi-Fi" });
});

test("panelLines: in control the button goes back", () => {
  const l = lines(dev(), { active: "h" });
  expect([l.button, l.canSwitch, l.hid]).toEqual(["Back to Windows", true, "In control"]);
});

test("panelLines: offline", () => {
  const l = lines(dev({ connected: false, reachable: false }), { clip: clip({ ipad: "Connecting" }) });
  expect([l.button, l.canSwitch, l.hid, l.files, l.clip]).toEqual(["iPad Air is offline", false, "Not connected", "Sends when it is back", "Not connected"]);
});

test("panelLines: Bluetooth off", () => {
  const l = lines(dev(), { btOff: true });
  expect([l.canSwitch, l.hid, l.why]).toEqual([false, "Bluetooth is off", "Keyboard and mouse sharing is paused."]);
});

test("panelLines: an unplaced device cannot switch and says why", () => {
  const l = lines(dev({ placement: null }));
  expect([l.key, l.canSwitch, l.why, l.hid]).toEqual(["Not placed", false, "Put it next to a screen edge first.", "Bluetooth, not placed"]);
});

test("panelLines: a device on an unplugged screen", () => {
  const l = lines(dev({ missingScreen: true }));
  // The engine, hotkey and tray still switch to it, so the button does too.
  expect([l.key, l.canSwitch, l.why, l.hid]).toEqual(["Screen not connected", true, "", "Bluetooth, screen not connected"]);
});

test("panelLines: files only and Bluetooth only", () => {
  const files = lines(dev({ hostId: null, placement: null, connected: false }));
  expect([files.canSwitch, files.why, files.hid]).toEqual([false, "It needs Bluetooth first.", "Needs Bluetooth"]);
  const ble = lines(dev({ peerId: null, kind: null, reachable: false }));
  expect([ble.files, ble.clip]).toEqual(["Not paired for files", "Not paired"]);
});

test("panelLines: Files row says what a send would do", () => {
  expect(lines(dev({ waiting: 3, reachable: false })).files).toBe("3 waiting");
  expect(lines(dev({ reachable: false })).files).toBe("Sends when it is back");
  expect(lines(dev({ kind: "Android", reachable: false })).files).toBe("Open Bridge there first");
});

test("panelLines: Clipboard row never shows a raw reason", () => {
  expect(lines(dev(), { clip: clip({ enabled: false }) }).clip).toBe("Sync off");
  expect(lines(dev(), { clip: clip({ ipad: { Unavailable: "raw os error 5" } }) }).clip).toBe("Not connected");
  expect(lines(dev(), { clip: clip({ ipad: "NotPaired" }) }).clip).toBe("Not paired");
  expect(lines(dev(), { clip: clip({ ipad: { Connected: { wifi: false } } }) }).clip).toBe("Connected over USB");
  expect(lines(dev({ kind: "Android" })).clip).toBe("Text only");
  expect(lines(dev({ kind: "Android", reachable: false })).clip).toBe("Not connected");
  expect(lines(dev(), { clip: null }).clip).toBe("Not connected");
});

const moving = (o: Partial<Row> = {}): Row => ({ id: 1, peer: "p", outgoing: true, name: "Site photos.zip", files: 1, total: 100e6, done: 84e6, startedAt: 0, now: 1000, paths: [], state: "moving", received: 0, ...o });
const stat = (o: Partial<Status> = {}): Status => ({ advertising: true, hosts: [host("h", "iPad Air")], active: null, notice: null, ...o });
const line = (o: { status?: Status | null; rows?: Row[]; selected?: Device | null; btOff?: boolean; filesDown?: boolean } = {}) =>
  deskLine(o.status === undefined ? stat() : o.status, o.rows ?? [], [dev()], o.selected ?? null, o.btOff ?? false, o.filesDown ?? false);

test("deskLine follows the canvas states, and input comes first", () => {
  expect(line()).toBe("Your keyboard and mouse are on Windows.");
  expect(line({ rows: [moving()] })).toBe("Sending Site photos.zip to iPad Air at 84.0 MB/s.");
  expect(line({ status: stat({ active: "h" }), rows: [moving()] })).toBe("Your keyboard and mouse are on iPad Air. Push back across the edge, or press Ctrl+Alt+Q.");
  expect(line({ btOff: true, status: stat({ notice: "Bluetooth advertising restarting…" }) })).toBe("Keyboard and mouse sharing is paused.");
  expect(line({ selected: dev({ connected: false, reachable: false }) })).toBe("iPad Air went offline. Bridge reconnects by itself.");
  expect(line({ selected: dev({ reachable: false, waiting: 3 }) })).toBe("iPad Air has Bridge closed. Files will wait.");
});

test("deskLine says when file sending did not start, after where the input is", () => {
  expect(line({ filesDown: true })).toBe("File sending did not start. Quit Bridge from the tray and open it again.");
  expect(line({ filesDown: true, status: stat({ active: "h" }) })).toContain("Your keyboard and mouse are on iPad Air.");
});

test("deskLine ignores incoming and finished rows, and has nothing before status", () => {
  expect(line({ rows: [moving({ outgoing: false }), moving({ id: 2, state: "done" })] })).toBe("Your keyboard and mouse are on Windows.");
  expect(line({ status: null })).toBe("");
});

test("idleLine", () => {
  expect(idleLine([])).toBe("Files you send show up here.");
  expect(idleLine([dev()])).toBe("No files moving.");
  expect(idleLine([dev({ waiting: 3 })])).toBe("3 files wait for iPad Air. They send when Bridge opens there.");
  expect(idleLine([dev({ waiting: 1 })])).toBe("1 file waits for iPad Air. It sends when Bridge opens there.");
});

test("deviceLook: a screen that is gone is said first, and an offline device stays grey", () => {
  const gone = { missingScreen: true };
  expect(look(dev({ ...gone, connected: false, reachable: false }))).toEqual({ sub: "Screen not connected", off: true });
  expect(look(dev({ ...gone, reachable: false, waiting: 3 }))).toEqual({ sub: "Screen not connected", off: true });
  expect(look(dev({ ...gone, connected: false, reachable: true }))).toEqual({ sub: "Screen not connected", off: true });
  expect(look(dev(gone))).toEqual({ sub: "Screen not connected", off: false });
});

test("deviceLook: a Bluetooth device reachable only for files reads Offline, because Files only means it cannot be placed", () => {
  expect(look(dev({ connected: false, reachable: true }))).toEqual({ sub: "Offline", off: true });
  expect(look(dev({ connected: false, reachable: true, placement: null }))).toEqual({ sub: "Offline", off: true });
});

test("deskLine says what a send hotkey did, and only that", () => {
  const nothing = "No device is in control, so nothing was sent.";
  expect(line({ status: stat({ notice: nothing }) })).toBe(nothing);
  expect(line({ status: stat({ notice: "Bluetooth not ready: 0x80070005" }) })).toBe("Your keyboard and mouse are on Windows.");
});

test("a send notice keeps the way out in view while a device is in control", () => {
  const way = "Push back across the edge, or press Ctrl+Alt+Q.";
  expect(line({ status: stat({ notice: "Sending the selected files.", active: "h" }) })).toBe(`Your keyboard and mouse are on iPad Air. Sending the selected files. ${way}`);
  expect(line({ status: stat({ notice: "Paste pressed.", active: "h" }) })).toBe(`Your keyboard and mouse are on iPad Air. Paste pressed. ${way}`);
});

test("a send notice never hides what matters more", () => {
  const status = stat({ notice: "Nothing is selected in Explorer, so nothing was sent." });
  expect(line({ status, filesDown: true })).toBe("File sending did not start. Quit Bridge from the tray and open it again.");
  expect(line({ status, rows: [moving()] })).toBe("Sending Site photos.zip to iPad Air at 84.0 MB/s.");
  expect(line({ status, btOff: true })).toBe("Keyboard and mouse sharing is paused.");
  expect(line({ status, selected: dev({ connected: false, reachable: false }) })).toBe("iPad Air went offline. Bridge reconnects by itself.");
});

test("deskLine never passes an engine notice through", () => {
  const raw = "advertising failed: HRESULT(0x80070005) BluetoothLE#BluetoothLE00:11:22";
  expect(line({ status: stat({ notice: raw }) })).toBe("Your keyboard and mouse are on Windows.");
  expect(line({ status: stat({ notice: raw, advertising: false }) })).toBe("Starting Bluetooth…");
  expect(line({ status: stat({ advertising: false }) })).toBe("Starting Bluetooth…");
});

test("migrate_moves_the_ipad_once", () => {
  const before = settings({
    attachments: [{ device_id: "PH:1", monitor_id: "M1", edge: "Left" }, { device_id: "BT:IPAD", monitor_id: "M1", edge: "Right" }],
    device_links: { "BT:IPAD": "p1", "PH:1": "p2" }, device_colors: { "BT:IPAD": "#123456" },
    calibration: { "BT:IPAD": { gain: 1.5, min_push: 96, dwell_ms: 300 } },
  });
  const peers: Peer[] = [peer("p1", "iPad"), { id: "p2", name: "Pixel", kind: "Android" }];
  const after = migrateIpad(before, peers);
  expect(after.attachments.map((a) => a.device_id)).toEqual(["PH:1", IPAD_HOST]);
  expect(after.attachments[1].edge).toBe("Right");
  expect(after.device_links).toEqual({ "PH:1": "p2", [IPAD_HOST]: "p1" });
  expect(after.device_colors).toEqual({ [IPAD_HOST]: "#123456" });
  expect(after.retired_hosts).toEqual(["BT:IPAD"]);
  expect(after.calibration[IPAD_HOST]).toBeUndefined();
  expect(migrateIpad(after, peers)).toBe(after);
  const laid = migrateIpad({ ...before, layout: [{ device_id: "BT:IPAD", rect: { left: 0, top: 0, right: 1, bottom: 1 } }] }, peers);
  expect(laid.layout.map((p) => p.device_id)).toEqual([IPAD_HOST]);
});

test("migrate_without_a_link_does_nothing", () => {
  const s = settings({ attachments: [{ device_id: "BT:IPAD", monitor_id: "M1", edge: "Right" }] });
  expect(migrateIpad(s, [peer("p1", "iPad")])).toBe(s);
});

test("retired_hosts_are_not_listed", () => {
  const s = settings({ retired_hosts: ["BT:IPAD"], device_links: { "BT:IPAD": "x" } });
  const d = buildDevices(status([host("BT:IPAD", "iPad"), host(IPAD_HOST, "iPad")]), [], s, [], {}, mon);
  expect(d.map((x) => x.hostId)).toEqual([IPAD_HOST]);
});

const BT = "BT:IPAD", wifi = host(IPAD_HOST, "iPad");

test("migrate_retires_a_bluetooth_host_with_no_link_by_its_name", () => {
  // The owner's case: ipad-wifi already has the peer and a place, so the Bluetooth host could never get a link.
  const before = settings({
    attachments: [{ device_id: BT, monitor_id: "M1", edge: "Bottom" }, { device_id: IPAD_HOST, monitor_id: "M1", edge: "Right" }, { device_id: "PH:1", monitor_id: "M2", edge: "Right" }],
    device_links: { [IPAD_HOST]: "p1" },
  });
  const hosts = [host(BT, "Amir's iPad"), host("PH:1", "Galaxy S24+"), wifi];
  const after = migrateIpad(before, [peer("p1", "iPad")], hosts);
  expect(after.retired_hosts).toEqual([BT]);
  expect(after.attachments.map((a) => [a.device_id, a.edge])).toEqual([[IPAD_HOST, "Right"], ["PH:1", "Right"]]);
  expect(after.device_links).toEqual({ [IPAD_HOST]: "p1" });
  expect(migrateIpad(after, [peer("p1", "iPad")], hosts)).toBe(after);
  const d = buildDevices(status(hosts), [peer("p1", "iPad")], after, [], {}, mon);
  expect(d.filter((x) => x.kind === "Ipad" || x.name.includes("iPad")).map((x) => x.hostId)).toEqual([IPAD_HOST]);
});

test("migrate_matches_the_ipad_peers_name_and_links_ipad_wifi", () => {
  const after = migrateIpad(settings(), [peer("p1", "Slate")], [host(BT, "Slate"), wifi]);
  expect(after.retired_hosts).toEqual([BT]);
  expect(after.device_links).toEqual({ [IPAD_HOST]: "p1" });
});

test("migrate_moves_the_place_when_ipad_wifi_has_none", () => {
  const before = settings({ attachments: [{ device_id: BT, monitor_id: "M1", edge: "Bottom" }] });
  const after = migrateIpad(before, [], [host(BT, "iPad"), wifi]);
  expect(after.attachments).toEqual([{ device_id: IPAD_HOST, monitor_id: "M1", edge: "Bottom" }]);
});

test("a_linked_host_still_migrates_when_ipad_wifi_is_already_linked", () => {
  const before = settings({ device_links: { [BT]: "p1", [IPAD_HOST]: "p1" } });
  const after = migrateIpad(before, [peer("p1", "iPad")]);
  expect(after.retired_hosts).toEqual([BT]);
  expect(after.device_links).toEqual({ [IPAD_HOST]: "p1" });
});

test("name_match_leaves_other_hosts_alone", () => {
  const s = settings({ device_links: { "BT:2": "p2" } });
  const peers: Peer[] = [peer("p1", "iPad"), { id: "p2", name: "Pixel", kind: "Android" }];
  expect(ipadBluetoothHosts(s, peers, [host("PH:1", "Galaxy"), host("BT:2", "iPad"), wifi]), "a host linked to another peer").toEqual([]);
  expect(ipadBluetoothHosts(s, peers, [host(BT, "iPad", false), wifi]), "not connected").toEqual([]);
  expect(ipadBluetoothHosts(s, peers, [host(BT, null), wifi]), "no name yet").toEqual([]);
  expect(ipadBluetoothHosts(s, peers, [host(BT, "iPad")]), "no Wi-Fi iPad: Bluetooth is its only link").toEqual([]);
  expect(ipadBluetoothHosts(settings({ retired_hosts: [BT] }), peers, [host(BT, "iPad"), wifi]), "already retired").toEqual([]);
});

test("a_retiring_or_retired_host_never_takes_a_name_link", () => {
  const peers = [peer("p1", "iPad")];
  expect(linkableHosts(settings(), peers, [host(BT, "iPad"), host("PH:1", "Galaxy"), wifi]).map((h) => h.id)).toEqual(["PH:1", IPAD_HOST]);
  expect(linkableHosts(settings({ retired_hosts: [BT] }), peers, [host(BT, "iPad")])).toEqual([]);
});

test("the_name_rule_waits_for_ipad_wifi_to_be_connected", () => {
  const down = host(IPAD_HOST, "iPad", false);
  const s = settings({ attachments: [{ device_id: BT, monitor_id: "M1", edge: "Right" }] });
  expect(ipadBluetoothHosts(s, [], [host(BT, "iPad"), down])).toEqual([]);
  expect(migrateIpad(s, [], [host(BT, "iPad"), down])).toBe(s);
});

test("a_kept_host_is_never_retired", () => {
  const peers = [peer("p1", "iPad")];
  expect(ipadBluetoothHosts(settings({ kept_hosts: [BT] }), peers, [host(BT, "iPad"), wifi]), "by name").toEqual([]);
  expect(ipadBluetoothHosts(settings({ kept_hosts: [BT], device_links: { [BT]: "p1" } }), peers, [wifi]), "by link").toEqual([]);
});

test("a_kept_ipad_mini_remote_stays_listed", () => {
  const s = settings({ kept_hosts: ["BT:MINI"] });
  const hosts = [host("BT:MINI", "iPad mini remote"), wifi];
  expect(migrateIpad(s, [], hosts)).toBe(s);
  expect(buildDevices(status(hosts), [], s, [], {}, mon).map((d) => [d.hostId, d.name])).toEqual([["BT:MINI", "iPad mini remote"], [IPAD_HOST, "iPad"]]);
});

test("hidden_devices_show_their_name_and_show_brings_one_back", () => {
  const hosts = [host(BT, "Amir's iPad"), wifi];
  const after = migrateIpad(settings(), [], hosts);
  const hidden = settings({ ...after, retired_hosts: [...after.retired_hosts, "BT:OLD"] });
  expect(hiddenDevices(hidden)).toEqual([{ id: BT, name: "Amir's iPad" }, { id: "BT:OLD", name: "Unnamed device" }]);
  const shown = showHost(hidden, BT);
  expect(shown.retired_hosts).toEqual(["BT:OLD"]);
  expect(shown.kept_hosts).toEqual([BT]);
  expect(shown.retired_names).toEqual({});
  expect(migrateIpad(shown, [], hosts), "it does not retire again").toBe(shown);
  expect(buildDevices(status(hosts), [], shown, [], {}, mon).map((d) => d.hostId)).toContain(BT);
});

test("each_list_decides_alone_whether_the_old_place_moves", () => {
  const rect = { left: 0, top: 0, right: 1, bottom: 1 };
  const before = settings({
    attachments: [{ device_id: IPAD_HOST, monitor_id: "M1", edge: "Right" }, { device_id: BT, monitor_id: "M1", edge: "Bottom" }],
    layout: [{ device_id: BT, rect }],
  });
  const after = migrateIpad(before, [], [host(BT, "iPad"), wifi]);
  expect(after.layout, "ipad-wifi had no layout entry, so the old one moves").toEqual([{ device_id: IPAD_HOST, rect }]);
  expect(after.attachments.map((a) => a.edge), "ipad-wifi had an attachment, so the old one goes").toEqual(["Right"]);
});

test("ipad_states_read_plainly", () => {
  expect(ipadLine("OnCall")).toBe("iPad is on a call");
  expect(ipadLine("NeedsUsb")).toBe("Plug in the iPad once");
  expect(ipadLine("NotFound")).toBe("iPad not found on Wi-Fi");
  expect(ipadLine("NeedsDeveloperMode")).toBe("Turn on Developer Mode");
  expect(ipadLine("Ready")).toBe("Connected over Wi-Fi");
  expect(ipadLine(null)).toBe("Connecting…");
});

test("the Wi-Fi iPad ignores Bluetooth being off", () => {
  const [d] = buildDevices(status([host(IPAD_HOST, "iPad")]), [], settings({ attachments: [{ device_id: IPAD_HOST, monitor_id: "M1", edge: "Right" }] }), [], {}, mon);
  expect(deviceLook(d, null, true)).toEqual({ sub: "Connected", off: false });
  const l = panelLines(d, { active: null, btOff: true, clip: null, where: "right of Screen 1", hotkey: 1 });
  expect(l.hid).toBe("Wi-Fi, right of Screen 1");
  expect(l.canSwitch).toBe(true);
  const [off] = buildDevices(status([host(IPAD_HOST, "iPad", false)]), [], settings(), [], {}, mon);
  expect(panelLines(off, { active: null, btOff: false, clip: null, where: "", hotkey: null, ipad: "NotFound" }).hid).toBe("iPad not found on Wi-Fi");
});

test("a device beside another reads as placed, with its neighbour named", () => {
  const s = settings({
    attachments: [{ device_id: "AA:1", monitor_id: "M1", edge: "Right" }],
    layout: [
      { device_id: "AA:1", rect: { left: 100, top: 0, right: 200, bottom: 100 } },
      { device_id: "BB:2", rect: { left: 200, top: 0, right: 300, bottom: 100 } },
    ],
  });
  const d = buildDevices(status([host("AA:1", "iPad"), host("BB:2", "Pixel")]), [], s, [], {}, mon);
  const pixel = d.find((x) => x.hostId === "BB:2")!;
  expect(pixel.beside).toEqual({ anchor: "AA:1", edge: "Right" });
  expect(deviceLook(pixel, null, false).sub).toBe("Connected");
  expect(panelLines(pixel, { active: null, btOff: false, clip: null, where: "right of iPad", hotkey: 2 }).key).toBe("Ctrl+Alt+2");
});
