import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import type { Status } from "@/lib/api";

export default function Diagnostics({ status }: { status: Status | null }) {
  return (
    <Card className="mt-2">
      <CardHeader>
        <CardTitle>Connection</CardTitle>
        <CardDescription>Timings only. No keystrokes or clipboard contents are ever recorded.</CardDescription>
      </CardHeader>
      <CardContent>
        <table className="w-full text-sm">
          <thead className="text-left text-muted-foreground">
            <tr><th className="py-1 font-normal">Device</th><th className="font-normal">Link</th><th className="font-normal">Report interval</th><th className="font-normal">Send avg / max</th></tr>
          </thead>
          <tbody className="tabular-nums">
            {status?.hosts.map((h) => (
              <tr key={h.id} className="border-t">
                <td className="py-1">{h.name ?? h.id}</td>
                <td>{h.connected ? "connected" : "offline"}</td>
                <td>{h.interval_ms ? `${h.interval_ms} ms` : "—"}</td>
                <td>{status.active === h.id ? `${h.notify_avg_ms.toFixed(1)} / ${h.notify_max_ms.toFixed(1)} ms` : "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {status?.notice && <p className="mt-3 text-sm text-muted-foreground">Last notice: {status.notice}</p>}
      </CardContent>
    </Card>
  );
}
