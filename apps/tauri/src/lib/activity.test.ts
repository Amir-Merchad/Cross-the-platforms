import { expect, test } from "vitest";
import type { TransferEvent } from "./transfer";
import { applyEvent, historyLine, plainError, waitingLine, type Row } from "./activity";

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

test("outgoing with total 0 ends done on Progress", () => {
  const r = run([{ type: "Started", id: 1, peer: "p", outgoing: true, name: "e", files: 1, total: 0, paths: [] }, { type: "Progress", id: 1, done: 0, total: 0 }]);
  expect(r[0].state).toBe("done");
});

test("a late Failed cannot flip a finished row", () => {
  const r = run([started(true), { type: "Progress", id: 1, done: 100, total: 100 }, { type: "Failed", id: 1, reason: "late" }]);
  expect(r[0].state).toBe("done");
});

const row = (o: Partial<Row>): Row => ({ id: 1, peer: "p", outgoing: true, name: "a.txt", files: 1, total: 2_000_000, done: 2_000_000, startedAt: 0, now: 1000, paths: [], state: "done", received: 0, ...o });

test("historyLine: sent, received and failed rows", () => {
  expect(historyLine(row({}), "iPad Air", true)).toEqual({ file: "a.txt", who: "To iPad Air", size: "2.0 MB", result: "Sent at 2.0 MB/s", failed: false });
  expect(historyLine(row({ outgoing: false }), "Galaxy", false)).toMatchObject({ who: "From Galaxy", result: "Received", failed: false });
  expect(historyLine(row({ state: "requeued" }), "iPad Air", true).result).toBe("Waiting. It sends when Bridge opens there.");
  const f = historyLine(row({ state: "failed", reason: "os error 5 at C:/secret/x" }), "iPad Air", true);
  expect(f).toMatchObject({ result: "That did not finish. Send it again.", failed: true });
  expect(historyLine(row({ state: "failed", reason: "peer not reachable" }), "iPad Air", false).result).toBe("Not reachable right now. Open Bridge on the other device, then send again.");
});

test("historyLine: a failed row with no peer has no who", () => {
  expect(historyLine(row({ peer: "", state: "failed", reason: "x" }), undefined, false).who).toBe("");
});

test("waitingLine", () => {
  expect(waitingLine(1, "iPad Air")).toEqual(["1 file waiting for iPad Air", "It sends when Bridge opens there."]);
  expect(waitingLine(2, "iPad Air")).toEqual(["2 files waiting for iPad Air", "They send when Bridge opens there."]);
});
