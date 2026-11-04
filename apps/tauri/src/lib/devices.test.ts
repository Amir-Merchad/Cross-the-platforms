import { expect, test } from "vitest";
import type { HostStatus, Monitor, Settings, Status } from "./api";
import type { Peer } from "./transfer";
import { buildDevices, displayName, forgetDevice, linkByName } from "./devices";

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
