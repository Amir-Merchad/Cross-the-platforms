import { expect, test } from "vitest";
import { clashes } from "./layout";

test("same edge twice clashes", () => {
  expect(clashes([{ device_id: "a", monitor_id: "M1", edge: "Right" }, { device_id: "b", monitor_id: "M1", edge: "Right" }])).toMatch(/Right/);
});
test("distinct edges are fine", () => {
  expect(clashes([{ device_id: "a", monitor_id: "M1", edge: "Right" }, { device_id: "b", monitor_id: "M1", edge: "Left" }])).toBeNull();
});
test("same device twice clashes", () => {
  expect(clashes([{ device_id: "a", monitor_id: "M1", edge: "Right" }, { device_id: "a", monitor_id: "M1", edge: "Left" }])).not.toBeNull();
});

// Final review I9: editing a placement keeps its position, so its hotkey number doesn't change.
import { placeDevice } from "./layout";
test("re-placing keeps hotkey order", () => {
  const before = [{ device_id: "ipad", monitor_id: "M1", edge: "Right" as const }, { device_id: "s24", monitor_id: "M1", edge: "Left" as const }];
  expect(placeDevice(before, "ipad", "M1", "Top").map((a) => a.device_id)).toEqual(["ipad", "s24"]);
});
test("new device goes last, unplacing removes", () => {
  const before = [{ device_id: "ipad", monitor_id: "M1", edge: "Right" as const }];
  expect(placeDevice(before, "s24", "M1", "Left").map((a) => a.device_id)).toEqual(["ipad", "s24"]);
  expect(placeDevice(before, "ipad", null, null)).toEqual([]);
});
