/** One frame of a damped spring toward `target`. */
export function step(s: { x: number; v: number }, target: number, k = 0.16, f = 0.66): { x: number; v: number } {
  const v = (s.v + (target - s.x) * k) * f;
  return { x: s.x + v, v };
}
