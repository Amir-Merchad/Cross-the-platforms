// In-memory transfer history for the session, folded from core events. No React.
import type { TransferEvent } from "./transfer";

export type Row = {
  id: number; peer: string; outgoing: boolean; name: string; files: number; total: number; done: number;
  startedAt: number; now: number; paths: string[]; state: "moving" | "done" | "failed" | "requeued"; reason?: string;
  received: number; // files received so far, to end incoming rows
};

const patch = (rows: Row[], id: number, f: (r: Row) => Partial<Row>) => rows.map((r) => (r.id === id ? { ...r, ...f(r) } : r));

export function applyEvent(rows: Row[], e: TransferEvent, now: number): Row[] {
  switch (e.type) {
    case "Started":
      return [...rows.filter((r) => r.id !== e.id), {
        id: e.id, peer: e.peer, outgoing: e.outgoing, name: e.name, files: e.files, total: e.total, done: 0,
        startedAt: now, now, paths: e.paths, state: "moving", received: 0,
      }];
    case "Progress":
      return patch(rows, e.id, (r) => ({ done: e.done, total: e.total, now, state: r.outgoing && e.total > 0 && e.done >= e.total ? "done" : r.state }));
    case "Received": {
      const row = rows.find((r) => !r.outgoing && r.peer === e.from && r.state === "moving");
      if (!row) return rows;
      const received = row.received + 1;
      return patch(rows, row.id, () => (received >= row.files ? { received, now, done: row.total, state: "done" } : { received, now }));
    }
    case "Requeued":
      return patch(rows, e.id, () => ({ state: "requeued", now }));
    case "Failed":
      return rows.some((r) => r.id === e.id)
        ? patch(rows, e.id, () => ({ state: "failed", reason: e.reason, now }))
        : [...rows, { id: e.id, peer: "", outgoing: false, name: "A file", files: 1, total: 0, done: 0, startedAt: now, now, paths: [], state: "failed", reason: e.reason, received: 0 }];
    default:
      return rows;
  }
}

/** A sentence for the user. Never the raw reason, which can carry paths or internals. */
export function plainError(reason: string, canQueue: boolean): string {
  if (/not reachable/i.test(reason))
    return canQueue ? "Not reachable right now. It is queued and goes when Bridge opens there." : "Not reachable right now. Open Bridge on the other device, then send again.";
  return "That did not finish. Send it again.";
}
