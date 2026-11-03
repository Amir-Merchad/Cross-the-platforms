import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { switchTo, type Settings, type Status } from "@/lib/api";

export default function Devices({ status, settings }: { status: Status | null; settings: Settings }) {
  const placed = (id: string) => settings.attachments.some((a) => a.device_id === id);
  return (
    <div className="flex flex-col gap-4 pt-2">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            Bluetooth {status?.advertising ? <Badge>Advertising</Badge> : <Badge variant="secondary">Starting…</Badge>}
          </CardTitle>
          <CardDescription>
            To add a device, open its Bluetooth settings and pick this PC's name. It shows up here once paired, then place it on the Layout tab.
          </CardDescription>
        </CardHeader>
        {status?.notice && <CardContent className="text-sm text-muted-foreground">{status.notice}</CardContent>}
      </Card>
      {status?.hosts.length ? status.hosts.map((h) => (
        <Card key={h.id}>
          <CardContent className="flex items-center justify-between gap-4 pt-6">
            <div className="min-w-0">
              <div className="flex items-center gap-2 font-medium">
                {h.name ?? "Unnamed device"}
                {h.connected ? <Badge>Connected</Badge> : <Badge variant="secondary">Offline</Badge>}
                {status.active === h.id && <Badge variant="outline">In control</Badge>}
              </div>
              <div className="truncate text-xs text-muted-foreground">{h.id}</div>
            </div>
            {status.active === h.id
              ? <Button variant="outline" onClick={() => switchTo(null)}>Back to Windows</Button>
              : <Button disabled={!h.connected || !placed(h.id)} title={placed(h.id) ? undefined : "Place it on the Layout tab first"} onClick={() => switchTo(h.id)}>Switch now</Button>}
          </CardContent>
        </Card>
      )) : <p className="text-sm text-muted-foreground">No devices yet.</p>}
    </div>
  );
}
