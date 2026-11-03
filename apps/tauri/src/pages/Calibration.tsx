import { Fragment } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { DEFAULT_CALIBRATION, DEFAULT_WIDTH, hostName, type Calibration as Cal, type Settings, type Status } from "@/lib/api";

const HOTKEYS = [["Ctrl+Alt+1 … 9", "Switch to the n-th placed device"], ["Ctrl+Alt+Q", "Back to Windows"], ["Esc Esc Esc", "Emergency: back to Windows"]];

function Row({ label, value, unit, min, max, step, onChange }: { label: string; value: number; unit: string; min: number; max: number; step: number; onChange: (v: number) => void }) {
  return (
    <div className="grid grid-cols-[10rem_1fr_5rem] items-center gap-4 text-sm">
      <span>{label}</span>
      <Slider min={min} max={max} step={step} value={value} onValueChange={(v) => onChange(Array.isArray(v) ? v[0] : v)} />
      <span className="text-right tabular-nums text-muted-foreground">{value}{unit}</span>
    </div>
  );
}

export default function Calibration({ status, settings, onChange }: { status: Status | null; settings: Settings; onChange: (s: Settings) => void }) {
  const cal = (id: string) => settings.calibration[id] ?? DEFAULT_CALIBRATION;
  const setCal = (id: string, patch: Partial<Cal>) => onChange({ ...settings, calibration: { ...settings.calibration, [id]: { ...cal(id), ...patch } } });
  const setWidth = (id: string, w: number) => onChange({ ...settings, remote_width: { ...settings.remote_width, [id]: w } });
  return (
    <div className="flex flex-col gap-4 pt-2">
      <Card>
        <CardHeader><CardTitle>General</CardTitle></CardHeader>
        <CardContent className="flex flex-col gap-4">
          <Row label="Edge hold to enter" value={settings.entry_hold_ms} unit=" ms" min={100} max={800} step={50} onChange={(v) => onChange({ ...settings, entry_hold_ms: v })} />
          <label className="flex items-center justify-between text-sm">Start with Windows
            <Switch checked={settings.start_with_windows} onCheckedChange={(c) => onChange({ ...settings, start_with_windows: c })} />
          </label>
        </CardContent>
      </Card>
      {settings.attachments.map(({ device_id: id }) => (
        <Card key={id}>
          <CardHeader>
            <CardTitle>{hostName(status, id)}</CardTitle>
            <CardDescription>How hard to push back to return, and how far the pointer travels on the device.</CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4">
            <Row label="Pointer gain" value={cal(id).gain} unit="×" min={0.5} max={2} step={0.05} onChange={(v) => setCal(id, { gain: v })} />
            <Row label="Return push" value={cal(id).min_push} unit="" min={32} max={256} step={8} onChange={(v) => setCal(id, { min_push: v })} />
            <Row label="Return hold" value={cal(id).dwell_ms} unit=" ms" min={100} max={800} step={50} onChange={(v) => setCal(id, { dwell_ms: v })} />
            <Row label="Screen width" value={settings.remote_width[id] ?? DEFAULT_WIDTH} unit=" pt" min={600} max={2000} step={2} onChange={(v) => setWidth(id, v)} />
          </CardContent>
        </Card>
      ))}
      <Card>
        <CardHeader><CardTitle>Hotkeys</CardTitle></CardHeader>
        <CardContent className="grid grid-cols-[10rem_1fr] gap-2 text-sm">
          {HOTKEYS.map(([k, d]) => <Fragment key={k}><kbd className="font-mono">{k}</kbd><span className="text-muted-foreground">{d}</span></Fragment>)}
        </CardContent>
      </Card>
    </div>
  );
}
