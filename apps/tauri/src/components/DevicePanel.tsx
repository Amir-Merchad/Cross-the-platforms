import { useId, useState } from "react";
import HoldButton from "@/components/HoldButton";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { DEFAULT_CALIBRATION, DEFAULT_WIDTH, switchTo, type Calibration } from "@/lib/api";
import { hotkeyNumber, placementChange, screenName, whereLine } from "@/lib/desk";
import { DEFAULT_COLOURS, forgetDevice, panelLines, type Device } from "@/lib/devices";
import { useHub } from "@/lib/hub";
import { forgetPeer } from "@/lib/transfer";
import { cn } from "@/lib/utils";

const COLOUR_NAMES = ["Orange", "Green", "Blue", "Pink", "Yellow"];

type TuneProps = { label: string; value: number; show: (v: number) => string; min: number; max: number; step: number; onCommit: (v: number) => void };

/** Moves freely while dragged and saves on release. */
export function Tune({ label, value, show, min, max, step, onCommit }: TuneProps) {
  const id = useId();
  const [draft, setDraft] = useState<number | null>(null);
  const one = (v: number | readonly number[]) => (typeof v === "number" ? v : v[0]);
  const shown = draft ?? value;
  return (
    <div>
      <div className="mb-2.5 flex justify-between gap-3"><span id={id}>{label}</span><span className="text-[13px] text-muted-foreground tabular-nums">{show(shown)}</span></div>
      <Slider
        aria-labelledby={id} min={min} max={max} step={step} value={shown}
        onValueChange={(v) => setDraft(one(v))}
        onValueCommitted={(v) => { setDraft(null); onCommit(one(v)); }}
      />
    </div>
  );
}

function Row({ name, value }: { name: string; value: string }) {
  return <div className="flex justify-between gap-3"><span className="flex-none">{name}</span><span className="text-right text-[13px] text-muted-foreground">{value}</span></div>;
}

function Panel({ d }: { d: Device }) {
  const { settings, update, status, clip, monitors, btOff, say, select, refreshPeers } = useHub();
  const active = status?.active ?? null;
  const lines = panelLines(d, {
    active, btOff, clip,
    where: d.placement ? whereLine(d.placement.edge, screenName(monitors, d.placement.monitor_id)) : "",
    hotkey: d.hostId ? hotkeyNumber(settings.attachments, d.hostId) : null,
  });
  const inControl = !!d.hostId && d.hostId === active;
  const toggle = () => switchTo(inControl ? null : d.hostId).catch(() => say("Could not switch. Try again."));

  const forget = async () => {
    try {
      if (placementChange(active, forgetDevice(settings, d).attachments) === null) await switchTo(null);
      if (d.peerId) { await forgetPeer(d.peerId); await refreshPeers(); }
    } catch {
      say("That did not finish. Hold to forget again.");
      return;
    }
    update((s) => forgetDevice(s, d));
    select(null);
    say("Remove it from Bluetooth on the device too.");
  };

  const host = d.hostId;
  const cal = (host && settings.calibration[host]) || DEFAULT_CALIBRATION;
  const setCal = (patch: Partial<Calibration>) =>
    update((s) => ({ ...s, calibration: { ...s.calibration, [host!]: { ...(s.calibration[host!] ?? DEFAULT_CALIBRATION), ...patch } } }));

  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <strong className="min-w-0 truncate text-xl font-semibold">{d.name}</strong>
        <span className="flex-none text-[13px] text-muted-foreground">{lines.key}</span>
      </div>
      <Button className="h-11 w-full flex-none rounded-xl font-semibold" disabled={!lines.canSwitch} onClick={toggle}><span className="truncate">{lines.button}</span></Button>
      {lines.why && <p className="-mt-1.5 text-[13px] text-muted-foreground">{lines.why}</p>}
      <div className="flex flex-col gap-2">
        <Row name="Keyboard and mouse" value={lines.hid} />
        <Row name="Files" value={lines.files} />
        <Row name="Clipboard" value={lines.clip} />
      </div>
      {host && (
        <>
          <Tune label="Pointer speed" value={cal.gain} show={(v) => `${v.toFixed(2)}×`} min={0.5} max={2} step={0.05} onCommit={(gain) => setCal({ gain })} />
          <details>
            <summary className="cursor-pointer text-muted-foreground">Fine tuning</summary>
            <div className="mt-3 flex flex-col gap-3">
              <Tune label="How hard to push back" value={cal.min_push} show={String} min={32} max={256} step={8} onCommit={(min_push) => setCal({ min_push })} />
              <Tune label="How long to hold at the edge" value={cal.dwell_ms} show={(v) => `${v} ms`} min={100} max={800} step={50} onCommit={(dwell_ms) => setCal({ dwell_ms })} />
              <Tune
                label="Device screen width" value={settings.remote_width[host] ?? DEFAULT_WIDTH} show={(v) => `${v} pt`} min={600} max={2000} step={2}
                onCommit={(w) => update((s) => ({ ...s, remote_width: { ...s.remote_width, [host]: w } }))}
              />
            </div>
          </details>
        </>
      )}
      <div>
        <div className="mb-1.5 text-[13px] text-muted-foreground">Colour</div>
        <div className="flex gap-2">
          {DEFAULT_COLOURS.map((c, i) => (
            <button
              key={c} type="button" aria-label={COLOUR_NAMES[i]} aria-pressed={d.colour === c}
              className={cn("size-7 rounded-full", d.colour === c && "outline-3 outline-offset-2 outline-foreground")}
              style={{ background: c }}
              onClick={() => update((s) => ({ ...s, device_colors: { ...s.device_colors, [d.key]: c } }))}
            />
          ))}
        </div>
      </div>
      <HoldButton onDone={forget} holding="Keep holding to forget">Hold to forget this device</HoldButton>
    </>
  );
}

export default function DevicePanel() {
  const { current, setTab } = useHub();
  return (
    <aside aria-label="Selected device" className="flex min-w-0 flex-col gap-3.5 overflow-y-auto rounded-[20px] bg-card p-4">
      {current ? <Panel key={current.key} d={current} /> : (
        <>
          <strong className="text-xl font-semibold">Add your first device</strong>
          <p className="text-muted-foreground">Bridge shares this PC's keyboard, mouse, clipboard and files with your phone or iPad. Nothing leaves your devices.</p>
          {/* Pairing lives on Transfers until the Add a device flow lands. */}
          <Button className="h-11 rounded-xl font-semibold" onClick={() => setTab("transfers")}>Add a device</Button>
          <p className="mt-auto text-[13px] text-muted-foreground">Closing this window keeps Bridge running in the tray.</p>
        </>
      )}
    </aside>
  );
}
