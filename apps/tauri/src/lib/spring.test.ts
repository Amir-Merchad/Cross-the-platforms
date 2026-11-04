import { expect, test } from "vitest";
import { STEP_MS, step, ticks } from "./spring";

test("settles within 1px in 30 frames, overshoots under 15%", () => {
  let s = { x: 0, v: 0 };
  let max = 0;
  for (let i = 0; i < 30; i++) { s = step(s, 100); max = Math.max(max, s.x); }
  expect(Math.abs(s.x - 100)).toBeLessThan(1);
  expect(max).toBeLessThanOrEqual(115);
});

/** Steps run in one second of frames at `hz`. */
function stepsIn(hz: number) {
  let carry = 0, total = 0;
  for (let i = 0; i < hz; i++) {
    const t = ticks(carry, 1000 / hz);
    carry = t.carry;
    total += t.steps;
  }
  return total;
}

test("ticks runs the spring at 60 Hz whatever the screen refresh", () => {
  for (const hz of [30, 60, 75, 144, 240]) expect(Math.abs(stepsIn(hz) - 60)).toBeLessThanOrEqual(1);
});

test("ticks keeps the remainder for the next frame", () => {
  expect(ticks(0, STEP_MS / 2)).toEqual({ steps: 0, carry: STEP_MS / 2 });
  const next = ticks(STEP_MS / 2, STEP_MS / 2);
  expect(next.steps).toBe(1);
  expect(next.carry).toBeCloseTo(0);
});

test("ticks caps a long gap, so a stalled window does not jump", () => {
  expect(ticks(0, 5000).steps).toBe(6);
  expect(ticks(0, 5000).carry).toBe(0);
});
