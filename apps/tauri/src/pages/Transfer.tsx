import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import QRCode from "qrcode";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  forgetPeer, formatBytes, formatRate, onTransfer, openReceiveDir, pairInvite, pairedPeers, progressPercent, queuedLine, receiveDir,
  sendFiles, setReceiveDir, type Peer,
} from "@/lib/transfer";

interface Live { done: number; total: number; started: number; now: number; failed?: string }
interface Got { name: string; size: number }

const fileName = (path: string) => path.split(/[\\/]/).pop() ?? path;

export default function Transfer() {
  const [peers, setPeers] = useState<Peer[]>([]);
  const [qr, setQr] = useState<{ img: string; until: number } | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [live, setLive] = useState<Record<number, Live>>({});
  const [got, setGot] = useState<Got[]>([]);
  const [waiting, setWaiting] = useState<Record<string, number>>({});
  const [dir, setDir] = useState("");
  const [over, setOver] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const fail = (e: unknown) => setError(String(e));

  const refresh = () => pairedPeers().then(setPeers, fail);

  useEffect(() => {
    refresh();
    receiveDir().then(setDir, fail);
    const off = onTransfer((e) => {
      if (e.type === "Paired") { setQr(null); refresh(); }
      if (e.type === "Progress") setLive((l) => ({ ...l, [e.id]: { done: e.done, total: e.total, started: l[e.id]?.started ?? Date.now(), now: Date.now() } }));
      if (e.type === "Queued") setWaiting((w) => ({ ...w, [e.peer]: (w[e.peer] ?? 0) + e.files }));
      // Failed carries no peer, so it clears every notice.
      if (e.type === "Failed") setWaiting({});
      if (e.type === "Received") setWaiting(({ [e.from]: _, ...rest }) => rest);
      if (e.type === "Failed") setLive((l) => ({ ...l, [e.id]: { ...(l[e.id] ?? { done: 0, total: 0, started: Date.now(), now: Date.now() }), failed: e.reason } }));
      if (e.type === "Received") setGot((g) => [{ name: fileName(e.path), size: e.size }, ...g].slice(0, 20));
    });
    // Files dropped on a device card go to that device.
    const offDrop = getCurrentWebview().onDragDropEvent((ev) => {
      const p = ev.payload;
      const peerAt = (x: number, y: number) => {
        const el = document.elementFromPoint(x / window.devicePixelRatio, y / window.devicePixelRatio)?.closest("[data-peer]");
        return el?.getAttribute("data-peer") ?? null;
      };
      if (p.type === "over") setOver(peerAt(p.position.x, p.position.y));
      else if (p.type === "drop") {
        const id = peerAt(p.position.x, p.position.y);
        setOver(null);
        if (id && p.paths.length) sendFiles(id, p.paths).catch(fail);
      } else setOver(null);
    });
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => { off.then((f) => f()); offDrop.then((f) => f()); clearInterval(t); };
  }, []);

  const showQr = async () => {
    try {
      const inv = await pairInvite();
      setQr({ img: await QRCode.toDataURL(inv.qr, { margin: 4, width: 576, errorCorrectionLevel: "L" }), until: Date.now() + inv.expires_in_s * 1000 });
      setError(null);
    } catch (e) { fail(e); }
  };
  const pick = async (id: string) => {
    const paths = await open({ multiple: true, directory: false });
    if (paths?.length) sendFiles(id, paths).catch(fail);
  };
  const changeDir = async () => {
    const d = await open({ directory: true });
    if (typeof d === "string") setReceiveDir(d).then(() => setDir(d), fail);
  };
  const left = qr ? Math.max(0, Math.round((qr.until - now) / 1000)) : 0;

  return (
    <div className="flex flex-col gap-4 pt-2">
      <Card>
        <CardHeader>
          <CardTitle>Paired devices</CardTitle>
          <CardDescription>Drop files on a device, or use "Send to" in Explorer. Files arrive in its Bridge folder.</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          {peers.length === 0 && <p className="text-sm text-muted-foreground">No devices yet. Pair your phone below.</p>}
          {peers.map((p) => (
            <div key={p.id} data-peer={p.id}
              className={`flex items-center justify-between rounded-lg border p-3 transition-colors ${over === p.id ? "border-primary bg-primary/5" : "border-dashed"}`}>
              <div className="flex items-center gap-2 text-sm">
                <span className="font-medium">{p.name}</span>
                <Badge variant="secondary">{p.kind === "Ipad" ? "iPad" : p.kind}</Badge>
              </div>
              {waiting[p.id] > 0 && <span className="text-sm text-muted-foreground">{queuedLine(p.name, waiting[p.id])}</span>}
              <div className="flex gap-2">
                <Button size="sm" onClick={() => pick(p.id)}>Send files…</Button>
                <Button size="sm" variant="ghost" onClick={() => forgetPeer(p.id).then(refresh, fail)}>Forget</Button>
              </div>
            </div>
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Pair a phone</CardTitle>
          <CardDescription>Open Bridge on the phone and scan this code. It works once and for 5 minutes.</CardDescription>
        </CardHeader>
        <CardContent className="flex items-center gap-6">
          {qr && left > 0 ? (
            <>
              <img src={qr.img} alt="Pairing code" className="size-72 rounded-md bg-white [image-rendering:pixelated]" />
              <p className="text-sm tabular-nums text-muted-foreground">Expires in {Math.floor(left / 60)}:{String(left % 60).padStart(2, "0")}</p>
            </>
          ) : (
            <Button variant="outline" onClick={showQr}>{qr ? "Code expired — show a new one" : "Show pairing code"}</Button>
          )}
        </CardContent>
      </Card>

      {Object.keys(live).length > 0 && (
        <Card>
          <CardHeader><CardTitle>Transfers</CardTitle></CardHeader>
          <CardContent className="flex flex-col gap-3">
            {Object.entries(live).reverse().slice(0, 5).map(([id, l]) => (
              <div key={id} className="flex flex-col gap-1 text-sm">
                <div className="h-2 overflow-hidden rounded-full bg-muted">
                  <div className={`h-full ${l.failed ? "bg-destructive" : "bg-primary"}`} style={{ width: `${progressPercent(l.done, l.total)}%` }} />
                </div>
                <span className="tabular-nums text-muted-foreground">
                  {l.failed ? `Failed: ${l.failed}` : `${formatBytes(l.done)} of ${formatBytes(l.total)} · ${formatRate(l.done, l.now - l.started)}`}
                </span>
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Received</CardTitle>
          <CardDescription className="break-all">{dir}</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-3">
          <div className="flex gap-2">
            <Button size="sm" variant="outline" onClick={() => openReceiveDir().catch(fail)}>Open folder</Button>
            <Button size="sm" variant="ghost" onClick={changeDir}>Change…</Button>
          </div>
          {got.map((g, i) => (
            <div key={i} className="flex justify-between text-sm"><span className="truncate">{g.name}</span><span className="tabular-nums text-muted-foreground">{formatBytes(g.size)}</span></div>
          ))}
        </CardContent>
      </Card>
      {error && <Alert variant="destructive"><AlertDescription>{error}</AlertDescription></Alert>}
    </div>
  );
}
