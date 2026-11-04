// Pure geometry and placement rules for the desk. No React, no Tauri.
import type { Attachment, Edge, Monitor, Rect } from "./api";
import type { Device } from "./devices";
import type { PeerKind } from "./transfer";

export type Box = { x: number; y: number; w: number; h: number };
type Size = { w: number; h: number };
type Point = { x: number; y: number };
export type Screen = { id: string; box: Box };
export const EDGES: Edge[] = ["Left", "Right", "Top", "Bottom"];

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

/**
 * False when monitors beyond `edge` cover all of it. layout::entry gives the pointer to a neighbour wherever one
 * abuts, so a device on a fully covered edge could never be entered. A partly covered edge still works on the rest.
 */
export function edgeOpen(monitors: Monitor[], id: string, edge: Edge): boolean {
  const r = monitors.find((m) => m.id === id)?.rect;
  if (!r) return true;
  const upright = edge === "Left" || edge === "Right";
  const touches = (o: Rect) => (edge === "Right" ? o.left === r.right : edge === "Left" ? o.right === r.left : edge === "Top" ? o.bottom === r.top : o.top === r.bottom);
  const span = (o: Rect) => (upright ? [o.top, o.bottom] : [o.left, o.right]);
  const [from, to] = span(r);
  const covers = monitors.filter((m) => m.id !== id && touches(m.rect)).map((m) => span(m.rect)).sort((a, b) => a[0] - b[0]);
  let reach = from;
  for (const [start, end] of covers) {
    if (start > reach) break;
    reach = Math.max(reach, end);
  }
  return reach < to;
}

/** The closest open edge slot to `centre` within `range`, or null. */
export function nearestEdge(centre: { x: number; y: number }, size: Size, screens: Screen[], monitors: Monitor[], range = 170): { monitorId: string; edge: Edge } | null {
  let best: { monitorId: string; edge: Edge } | null = null;
  let bestD = range;
  for (const s of screens) {
    for (const edge of EDGES) {
      if (!edgeOpen(monitors, s.id, edge)) continue;
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

/** null when the device in control has lost its place: return input to Windows before saving. */
export function placementChange(activeHostId: string | null, next: Attachment[]): null | undefined {
  return activeHostId && !next.some((a) => a.device_id === activeHostId) ? null : undefined;
}

/** The device nearest to `at`, measured to its edge, within `range`. */
export function fileTarget(at: Point, rects: { key: string; left: number; top: number; right: number; bottom: number }[], range = 34): string | null {
  let best: string | null = null;
  let bestD = range;
  for (const r of rects) {
    const d = Math.hypot(Math.max(r.left - at.x, 0, at.x - r.right), Math.max(r.top - at.y, 0, at.y - r.bottom));
    if (d <= bestD) { bestD = d; best = r.key; }
  }
  return best;
}

export const deviceSize = (kind: PeerKind | null): Size => (kind === "Android" ? { w: 84, h: 150 } : { w: 150, h: 112 });

const ROOM = { side: 170, top: 170, bottom: 200 }; // around the screens, so a device fits on every outer edge
const TRAY = { inset: 12, h: 184, minW: 190, maxW: 290, left: 18, right: 12, bottom: 6, gap: 8 };

/** Screens, the tray and where every device rests, for a map of size `stage`. */
export function deskPlan(devices: Pick<Device, "key" | "kind" | "placement">[], monitors: Monitor[], stage: Size): { screens: Screen[]; tray: Box; homes: Map<string, Point> } {
  const area = { w: Math.max(stage.w - 2 * ROOM.side, 1), h: Math.max(stage.h - ROOM.top - ROOM.bottom, 1) };
  const screens = screenBoxes(monitors, area).map((s) => ({ ...s, box: { ...s.box, x: s.box.x + ROOM.side, y: s.box.y + ROOM.top } }));
  const homes = new Map<string, Point>();
  const loose: { key: string; size: Size }[] = [];
  for (const d of devices) {
    const size = deviceSize(d.kind);
    const screen = screens.find((s) => s.id === d.placement?.monitor_id);
    if (!screen) { loose.push({ key: d.key, size }); continue; }
    const { x, y } = slotFor(screen.box, d.placement!.edge, size);
    homes.set(d.key, { x, y });
  }
  // ponytail: past two or three devices the tray overlaps them; make it scroll if people keep that many unplaced.
  const advance = loose.reduce((n, l) => n + l.size.w + TRAY.gap, 0);
  const last = loose.at(-1)?.size.w ?? 0;
  const need = TRAY.left + advance - TRAY.gap + TRAY.right;
  const w = Math.min(Math.max(need, TRAY.minW), TRAY.maxW);
  const tray = { x: TRAY.inset, y: stage.h - TRAY.inset - TRAY.h, w, h: TRAY.h };
  const squeeze = need > w ? (w - TRAY.left - TRAY.right - last) / (advance - last - TRAY.gap) : 1;
  let x = tray.x + TRAY.left;
  for (const l of loose) {
    homes.set(l.key, { x, y: tray.y + TRAY.h - TRAY.bottom - l.size.h });
    x += (l.size.w + TRAY.gap) * squeeze;
  }
  return { screens, tray, homes };
}

/** A bar `thick` by `length`, `off` px outside `edge` of `screen` and centred along it. */
export function edgeBar(screen: Box, edge: Edge, thick: number, length: number, off: number): Box {
  const x = screen.x + (screen.w - length) / 2;
  const y = screen.y + (screen.h - length) / 2;
  switch (edge) {
    case "Left": return { x: screen.x - off - thick, y, w: thick, h: length };
    case "Right": return { x: screen.x + screen.w + off, y, w: thick, h: length };
    case "Top": return { x, y: screen.y - off - thick, w: length, h: thick };
    case "Bottom": return { x, y: screen.y + screen.h + off, w: length, h: thick };
  }
}

/** The screen arrow keys place on: the picked one, else the device's own, else the first. */
export function keyboardScreen(picked: string | null, placement: Attachment | null, monitors: Monitor[]): string | null {
  const live = (id?: string | null) => monitors.some((m) => m.id === id);
  if (live(picked)) return picked;
  return live(placement?.monitor_id) ? placement!.monitor_id : monitors[0]?.id ?? null;
}

const WORDS: Record<Edge, string> = { Right: "right of", Left: "left of", Top: "above", Bottom: "below" };

export const screenName = (monitors: Monitor[], id: string) => `Screen ${monitors.findIndex((m) => m.id === id) + 1}`;
export const whereLine = (edge: Edge, screen: string) => `${WORDS[edge]} ${screen}`;
export const placedLine = (name: string, screen: string, edge: Edge, hotkey: number) => `${name} sits ${whereLine(edge, screen)}. Ctrl+Alt+${hotkey} jumps to it.`;
