// Pure geometry and placement rules for the desk. No React, no Tauri.
import type { Attachment, Edge, Monitor } from "./api";

export type Box = { x: number; y: number; w: number; h: number };
type Size = { w: number; h: number };
type Screen = { id: string; box: Box };
const EDGES: Edge[] = ["Left", "Right", "Top", "Bottom"];

/** Monitors drawn to scale, fitted into `area` and centred. */
export function screenBoxes(monitors: Monitor[], area: Size): Screen[] {
  if (monitors.length === 0) return [];
  const left = Math.min(...monitors.map((m) => m.rect.left));
  const top = Math.min(...monitors.map((m) => m.rect.top));
  const w = Math.max(...monitors.map((m) => m.rect.right)) - left;
  const h = Math.max(...monitors.map((m) => m.rect.bottom)) - top;
  const s = Math.min(area.w / w, area.h / h);
  const ox = (area.w - w * s) / 2;
  const oy = (area.h - h * s) / 2;
  return monitors.map((m) => ({
    id: m.id,
    box: { x: ox + (m.rect.left - left) * s, y: oy + (m.rect.top - top) * s, w: (m.rect.right - m.rect.left) * s, h: (m.rect.bottom - m.rect.top) * s },
  }));
}

/** Where a device of `size` sits when attached to `edge` of `screen`, centred along it. */
export function slotFor(screen: Box, edge: Edge, size: Size, gap = 14): Box {
  const cx = screen.x + (screen.w - size.w) / 2;
  const cy = screen.y + (screen.h - size.h) / 2;
  switch (edge) {
    case "Left": return { x: screen.x - gap - size.w, y: cy, ...size };
    case "Right": return { x: screen.x + screen.w + gap, y: cy, ...size };
    case "Top": return { x: cx, y: screen.y - gap - size.h, ...size };
    case "Bottom": return { x: cx, y: screen.y + screen.h + gap, ...size };
  }
}

/** The closest edge slot to `centre` within `range`, or null. */
export function nearestEdge(centre: { x: number; y: number }, size: Size, screens: Screen[], range = 170): { monitorId: string; edge: Edge } | null {
  let best: { monitorId: string; edge: Edge } | null = null;
  let bestD = range;
  for (const s of screens) {
    for (const edge of EDGES) {
      const slot = slotFor(s.box, edge, size);
      const d = Math.hypot(centre.x - (slot.x + slot.w / 2), centre.y - (slot.y + slot.h / 2));
      if (d <= bestD) { bestD = d; best = { monitorId: s.id, edge }; }
    }
  }
  return best;
}

/** Pulls a dragged position `pull` of the way to the top-left of the slot. */
export function magnet(pos: { x: number; y: number }, slot: Box, pull = 0.35): { x: number; y: number } {
  return { x: pos.x + (slot.x - pos.x) * pull, y: pos.y + (slot.y - pos.y) * pull };
}

/** Places, moves or (target null) unplaces a device. A taken edge swaps, or unplaces its owner when the mover had no place. List order is kept. */
export function moveDevice(attachments: Attachment[], id: string, target: { monitorId: string; edge: Edge } | null): Attachment[] {
  if (!target) return attachments.filter((a) => a.device_id !== id);
  const mine = attachments.find((a) => a.device_id === id);
  const taken = attachments.find((a) => a.device_id !== id && a.monitor_id === target.monitorId && a.edge === target.edge);
  const next: Attachment = { device_id: id, monitor_id: target.monitorId, edge: target.edge };
  const out = attachments.flatMap((a): Attachment[] => {
    if (a === mine) return [next];
    if (a === taken) return mine ? [{ ...a, monitor_id: mine.monitor_id, edge: mine.edge }] : [];
    return [a];
  });
  return mine ? out : [...out, next];
}

/** 1-based hotkey number, following list order. */
export function hotkeyNumber(attachments: Attachment[], id: string): number | null {
  const i = attachments.findIndex((a) => a.device_id === id);
  return i < 0 ? null : i + 1;
}
