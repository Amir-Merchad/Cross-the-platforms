import { expect, test } from "vitest";
import type { TransferEvent } from "./transfer";
import { applyEvent, plainError, type Row } from "./activity";

const run = (evs: TransferEvent[]) => evs.reduce<Row[]>((r, e, i) => applyEvent(r, e, 1000 + i), []);
const started = (outgoing: boolean, files = 2): TransferEvent => ({ type: "Started", id: 1, peer: "p", outgoing, name: "a.txt", files, total: 100, paths: ["C:/a.txt"] });
const recv: TransferEvent = { type: "Received", from: "p", path: "x", size: 50 };

test("incoming ends done once all files are received", () => {
  const mid = run([started(false), { type: "Progress", id: 1, done: 100, total: 100 }, recv]);
  expect(mid[0].state).toBe("moving");
  expect(run([started(false), { type: "Progress", id: 1, done: 100, total: 100 }, recv, recv])[0]).toMatchObject({ state: "done", done: 100 });
});

test("outgoing ends done on the final Progress", () => {
  const r = run([started(true), { type: "Progress", id: 1, done: 40, total: 100 }]);
  expect(r[0]).toMatchObject({ state: "moving", done: 40 });
  expect(run([started(true), { type: "Progress", id: 1, done: 100, total: 100 }])[0].state).toBe("done");
});

test("Requeued ends requeued", () => {
  expect(run([started(true), { type: "Requeued", id: 1 }])[0].state).toBe("requeued");
});

test("Failed after Started marks the row failed with a reason", () => {
  expect(run([started(true), { type: "Failed", id: 1, reason: "boom" }])[0]).toMatchObject({ state: "failed", reason: "boom" });
});

test("Failed with an unknown id adds a failed row named A file", () => {
  const r = run([{ type: "Failed", id: 9, reason: "boom" }]);
  expect(r).toHaveLength(1);
  expect(r[0]).toMatchObject({ id: 9, name: "A file", state: "failed" });
});

test("plainError hides the raw string and says queued only when it can", () => {
  const raw = "peer is not reachable: timeout 0x5";
  expect(plainError(raw, false)).not.toContain("0x5");
  expect(plainError(raw, false).toLowerCase()).not.toContain("queued");
  expect(plainError(raw, true).toLowerCase()).toContain("queued");
  expect(plainError("weird internal", true)).not.toContain("weird");
});
