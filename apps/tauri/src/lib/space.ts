// Free layout in the hub: mirrors core/src/space.rs. Monitors and devices are rectangles in desktop pixels, joined
// where edges touch. No React.
import type { Attachment, Edge, Monitor, Placement, Rect, Settings } from "./api";

export const DEFAULT_WIDTH_PX = 1366;
/** Device height (or depth above and below a screen) as a share of its width: an iPad in landscape. */
const ASPECT = 3 / 4;

const widthOf = (s: Settings, id: string) => Math.max(1, Math.round(s.remote_width[id] ?? DEFAULT_WIDTH_PX));
const overlap = (a0: number, a1: number, b0: number, b1: number) => Math.max(a0, b0) < Math.min(a1, b1);
const intersects = (a: Rect, b: Rect) => overlap(a.left, a.right, b.left, b.right) && overlap(a.top, a.bottom, b.top, b.bottom);

/** The edge of `a` that `b` touches with a shared span; corners alone do not touch. */
export function touching(a: Rect, b: Rect): Edge | null {
  const v = overlap(a.top, a.bottom, b.top, b.bottom), h = overlap(a.left, a.right, b.left, b.right);
  if (v && a.right === b.left) return "Right";
  if (v && a.left === b.right) return "Left";
  if (h && a.bottom === b.top) return "Bottom";
  if (h && a.top === b.bottom) return "Top";
  return null;
}

/**
 * A device `width` px wide beside `edge` of `r`, covering that whole edge, as core's made-up placements do, so
 * the pointer crosses anywhere along it.
 */
export function slotRect(r: Rect, edge: Edge, width: number): Rect {
  const depth = Math.max(1, Math.round(width * ASPECT));
  switch (edge) {
    case "Right": return { ...r, left: r.right, right: r.right + width };
    case "Left": return { ...r, left: r.left - width, right: r.left };
    case "Top": return { ...r, top: r.top - depth, bottom: r.top };
    case "Bottom": return { ...r, top: r.bottom, bottom: r.bottom + depth };
  }
}

/** The layout in use: the saved one, else made from attachments (devices on missing screens left out). */
export function effectiveLayout(s: Settings, monitors: Monitor[]): Placement[] {
  if (s.layout?.length) return s.layout;
  return s.attachments.flatMap((a) => {
    const m = monitors.find((x) => x.id === a.monitor_id);
    return m ? [{ device_id: a.device_id, rect: slotRect(m.rect, a.edge, widthOf(s, a.device_id)) }] : [];
  });
}

/** Each device once, never overlapping another device or a monitor. */
export function layoutValid(layout: Placement[], monitors: Monitor[]): boolean {
  return layout.every((p, i) =>
    p.rect.right > p.rect.left && p.rect.bottom > p.rect.top
    && layout.slice(i + 1).every((q) => q.device_id !== p.device_id && !intersects(p.rect, q.rect))
    && !monitors.some((m) => intersects(p.rect, m.rect)));
}

/** Where each device placed beside another device hangs: its anchor device and which of the anchor's edges. */
export function besideMap(s: Settings, monitors: Monitor[]): Map<string, { anchor: string; edge: Edge }> {
  const out = new Map<string, { anchor: string; edge: Edge }>();
  const layout = s.layout ?? [];
  const onScreen = new Set(layout.filter((p) => monitors.some((m) => touching(p.rect, m.rect))).map((p) => p.device_id));
  // Breadth first from devices that touch a screen, so every chain hangs from something drawn.
  let frontier = [...onScreen];
  const seen = new Set(onScreen);
  while (frontier.length) {
    const next: string[] = [];
    for (const id of frontier) {
      const a = layout.find((p) => p.device_id === id)!;
      for (const p of layout) {
        if (seen.has(p.device_id)) continue;
        const edge = touching(a.rect, p.rect);
        if (edge) { out.set(p.device_id, { anchor: id, edge }); seen.add(p.device_id); next.push(p.device_id); }
      }
    }
    frontier = next;
  }
  return out;
}

/** The saved layout once it must exist: made from attachments first, so nothing already placed moves. */
const ensureLayout = (s: Settings, monitors: Monitor[]): Placement[] => (s.layout?.length ? s.layout : effectiveLayout(s, monitors));

/** `id` beside `edge` of device `anchorId`, or null when that would overlap something. Hotkey order: appended. */
export function placeBeside(s: Settings, monitors: Monitor[], id: string, anchorId: string, edge: Edge): Settings | null {
  if (id === anchorId) return null;
  const layout = ensureLayout(s, monitors).filter((p) => p.device_id !== id);
  const anchor = layout.find((p) => p.device_id === anchorId);
  if (!anchor) return null;
  const mine = { device_id: id, rect: slotRect(anchor.rect, edge, widthOf(s, id)) };
  const at = (s.layout ?? []).findIndex((p) => p.device_id === id);
  const next = at < 0 ? [...layout, mine] : [...layout.slice(0, at), mine, ...layout.slice(at)];
  if (!layoutValid(next, monitors)) return null;
  return { ...s, layout: next, attachments: s.attachments.filter((a) => a.device_id !== id) };
}

/**
 * Keeps a saved layout in step after the attachments changed (placed, moved, swapped or unplaced at a screen edge).
 * Devices beside devices stay; a device that lost its anchor stays where it is. No saved layout: nothing to do.
 * Null when the result would overlap.
 */
export function syncLayout(s: Settings, before: Attachment[], monitors: Monitor[]): Settings | null {
  if (!s.layout?.length) return s;
  const changed = new Set([...before, ...s.attachments].filter((a) => {
    const b = before.find((x) => x.device_id === a.device_id), n = s.attachments.find((x) => x.device_id === a.device_id);
    return !b || !n || b.monitor_id !== n.monitor_id || b.edge !== n.edge;
  }).map((a) => a.device_id));
  let layout = s.layout.filter((p) => !changed.has(p.device_id) || s.attachments.some((a) => a.device_id === p.device_id));
  for (const a of s.attachments) {
    if (!changed.has(a.device_id)) continue;
    const m = monitors.find((x) => x.id === a.monitor_id);
    if (!m) continue;
    const rect = slotRect(m.rect, a.edge, widthOf(s, a.device_id));
    const at = layout.findIndex((p) => p.device_id === a.device_id);
    layout = at < 0 ? [...layout, { device_id: a.device_id, rect }] : layout.map((p, i) => (i === at ? { ...p, rect } : p));
  }
  // A screen-edge move that would overlap a device beside another is refused: the hub says why.
  return layoutValid(layout, monitors) ? { ...s, layout } : null;
}

/** Hotkey order, as the engine's Settings::placed_ids. */
export const placedIds = (s: Settings): string[] => (s.layout?.length ? s.layout.map((p) => p.device_id) : s.attachments.map((a) => a.device_id));
