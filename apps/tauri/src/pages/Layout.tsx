import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { hostName, type Edge, type Monitor, type Settings, type Status } from "@/lib/api";
import { placeDevice } from "@/lib/layout";

const EDGES: Edge[] = ["Left", "Right", "Top", "Bottom"];
const NONE = "none";

export default function Layout({ status, settings, monitors, onChange }: { status: Status | null; settings: Settings; monitors: Monitor[]; onChange: (s: Settings) => void }) {
  // Devices shown: everything connected plus everything already placed.
  const ids = Array.from(new Set([...(status?.hosts.map((h) => h.id) ?? []), ...settings.attachments.map((a) => a.device_id)]));
  const place = (id: string, monitor_id: string, edge: Edge | null) =>
    onChange({ ...settings, attachments: placeDevice(settings.attachments, id, monitor_id === NONE ? null : monitor_id, edge) });
  const monitorItems: Record<string, string> = Object.fromEntries([[NONE, "Not placed"], ...monitors.map((m, i): [string, string] => [m.id, `Screen ${i + 1} (${m.rect.right - m.rect.left}×${m.rect.bottom - m.rect.top})`])]);
  const edgeItems: Record<string, string> = Object.fromEntries(EDGES.map((e) => [e, e]));

  return (
    <div className="flex flex-col gap-4 pt-2">
      <Card>
        <CardHeader>
          <CardTitle>Screens</CardTitle>
          <CardDescription>Put each device on the edge of the screen it sits next to. Hotkeys follow this order: Ctrl+Alt+1 is the first placed device.</CardDescription>
        </CardHeader>
        <CardContent><Arrangement monitors={monitors} settings={settings} status={status} /></CardContent>
      </Card>
      {ids.map((id) => {
        const a = settings.attachments.find((x) => x.device_id === id);
        return (
          <Card key={id}>
            <CardContent className="flex items-center gap-3 pt-6">
              <div className="min-w-0 flex-1 truncate font-medium">{hostName(status, id)}</div>
              <Select items={monitorItems} value={a?.monitor_id ?? NONE} onValueChange={(v) => place(id, String(v), a?.edge ?? "Right")}>
                <SelectTrigger className="w-56"><SelectValue /></SelectTrigger>
                <SelectContent>{Object.entries(monitorItems).map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
              </Select>
              <Select items={edgeItems} value={a?.edge ?? "Right"} disabled={!a} onValueChange={(v) => a && place(id, a.monitor_id, v as Edge)}>
                <SelectTrigger className="w-32"><SelectValue /></SelectTrigger>
                <SelectContent>{EDGES.map((e) => <SelectItem key={e} value={e}>{e}</SelectItem>)}</SelectContent>
              </Select>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}

/** Monitors drawn to scale with placed devices as tabs on their edges. */
function Arrangement({ monitors, settings, status }: { monitors: Monitor[]; settings: Settings; status: Status | null }) {
  if (!monitors.length) return <p className="text-sm text-muted-foreground">No screens found.</p>;
  const minX = Math.min(...monitors.map((m) => m.rect.left)), minY = Math.min(...monitors.map((m) => m.rect.top));
  const maxX = Math.max(...monitors.map((m) => m.rect.right)), maxY = Math.max(...monitors.map((m) => m.rect.bottom));
  const scale = Math.min(560 / (maxX - minX), 180 / (maxY - minY));
  const pad = 70;
  return (
    <div className="relative mx-auto" style={{ width: (maxX - minX) * scale + pad * 2, height: (maxY - minY) * scale + pad * 2 }}>
      {monitors.map((m, i) => {
        const left = (m.rect.left - minX) * scale + pad, top = (m.rect.top - minY) * scale + pad;
        const w = (m.rect.right - m.rect.left) * scale, h = (m.rect.bottom - m.rect.top) * scale;
        return (
          <div key={m.id} className="absolute rounded-md border bg-muted text-xs text-muted-foreground" style={{ left, top, width: w, height: h }}>
            <span className="absolute left-2 top-1">Screen {i + 1}</span>
            {settings.attachments.filter((a) => a.monitor_id === m.id).map((a) => {
              const tab = { Left: { right: w + 6, top: h / 2 - 12 }, Right: { left: w + 6, top: h / 2 - 12 }, Top: { left: w / 2 - 30, bottom: h + 6 }, Bottom: { left: w / 2 - 30, top: h + 6 } }[a.edge];
              const active = status?.active === a.device_id;
              return <span key={a.device_id} className={`absolute whitespace-nowrap rounded px-2 py-0.5 ${active ? "bg-primary text-primary-foreground" : "bg-secondary text-secondary-foreground"}`} style={tab}>{hostName(status, a.device_id)}</span>;
            })}
          </div>
        );
      })}
    </div>
  );
}
