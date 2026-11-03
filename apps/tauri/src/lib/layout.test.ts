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
