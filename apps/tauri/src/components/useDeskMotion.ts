// Springs and pointer dragging for the map. Positions go straight to the DOM, so React never renders per frame.
import { useEffect, useLayoutEffect, useRef, useState, type PointerEvent, type RefObject } from "react";
import type { Edge } from "@/lib/api";
import { deviceSize, magnet, nearestEdge, screenName, slotFor, whereLine, type deskPlan } from "@/lib/desk";
import type { Device } from "@/lib/devices";
import { useHub } from "@/lib/hub";
import { step } from "@/lib/spring";

export type Target = { monitorId: string; edge: Edge };
type Axis = { x: number; v: number; t: number };
type Body = { x: Axis; y: Axis; s: Axis };
type Drag = { key: string; ox: number; oy: number; sx: number; sy: number; moved: boolean; near: Target | null };

const LIFT = 1.07, FILE_OVER = 1.12, PICK_UP_PX = 5;
const FOLLOW = [0.4, 0.55] as const; // stiffer, so a held device keeps up with the pointer
const REST = 0.01;

export const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const axis = (x: number): Axis => ({ x, v: 0, t: x });

/** One spring frame. True while the axis is still moving. */
function advance(a: Axis, k?: number, f?: number): boolean {
  const n = step(a, a.t, k, f);
  a.x = n.x; a.v = n.v;
  if (Math.abs(a.t - a.x) >= REST || Math.abs(a.v) >= REST) return true;
  a.x = a.t; a.v = 0;
  return false;
}

/** Every body on the map, moved by one requestAnimationFrame loop that sleeps when all are at rest. */
function makeSprings() {
  const bodies = new Map<string, Body>();
  const els = new Map<string, HTMLElement>();
  const m = { bodies, els, held: null as string | null, raf: 0, paint, wake, hold: (key: string | null) => { m.held = key; }, stop: () => { cancelAnimationFrame(m.raf); m.raf = 0; } };

  function paint(key: string, b: Body) {
    const el = els.get(key);
    if (el) el.style.transform = `translate(${b.x.x}px,${b.y.x}px) scale(${b.s.x})`;
  }
  function frame() {
    m.raf = 0;
    const instant = reducedMotion();
    let moving = false;
    for (const [key, b] of bodies) {
      if (instant) for (const a of [b.x, b.y, b.s]) { a.x = a.t; a.v = 0; }
      else {
        const [k, f] = key === m.held ? FOLLOW : [];
        moving = [advance(b.x, k, f), advance(b.y, k, f), advance(b.s)].some(Boolean) || moving;
      }
      paint(key, b);
    }
    if (moving) wake();
  }
  function wake() { m.raf ||= requestAnimationFrame(frame); }
  return m;
}

export function useDeskMotion(stage: RefObject<HTMLDivElement | null>, plan: ReturnType<typeof deskPlan>, over: string | null, onDrop: (d: Device, target: Target | null) => void) {
  const { select, say, monitors } = useHub();
  const [lifted, setLifted] = useState<string | null>(null);
  const [near, setNear] = useState<Target | null>(null);
  const [springs] = useState(makeSprings);
  const drag = useRef<Drag | null>(null);

  useEffect(() => springs.stop, [springs]);

  useLayoutEffect(() => {
    for (const key of springs.bodies.keys()) if (!plan.homes.has(key)) springs.bodies.delete(key);
    if (drag.current && !plan.homes.has(drag.current.key)) {
      drag.current = null; springs.hold(null); setLifted(null); setNear(null);
    }
    for (const [key, home] of plan.homes) {
      let b = springs.bodies.get(key);
      if (!b) { b = { x: axis(home.x), y: axis(home.y), s: axis(1) }; springs.bodies.set(key, b); springs.paint(key, b); }
      if (key !== springs.held) { b.x.t = home.x; b.y.t = home.y; }
      b.s.t = key === lifted ? LIFT : key === over ? FILE_OVER : 1;
    }
    springs.wake();
  }, [springs, plan, lifted, over]);

  const end = () => { drag.current = null; springs.hold(null); setLifted(null); setNear(null); };

  const at = (e: PointerEvent) => {
    const r = stage.current!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  };

  const move = (e: PointerEvent, d: Device) => {
    const g = drag.current;
    if (!g) return;
    const p = at(e);
    if (!g.moved) {
      if (Math.hypot(p.x - g.sx, p.y - g.sy) < PICK_UP_PX) return;
      select(d.key);
      if (!d.hostId) { drag.current = null; say("It needs Bluetooth first."); return; }
      g.moved = true;
      springs.hold(d.key);
      setLifted(d.key);
    }
    const size = deviceSize(d.kind);
    const pos = { x: p.x - g.ox, y: p.y - g.oy };
    const edge = nearestEdge({ x: pos.x + size.w / 2, y: pos.y + size.h / 2 }, size, plan.screens);
    const screen = edge && plan.screens.find((s) => s.id === edge.monitorId);
    const aim = edge && screen ? magnet(pos, slotFor(screen.box, edge.edge, size)) : pos;
    const b = springs.bodies.get(d.key)!;
    b.x.t = aim.x; b.y.t = aim.y;
    springs.wake();
    if (edge?.monitorId !== g.near?.monitorId || edge?.edge !== g.near?.edge) {
      g.near = edge;
      setNear(edge);
      say(edge ? `Release to place ${d.name} ${whereLine(edge.edge, screenName(monitors, edge.monitorId))}.` : "");
    }
  };

  /** Props for a device's button. */
  const bind = (d: Device) => ({
    ref: (el: HTMLElement | null) => { if (el) springs.els.set(d.key, el); else springs.els.delete(d.key); },
    onPointerDown: (e: PointerEvent<HTMLButtonElement>) => {
      const b = springs.bodies.get(d.key);
      if (e.button !== 0 || !b) return;
      e.currentTarget.setPointerCapture(e.pointerId);
      const p = at(e);
      drag.current = { key: d.key, ox: p.x - b.x.x, oy: p.y - b.y.x, sx: p.x, sy: p.y, moved: false, near: null };
    },
    onPointerMove: (e: PointerEvent) => move(e, d),
    onPointerUp: () => {
      const g = drag.current;
      if (!g) return;
      end();
      if (g.moved) onDrop(d, g.near); else select(d.key);
    },
    onPointerCancel: () => { if (drag.current?.moved) say(""); end(); },
  });

  return { lifted, near, bind };
}
