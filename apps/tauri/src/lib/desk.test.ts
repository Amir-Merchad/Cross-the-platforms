import { expect, test } from "vitest";
import type { Attachment, Monitor, Settings } from "./api";
import { deskPlan, deviceSize, edgeBar, fileTarget, hotkeyNumber, keyboardScreen, magnet, moveDevice, nearestEdge, placedLine, placementChange, screenBoxes, slotFor } from "./desk";
import { forgetDevice, type Device } from "./devices";

const a = (device_id: string, monitor_id: string, edge: Attachment["edge"]): Attachment => ({ device_id, monitor_id, edge });
const screens = [{ id: "M1", box: { x: 100, y: 100, w: 200, h: 120 } }];
const size = { w: 60, h: 40 };

test("nearestEdge finds an edge within 170 and none beyond", () => {
  const right = slotFor(screens[0].box, "Right", size);
  const c = { x: right.x + 30, y: right.y + 20 };
  expect(nearestEdge(c, size, screens)).toEqual({ monitorId: "M1", edge: "Right" });
  expect(nearestEdge({ x: c.x + 1000, y: c.y }, size, screens)).toBeNull();
});

test("moveDevice onto a taken edge swaps the two", () => {
  const out = moveDevice([a("x", "M1", "Left"), a("y", "M1", "Right")], "x", { monitorId: "M1", edge: "Right" });
  expect(out).toEqual([a("x", "M1", "Right"), a("y", "M1", "Left")]);
});

test("an unplaced device taking an edge unplaces the other", () => {
  const out = moveDevice([a("y", "M1", "Right")], "x", { monitorId: "M1", edge: "Right" });
  expect(out).toEqual([a("x", "M1", "Right")]);
});

test("null unplaces and keeps the order of the rest", () => {
  const out = moveDevice([a("x", "M1", "Left"), a("y", "M1", "Right"), a("z", "M1", "Top")], "y", null);
  expect(out.map((p) => p.device_id)).toEqual(["x", "z"]);
});

test("hotkeyNumber follows list order", () => {
  const l = [a("x", "M1", "Left"), a("y", "M1", "Right")];
  expect(hotkeyNumber(l, "y")).toBe(2);
  expect(hotkeyNumber(l, "nope")).toBeNull();
});

test("screenBoxes scales to fit and centres", () => {
  const m: Monitor[] = [{ id: "M1", rect: { left: 0, top: 0, right: 1000, bottom: 500 } }];
  const [b] = screenBoxes(m, { w: 500, h: 500 });
  expect(b.box).toEqual({ x: 0, y: 125, w: 500, h: 250 });
});

test("magnet pulls 35% of the way", () => {
  expect(magnet({ x: 0, y: 0 }, { x: 100, y: 200, w: 1, h: 1 })).toEqual({ x: 35, y: 70 });
});

test("placementChange: unplacing the device in control means Windows first", () => {
  const next = moveDevice([a("x", "M1", "Left"), a("y", "M1", "Right")], "x", null);
  expect(placementChange("x", next)).toBeNull();
});

test("placementChange: forgetting the device in control means Windows first", () => {
  const before = { attachments: [a("x", "M1", "Left")], device_links: {}, device_colors: {} } as unknown as Settings;
  const next = forgetDevice(before, { hostId: "x", peerId: null } as Device);
  expect(placementChange("x", next.attachments)).toBeNull();
});

test("placementChange: losing an edge to an unplaced device means Windows first", () => {
  expect(placementChange("y", moveDevice([a("y", "M1", "Right")], "x", { monitorId: "M1", edge: "Right" }))).toBeNull();
});

test("placementChange: a swap, another device or no device in control changes nothing", () => {
  const swapped = moveDevice([a("x", "M1", "Left"), a("y", "M1", "Right")], "x", { monitorId: "M1", edge: "Right" });
  expect(placementChange("y", swapped)).toBeUndefined();
  expect(placementChange("x", moveDevice([a("x", "M1", "Left"), a("y", "M1", "Right")], "y", null))).toBeUndefined();
  expect(placementChange(null, [])).toBeUndefined();
});

test("fileTarget picks the nearest device within 34 px, and none beyond", () => {
  const rects = [{ key: "a", left: 100, top: 100, right: 200, bottom: 200 }, { key: "b", left: 260, top: 100, right: 360, bottom: 200 }];
  expect(fileTarget({ x: 150, y: 150 }, rects)).toBe("a");
  expect(fileTarget({ x: 225, y: 150 }, rects)).toBe("a");
  expect(fileTarget({ x: 240, y: 150 }, rects)).toBe("b");
  expect(fileTarget({ x: 150, y: 235 }, rects)).toBeNull();
});

const dev = (key: string, o: Partial<Device> = {}) => ({ key, kind: null, placement: null, ...o });
const stage = { w: 756, h: 572 };
const wide: Monitor[] = [{ id: "M1", rect: { left: 0, top: 0, right: 1600, bottom: 900 } }];

test("deskPlan puts a placed device on its slot and the rest in the tray", () => {
  const { screens, tray, homes } = deskPlan([dev("x", { placement: a("x", "M1", "Right") }), dev("y")], wide, stage);
  const slot = slotFor(screens[0].box, "Right", deviceSize(null));
  expect(homes.get("x")).toEqual({ x: slot.x, y: slot.y });
  const y = homes.get("y")!;
  expect(y.x).toBeGreaterThanOrEqual(tray.x);
  expect(y.y).toBeGreaterThanOrEqual(tray.y);
});

test("deskPlan sends a device on an unplugged screen to the tray", () => {
  const { tray, homes } = deskPlan([dev("x", { placement: a("x", "GONE", "Right"), missingScreen: true })], wide, stage);
  expect(homes.get("x")!.y).toBeGreaterThanOrEqual(tray.y);
});

test("deskPlan leaves room above the screen for a phone on the top edge", () => {
  const { homes } = deskPlan([dev("x", { kind: "Android", placement: a("x", "M1", "Top") })], wide, stage);
  expect(homes.get("x")!.y).toBeGreaterThanOrEqual(0);
});

test("tray fits two devices side by side and squeezes more inside itself", () => {
  const two = deskPlan([dev("x"), dev("y", { kind: "Android" })], wide, stage);
  expect(two.homes.get("y")!.x).toBeGreaterThanOrEqual(two.homes.get("x")!.x + deviceSize(null).w);
  const keys = ["p", "q", "r", "s", "t"];
  const plan = deskPlan(keys.map((k) => dev(k)), wide, stage);
  for (const k of keys) expect(plan.homes.get(k)!.x + deviceSize(null).w).toBeLessThanOrEqual(plan.tray.x + plan.tray.w);
});

test("edgeBar sits outside the edge and centred along it", () => {
  const box = { x: 214, y: 150, w: 320, h: 200 };
  expect(edgeBar(box, "Right", 6, 120, 5)).toEqual({ x: 539, y: 190, w: 6, h: 120 });
  expect(edgeBar(box, "Left", 6, 120, 5)).toEqual({ x: 203, y: 190, w: 6, h: 120 });
  expect(edgeBar(box, "Top", 8, 180, 4)).toEqual({ x: 284, y: 138, w: 180, h: 8 });
  expect(edgeBar(box, "Bottom", 8, 180, 4)).toEqual({ x: 284, y: 354, w: 180, h: 8 });
});

test("keyboardScreen: the picked screen, else the device's own, else the first", () => {
  const two: Monitor[] = [{ id: "M1", rect: wide[0].rect }, { id: "M2", rect: wide[0].rect }];
  expect(keyboardScreen("M2", a("x", "M1", "Left"), two)).toBe("M2");
  expect(keyboardScreen(null, a("x", "M2", "Left"), two)).toBe("M2");
  expect(keyboardScreen("GONE", a("x", "GONE", "Left"), two)).toBe("M1");
  expect(keyboardScreen(null, null, [])).toBeNull();
});

test("placedLine uses the canvas wording", () => {
  expect(placedLine("iPad Air", "Screen 1", "Right", 1)).toBe("iPad Air sits right of Screen 1. Ctrl+Alt+1 jumps to it.");
  expect(placedLine("iPad Air", "Screen 2", "Top", 3)).toBe("iPad Air sits above Screen 2. Ctrl+Alt+3 jumps to it.");
});
