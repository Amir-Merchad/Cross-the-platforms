import { useEffect, useRef } from "react";
import DeskMap from "@/components/DeskMap";
import DevicePanel from "@/components/DevicePanel";
import { useFileDrop } from "@/components/useFileDrop";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { plainError } from "@/lib/activity";
import { openBluetoothSettings, setClipboardSync } from "@/lib/api";
import { displayName, idleLine } from "@/lib/devices";
import { useHub } from "@/lib/hub";
import { formatBytes, formatRate, progressPercent } from "@/lib/transfer";

function ActivityStrip() {
  const { rows, devices, clip, say } = useHub();
  const row = rows.filter((r) => r.state === "moving").at(-1);
  const toggle = (on: boolean) => setClipboardSync(on).catch(() => say("Could not change clipboard sync. Try again."));
  return (
    <div className="flex h-[92px] flex-none items-center gap-6 rounded-[20px] bg-card px-4 py-3">
      {row ? (
        <div className="min-w-0 flex-1">
          <div className="flex justify-between gap-3 font-medium">
            <span className="truncate">{row.name} {row.outgoing ? "to" : "from"} {displayName(devices.find((d) => d.peerId === row.peer)?.name)}</span>
            <span className="flex-none tabular-nums">{formatRate(row.done, row.now - row.startedAt)}</span>
          </div>
          <div className="mt-1.5 mb-1 h-1.5 rounded-[3px] bg-border"><div className="h-1.5 rounded-[3px] bg-primary" style={{ width: `${progressPercent(row.done, row.total)}%` }} /></div>
          <div className="text-[13px] text-muted-foreground tabular-nums">{formatBytes(row.done)} of {formatBytes(row.total)}</div>
        </div>
      ) : <div className="min-w-0 flex-1 text-muted-foreground">{idleLine(devices)}</div>}
      <label className="flex flex-none items-center gap-2.5 text-[13px] text-muted-foreground">
        Clipboard sync
        <Switch size="lg" checked={clip?.enabled ?? false} disabled={!clip} onCheckedChange={toggle} />
      </label>
    </div>
  );
}

export default function Desk() {
  const { rows, btOff, say } = useHub();
  const { over, chip, chipEl } = useFileDrop();

  // A send fails after the drop, so the status line says so when its event lands.
  const told = useRef(new Set<number>());
  useEffect(() => {
    for (const r of rows) {
      if (r.state !== "failed" || told.current.has(r.id)) continue;
      told.current.add(r.id);
      // A failed send was not queued: a send that can wait arrives as Queued instead.
      say(plainError(r.reason ?? "", false));
    }
  }, [rows, say]);

  return (
    <>
      {btOff && (
        <div className="flex flex-none items-center justify-between gap-3 rounded-xl bg-warn px-3.5 py-2.5 font-medium text-warn-foreground">
          <span>Bluetooth is off, so keyboard and mouse sharing is paused. Files and clipboard still work.</span>
          <Button
            className="h-8 flex-none rounded-[9px] px-3 font-semibold shadow-none"
            onClick={() => openBluetoothSettings().catch(() => say("Could not open Bluetooth settings. Open them from the Windows Settings app."))}
          >Open Bluetooth settings</Button>
        </div>
      )}
      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_300px] gap-3">
        <div className="flex min-w-0 flex-col gap-3">
          <DeskMap over={over} />
          <ActivityStrip />
        </div>
        <DevicePanel />
      </div>
      {chip && (
        <div
          ref={chipEl}
          className="pointer-events-none fixed z-50 flex h-[42px] max-w-60 items-center rounded-xl border-2 bg-background px-3 font-medium shadow-[0_8px_16px_rgba(0,0,0,.28)]"
          style={{ left: chip.x + 14, top: chip.y + 14 }}
        ><span className="truncate">{chip.text}</span></div>
      )}
    </>
  );
}
