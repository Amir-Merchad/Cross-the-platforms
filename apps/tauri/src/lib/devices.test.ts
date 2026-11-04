import { expect, test } from "vitest";
import type { Row } from "./activity";
import type { ClipStatus, HostStatus, Monitor, Settings, Status } from "./api";
import type { Peer } from "./transfer";
import { buildDevices, deskLine, deviceLook, displayName, forgetDevice, idleLine, linkByName, panelLines, type Device } from "./devices";

const host = (id: string, name: string | null, connected = true): HostStatus => ({ id, name, connected, interval_ms: 0, notify_avg_ms: 0, notify_max_ms: 0 });
const status = (hosts: HostStatus[]): Status => ({ advertising: true, hosts, active: null, notice: null });
const settings = (o: Partial<Settings> = {}): Settings => ({
  version: 1, attachments: [], calibration: {}, entry_hold_ms: 0, start_with_windows: false, remote_width: {},
  clipboard_sync: true, receive_dir: null, device_links: {}, device_colors: {}, appearance: "system", ...o,
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
  placement: { device_id: "h", monitor_id: "M1", edge: "Right" }, missingScreen: false, colour: "#FF7A4D", ...o,
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
  expect(look(dev({ connected: false }))).toEqual({ sub: "Files only", off: false });
});

const clip = (o: Partial<ClipStatus> = {}): ClipStatus => ({ enabled: true, ipad: { Connected: { wifi: true } }, synced: 0, skipped_too_large: 0, notice: null, ...o });
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
  expect([l.key, l.canSwitch, l.why, l.hid]).toEqual(["Screen not connected", false, "Its screen is not connected.", "Bluetooth, screen not connected"]);
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
const line = (o: { status?: Status | null; rows?: Row[]; selected?: Device | null; btOff?: boolean } = {}) =>
  deskLine(o.status === undefined ? stat() : o.status, o.rows ?? [], [dev()], o.selected ?? null, o.btOff ?? false);

test("deskLine follows the canvas states, and input comes first", () => {
  expect(line()).toBe("Your keyboard and mouse are on Windows.");
  expect(line({ rows: [moving()] })).toBe("Sending Site photos.zip to iPad Air at 84.0 MB/s.");
  expect(line({ status: stat({ active: "h" }), rows: [moving()] })).toBe("Your keyboard and mouse are on iPad Air. Push back across the edge, or press Ctrl+Alt+Q.");
  expect(line({ btOff: true, status: stat({ notice: "Bluetooth advertising restarting…" }) })).toBe("Keyboard and mouse sharing is paused.");
  expect(line({ selected: dev({ connected: false, reachable: false }) })).toBe("iPad Air went offline. Bridge reconnects by itself.");
  expect(line({ selected: dev({ reachable: false, waiting: 3 }) })).toBe("iPad Air has Bridge closed. Files will wait.");
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
