import { Fragment } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import type { Settings } from "@/lib/api";

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

export default function Calibration({ settings, onChange }: { settings: Settings; onChange: (s: Settings) => void }) {
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
      <Card>
        <CardHeader><CardTitle>Hotkeys</CardTitle></CardHeader>
        <CardContent className="grid grid-cols-[10rem_1fr] gap-2 text-sm">
          {HOTKEYS.map(([k, d]) => <Fragment key={k}><kbd className="font-mono">{k}</kbd><span className="text-muted-foreground">{d}</span></Fragment>)}
        </CardContent>
      </Card>
    </div>
  );
}
