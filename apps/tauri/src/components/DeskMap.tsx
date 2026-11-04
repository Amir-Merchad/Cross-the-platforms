import { useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import DeviceObject from "@/components/DeviceObject";
import { useDeskMotion, type Target } from "@/components/useDeskMotion";
import { switchTo, type Edge } from "@/lib/api";
import { deskPlan, dropCounts, EDGES, edgeBar, edgeOpen, hotkeyNumber, keyboardScreen, moveDevice, placedLine, placementChange, screenName, swapToMissing, type Box } from "@/lib/desk";
import { deviceLook, type Device } from "@/lib/devices";
import { useHub } from "@/lib/hub";
import { progressPercent } from "@/lib/transfer";
import { cn } from "@/lib/utils";

const NONE: Device[] = [];
const KEYS: Record<string, Edge> = { ArrowRight: "Right", ArrowLeft: "Left", ArrowUp: "Top", ArrowDown: "Bottom" };

const along = (box: Box, edge: Edge) => (edge === "Left" || edge === "Right" ? box.h : box.w);
const place = ({ x, y, w, h }: Box) => ({ left: x, top: y, width: w, height: h });

export default function DeskMap({ over }: { over: string | null }) {
  const { devices, monitors, settings, status, rows, btOff, current, update, select, say } = useHub();
  const stage = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [picked, setPicked] = useState<string | null>(null);

  useLayoutEffect(() => {
    // Zero while the tab is hidden: keep the last size so nothing moves.
    const ro = new ResizeObserver(([e]) => { if (e.contentRect.width) setSize({ w: e.contentRect.width, h: e.contentRect.height }); });
    ro.observe(stage.current!);
    return () => ro.disconnect();
  }, []);

  // Until size and monitors are known, every device would look unplaced and fly in from the tray.
  const ready = size.w > 0 && monitors.length > 0;
  const shown = ready ? devices : NONE;
  const plan = useMemo(() => deskPlan(shown, monitors, size), [shown, monitors, size]);
  const active = status?.active ?? null;

  const commit = async (d: Device, target: Target | null) => {
    const id = d.hostId!;
    if (swapToMissing(settings.attachments, id, target, monitors)) {
      say(`Another device sits there, and it cannot swap to the screen ${d.name} was on. Pick a free edge.`);
      return;
    }
    const next = moveDevice(settings.attachments, id, target);
    try {
      if (placementChange(active, next) === null) await switchTo(null);
    } catch {
      say("Could not bring your keyboard and mouse back, so nothing changed. Try again.");
      return;
    }
    update((s) => ({ ...s, attachments: moveDevice(s.attachments, id, target) }));
    select(d.key);
    say(target
      ? placedLine(d.name, screenName(monitors, target.monitorId), target.edge, hotkeyNumber(next, id)!)
      : `${d.name} is not placed.${d.peerId ? " It stays paired for files and clipboard." : ""}`);
  };

  const drop = (d: Device, target: Target | null) => {
    if (dropCounts(d, target)) commit(d, target);
    else say(`${d.name} keeps its place until its screen is back.`);
  };

  const { lifted, near, bind } = useDeskMotion(stage, plan, over, drop);

  const onKeyDown = (e: KeyboardEvent, d: Device) => {
    const edge = KEYS[e.key];
    if (edge) {
      e.preventDefault();
      if (!d.hostId) { say("It needs Bluetooth first."); return; }
      const monitorId = keyboardScreen(picked, d.placement, monitors);
      if (!monitorId) return;
      if (edgeOpen(monitors, monitorId, edge)) commit(d, { monitorId, edge });
      else say(`Another screen sits on that side of ${screenName(monitors, monitorId)}. Pick a different edge.`);
    }
    if (e.key === "Delete" && d.placement) commit(d, null);
  };
  // Enter and Space arrive as a click with no pointer behind it; pointer selection happens on pointer up.
  const onClick = (e: MouseEvent, d: Device) => { if (e.detail === 0) select(d.key); };

  const hint = !lifted && devices.find((d) => d.hostId && !d.placement);
  const liftedDevice = devices.find((d) => d.key === lifted);
  const inControl = devices.find((d) => d.hostId && d.hostId === active);
  const home = inControl && plan.homes.get(inControl.key);
  const first = ready ? plan.screens[0].box : null;
  const pointer = home ? { x: home.x + 60, y: home.y + 44 } : first ? { x: first.x + first.w / 2, y: first.y + first.h / 2 } : null;

  return (
    <div ref={stage} className="relative min-h-0 flex-1 overflow-hidden rounded-[20px] bg-card">
      {hint && <p className="absolute top-3.5 right-5 left-5 truncate font-medium">Pick up {hint.name} and put it next to a screen edge.</p>}
      <div className="absolute rounded-[18px] border-2 border-dashed px-3 py-1.5 text-[13px] text-muted-foreground" style={place(plan.tray)}>Not placed</div>

      {plan.screens.map(({ id, box }) => {
        const many = plan.screens.length > 1;
        const Screen = many ? "button" : "div";
        return (
          <div key={id}>
            <Screen
              {...(many ? { type: "button" as const, "aria-pressed": picked === id, onClick: () => setPicked(id) } : {})}
              className={cn("absolute rounded-[14px] bg-screen px-4 py-3 text-left font-semibold text-[#EEF3F8] shadow-[0_5px_0_#0D141C]", picked === id && many && "outline-3 outline-offset-3 outline-foreground")}
              style={place(box)}
            >
              Windows PC<div className="text-[13px] font-normal opacity-85">{screenName(monitors, id)}</div>
            </Screen>
            <div className="absolute h-3 w-[60px] rounded-b-lg bg-border" style={{ left: box.x + box.w / 2 - 30, top: box.y + box.h + 5 }} />
            {lifted && EDGES.filter((edge) => edgeOpen(monitors, id, edge)).map((edge) => {
              const hot = near?.monitorId === id && near.edge === edge;
              const length = Math.min(edge === "Left" || edge === "Right" ? 140 : 180, along(box, edge));
              return <div key={edge} className="absolute rounded-[5px]" style={{ ...place(edgeBar(box, edge, 8, length, 4)), background: hot ? liftedDevice?.colour : "var(--edge)" }} />;
            })}
          </div>
        );
      })}

      {shown.map((d) => {
        const box = plan.screens.find((s) => s.id === d.placement?.monitor_id)?.box;
        if (!box || !d.placement || d.key === lifted) return null;
        const { off } = deviceLook(d, active, btOff);
        const { edge } = d.placement;
        return (
          <div
            key={d.key}
            className="absolute rounded-[3px]"
            style={{ ...place(edgeBar(box, edge, 6, Math.min(120, along(box, edge)), 5)), background: off ? "var(--off)" : d.colour, boxShadow: d.hostId === active ? `0 0 18px 4px ${d.colour}` : "none" }}
          />
        );
      })}

      {shown.map((d) => {
        const { sub, off } = deviceLook(d, active, btOff);
        const sending = rows.filter((r) => r.peer === d.peerId && r.outgoing && r.state === "moving").at(-1);
        return (
          <DeviceObject
            key={d.key} d={d} off={off} sub={d.key === lifted ? "Placing" : sub} selected={d.key === current?.key} lifted={d.key === lifted}
            hotkey={d.hostId && !d.missingScreen ? hotkeyNumber(settings.attachments, d.hostId) : null}
            ring={sending ? progressPercent(sending.done, sending.total) / 100 : null}
            onKeyDown={(e) => onKeyDown(e, d)} onClick={(e) => onClick(e, d)} {...bind(d)}
          />
        );
      })}

      {pointer && (
        <svg width="26" height="26" viewBox="0 0 24 24" fill="#fff" stroke="#10202E" strokeWidth="1.5" aria-hidden className="pointer-events-none absolute top-0 left-0 z-20 transition-transform duration-[450ms]" style={{ transform: `translate(${pointer.x}px,${pointer.y}px)` }}>
          <path d="M5 2l14 11-6 1 4 7-3 1-4-7-5 4z" />
        </svg>
      )}
    </div>
  );
}
