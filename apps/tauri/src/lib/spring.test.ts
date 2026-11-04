import { expect, test } from "vitest";
import { step } from "./spring";

test("settles within 1px in 30 frames, overshoots under 15%", () => {
  let s = { x: 0, v: 0 };
  let max = 0;
  for (let i = 0; i < 30; i++) { s = step(s, 100); max = Math.max(max, s.x); }
  expect(Math.abs(s.x - 100)).toBeLessThan(1);
  expect(max).toBeLessThanOrEqual(115);
});
