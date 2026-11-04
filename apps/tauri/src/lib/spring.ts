export const STEP_MS = 1000 / 60;
const MAX_STEPS = 6; // a stalled window catches up by 100 ms at most, so nothing jumps

/** How many fixed 60 Hz steps `elapsed` ms pays for, and the time left over for next frame. */
export function ticks(carry: number, elapsed: number): { steps: number; carry: number } {
  const total = carry + elapsed;
  const steps = Math.floor(total / STEP_MS);
  return steps > MAX_STEPS ? { steps: MAX_STEPS, carry: 0 } : { steps, carry: total - steps * STEP_MS };
}

/** One 60 Hz step of a damped spring toward `target`. */
export function step(s: { x: number; v: number }, target: number, k = 0.16, f = 0.66): { x: number; v: number } {
  const v = (s.v + (target - s.x) * k) * f;
  return { x: s.x + v, v };
}
