import { expect, test } from "vitest";
import type { Settings } from "./api";
import { loadPeers, makeSaver, makeStore, resolveDark } from "./hub";

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

test("loadPeers gives null when the transfer service is down, so the hub can stop waiting", async () => {
  await expect(loadPeers(() => Promise.reject("file transfer isn't running"))).resolves.toBeNull();
  await expect(loadPeers(() => Promise.resolve([]))).resolves.toEqual([]);
});

test("anything but light or dark counts as system", () => {
  expect(resolveDark("light", true)).toBe(false);
  expect(resolveDark("dark", false)).toBe(true);
  expect(resolveDark("system", true)).toBe(true);
  expect(resolveDark("weird", false)).toBe(false);
  expect(resolveDark("weird", true)).toBe(true);
});

const store = (failOn: number[]) => {
  const disk: number[] = [];
  const ui: Settings[] = [];
  const errs: string[] = [];
  const st = makeStore(s(0), async (x) => { if (failOn.includes(x.version)) throw new Error("no"); disk.push(x.version); }, (x) => ui.push(x), (e) => errs.push(String(e)));
  return { st, disk, ui, errs };
};

test("A fails then B (which includes A) succeeds: UI ends equal to what was saved", async () => {
  const { st, disk, ui, errs } = store([1]);
  const a = st.update((x) => ({ ...x, version: 1 }));
  const b = st.update((x) => ({ ...x, version: x.version + 1 })); // builds on A: version 2
  await Promise.all([a, b]);
  expect(disk).toEqual([2]);
  expect(ui.at(-1)!.version).toBe(2);
  expect(st.current().version).toBe(2);
  expect(errs).toEqual([]);
});

test("B fails after A saved: restore to A once and report", async () => {
  const { st, ui, errs } = store([2]);
  await Promise.all([st.update((x) => ({ ...x, version: 1 })), st.update((x) => ({ ...x, version: 2 }))]);
  expect(st.current().version).toBe(1);
  expect(ui.at(-1)!.version).toBe(1);
  expect(errs.length).toBe(1);
});
