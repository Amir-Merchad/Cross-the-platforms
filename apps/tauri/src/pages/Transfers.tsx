import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { canSendAgain, historyLine, waitingLine } from "@/lib/activity";
import { displayName } from "@/lib/devices";
import { useHub } from "@/lib/hub";
import { formatBytes, formatRate, openReceiveDir, progressPercent, receiveDir, sendFiles, setReceiveDir } from "@/lib/transfer";
import { RAISED } from "@/lib/utils";
import Pairing from "@/pages/Transfer";

const CARD = "flex-none rounded-[20px] bg-card px-4 py-3.5";

function ReceiveFolder() {
  const { say } = useHub();
  const [dir, setDir] = useState("");
  useEffect(() => { receiveDir().then(setDir, () => {}); }, []);
  const change = async () => {
    try {
      const d = await open({ directory: true });
      if (typeof d === "string") { await setReceiveDir(d); setDir(d); }
    } catch { say("Could not change the folder. Try again."); }
  };
  return (
    <section className={`${CARD} flex items-center justify-between gap-3`}>
      <span className="min-w-0">Received files go to <span className="break-all text-muted-foreground">{dir}</span></span>
      <span className="flex flex-none gap-2">
        <Button variant="secondary" className={RAISED} onClick={() => openReceiveDir().catch(() => say("Could not open the folder. Try again."))}>Open folder</Button>
        <Button variant="secondary" className={RAISED} onClick={change}>Change</Button>
      </span>
    </section>
  );
}

export default function Transfers() {
  const { rows, devices, say } = useHub();
  const byPeer = (peer: string) => devices.find((d) => d.peerId === peer);
  const moving = rows.filter((r) => r.state === "moving");
  const history = rows.filter((r) => r.state !== "moving").reverse();
  const waiting = devices.filter((d) => d.waiting > 0);
  const again = (peer: string, paths: string[]) => sendFiles(peer, paths).catch(() => say("That did not finish. Send it again."));

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto text-[15px] data-[hidden]:hidden">
      {rows.length === 0 && waiting.length === 0 ? (
        <div className="flex min-h-40 flex-1 flex-col items-center justify-center gap-1.5 rounded-[20px] bg-card">
          <strong className="text-xl font-semibold">Nothing sent yet</strong>
          <span className="text-muted-foreground">Drop files on a device on the Desk, or right-click in Explorer and choose Send to.</span>
        </div>
      ) : (
        <>
          {moving.length > 0 && (
            <section className={CARD}>
              <div className="mb-2 font-semibold">Moving now</div>
              {moving.map((r) => (
                <div key={r.id} className="mb-2 last:mb-0">
                  <div className="flex justify-between gap-3 font-medium">
                    <span className="truncate">{r.name} {r.outgoing ? "to" : "from"} {displayName(byPeer(r.peer)?.name)}</span>
                    <span className="flex-none tabular-nums">{formatRate(r.done, r.now - r.startedAt)}</span>
                  </div>
                  <div className="mt-1.5 mb-1 h-1.5 rounded-[3px] bg-border"><div className="h-1.5 rounded-[3px] bg-primary" style={{ width: `${progressPercent(r.done, r.total)}%` }} /></div>
                  <div className="text-[13px] text-muted-foreground tabular-nums">{formatBytes(r.done)} of {formatBytes(r.total)}</div>
                </div>
              ))}
            </section>
          )}
          {waiting.map((d) => {
            const [what, when] = waitingLine(d.waiting, d.name);
            return (
              <section key={d.key} className={`${CARD} flex items-center justify-between gap-3`}>
                <span className="font-semibold">{what}</span><span className="text-muted-foreground">{when}</span>
              </section>
            );
          })}
          {history.length > 0 && (
            <section className={CARD}>
              <div className="mb-1.5 font-semibold">History</div>
              {history.map((r) => {
                const peer = byPeer(r.peer);
                const h = historyLine(r, peer?.name, peer?.kind === "Ipad");
                return (
                  <div key={r.id} className="grid min-h-[42px] grid-cols-[2fr_1.4fr_90px_1.6fr] items-center gap-3 border-t border-border">
                    <span className="truncate">{h.file}</span>
                    <span className="truncate text-muted-foreground">{h.who}</span>
                    <span className="text-muted-foreground tabular-nums">{h.size}</span>
                    <span className="flex items-center justify-between gap-2">
                      <span className={h.failed ? "text-destructive" : "text-success"}>{h.result}</span>
                      {canSendAgain(r, peer) && (
                        <Button variant="secondary" className={RAISED} onClick={() => again(r.peer, r.paths)}>Send again</Button>
                      )}
                    </span>
                  </div>
                );
              })}
            </section>
          )}
        </>
      )}
      <ReceiveFolder />
      <Pairing />
    </div>
  );
}
