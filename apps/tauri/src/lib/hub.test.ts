import { expect, test } from "vitest";
import type { Settings } from "./api";
import { makeSaver, resolveDark } from "./hub";

const s = (n: number) => ({ version: n }) as Settings;
const gate = () => { let open!: () => void; const p = new Promise<void>((r) => (open = r)); return { p, open }; };

test("quick saves land in order and the last value wins", async () => {
  const seen: number[] = [];
  const g = gate();
  const save = makeSaver(async (x) => { if (x.version === 1) await g.p; seen.push(x.version); });
  const a = save(s(1)), b = save(s(2));
  g.open();
  await Promise.all([a, b]);
  expect(seen).toEqual([1, 2]);
});

test("a failed save rejects but later saves still run", async () => {
  const seen: number[] = [];
  const save = makeSaver(async (x) => { if (x.version === 1) throw new Error("no"); seen.push(x.version); });
  await expect(save(s(1))).rejects.toThrow("no");
  await save(s(2));
  expect(seen).toEqual([2]);
});

test("anything but light or dark counts as system", () => {
  expect(resolveDark("light", true)).toBe(false);
  expect(resolveDark("dark", false)).toBe(true);
  expect(resolveDark("system", true)).toBe(true);
  expect(resolveDark("weird", false)).toBe(false);
  expect(resolveDark("weird", true)).toBe(true);
});
