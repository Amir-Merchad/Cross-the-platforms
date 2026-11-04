import { expect, test } from "vitest";
import type { Attachment, Monitor } from "./api";
import { hotkeyNumber, magnet, moveDevice, nearestEdge, screenBoxes, slotFor } from "./desk";

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
