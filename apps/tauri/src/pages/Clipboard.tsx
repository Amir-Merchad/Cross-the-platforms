import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { type HistoryList, historyClear, historyCopy, historyDelete, historyEnable, historyList, historyPin, pairIpad, setClipboardSync } from "@/lib/api";
import { clipNotice, ipadLine, needsRepair } from "@/lib/clip";
import { entryTitle, whenLabel } from "@/lib/history";
import { useHub } from "@/lib/hub";
import { RAISED, cn } from "@/lib/utils";

const CARD = "flex-none rounded-[20px] bg-card";
const ROW = "flex min-h-16 items-center justify-between gap-3 border-t border-border first:border-t-0";

function History({ rev }: { rev: number }) {
  const { say } = useHub();
  const [query, setQuery] = useState("");
  const [list, setList] = useState<HistoryList | null>(null);
  const load = useCallback(() => historyList(query).then(setList).catch(() => say("Could not load the history. Try again.")), [query, say]);
  useEffect(() => { void load(); }, [load, rev]);
  const act = (p: Promise<unknown>, fail: string, done?: string) => p.then(() => { if (done) say(done); return load(); }).catch(() => say(fail));
  const now = Date.now();
  const on = list?.enabled ?? true;
  const ICON = "flex-none rounded-lg px-2.5 py-1.5 text-[13px] font-medium text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring";

  return (
    <>
      <label className={`${CARD} flex items-center justify-between gap-3 px-5 py-[18px]`}>
        <span>
          <span className="block text-xl font-semibold">Clipboard history</span>
          <span className="block text-muted-foreground">Remembers your last 50 copies on this PC only. Anything a password manager marks private is never kept.</span>
        </span>
        <Switch size="lg" checked={on} disabled={!list} onCheckedChange={(next) => act(historyEnable(next), "Could not change the history. Try again.")} />
      </label>
      <section className={`${CARD} px-5 py-3`}>
        <div className="flex items-center gap-3 py-1.5">
          <input
            type="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search your copies" aria-label="Search your copies"
            className="h-10 min-w-0 flex-1 rounded-xl border border-input bg-background px-3 outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
          />
          <Button variant="secondary" className={RAISED} disabled={!list?.entries.length} onClick={() => act(historyClear(), "Could not clear the history. Try again.", "History cleared.")}>Clear history</Button>
        </div>
        {list && list.entries.length === 0 && <p className="py-4 text-muted-foreground">{query ? "Nothing matches that." : on ? "Nothing yet. What you copy shows up here." : "History is off."}</p>}
        {list?.entries.map((e) => (
          <div key={e.id} className={ROW}>
            <button
              type="button" className="min-w-0 flex-1 rounded-lg py-2 text-left focus-visible:outline-2 focus-visible:outline-ring"
              onClick={() => act(historyCopy(e.id), "Could not copy that. Try again.", "Copied.")}
            >
              <span className="block truncate font-medium">{entryTitle(e)}</span>
              <span className="block text-[13px] text-muted-foreground">{e.from} · {whenLabel(e.at, now)}</span>
            </button>
            <button type="button" className={cn(ICON, e.pinned && "text-foreground")} aria-pressed={e.pinned} onClick={() => act(historyPin(e.id, !e.pinned), "Could not pin that. Try again.")}>{e.pinned ? "Pinned" : "Pin"}</button>
            <button type="button" className={ICON} aria-label={`Delete ${entryTitle(e)}`} onClick={() => act(historyDelete(e.id), "Could not delete that. Try again.")}>Delete</button>
          </div>
        ))}
      </section>
    </>
  );
}

export default function Clipboard() {
  const { clip, devices, say } = useHub();
  const on = clip?.enabled ?? false;
  const repair = on && needsRepair(clip?.ipad ?? null);
  const ipad = devices.find((d) => d.kind === "Ipad");
  const others = devices.filter((d) => d.peerId && d.kind !== "Ipad");
  const toggle = (next: boolean) => setClipboardSync(next).catch(() => say("Could not change clipboard sync. Try again."));
  const pair = () => pairIpad().catch(() => say("Could not pair over USB. Plug the iPad in and try again."));
  const notice = clipNotice(clip?.notice ?? null);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto text-[15px] data-[hidden]:hidden">
      <label className={`${CARD} flex items-center justify-between gap-3 px-5 py-[18px]`}>
        <span>
          <span className="block text-xl font-semibold">Sync clipboard</span>
          <span className="block text-muted-foreground">Copy on one device, paste on another. Contents are never logged.</span>
        </span>
        <Switch size="lg" checked={on} disabled={!clip} onCheckedChange={toggle} />
      </label>
      <section className={`${CARD} px-5 py-1.5`}>
        <div className={ROW}>
          <div className="min-w-0"><div className="truncate font-medium">{ipad?.name ?? "iPad"}</div><div className="text-[13px] text-muted-foreground">Text and images, both ways</div></div>
          <span className="flex flex-none items-center gap-3">
            <span className={cn(repair ? "text-destructive" : "text-muted-foreground")}>{on ? ipadLine(clip?.ipad ?? null) : "Sync off"}</span>
            <Button variant={repair ? "default" : "secondary"} className={RAISED} onClick={pair}>Pair over USB</Button>
          </span>
        </div>
        {others.map((d) => (
          <div key={d.key} className={ROW}>
            <div className="min-w-0">
              <div className="truncate font-medium">{d.name}</div>
              <div className="text-[13px] text-muted-foreground">{d.kind === "Android" ? "Text only. From the phone, use Bridge or its Quick Settings tile." : "Text only"}</div>
            </div>
            <span className="flex-none text-muted-foreground">{!on ? "Sync off" : d.reachable ? "Connected" : "Not connected"}</span>
          </div>
        ))}
      </section>
      {notice && <p role="status" className="flex-none px-1 text-muted-foreground">{notice}</p>}
      <section className={`${CARD} flex gap-12 px-5 py-3.5`}>
        <span>Synced since start <strong className="font-semibold tabular-nums">{clip?.synced ?? 0}</strong></span>
        <span>Skipped, too large <strong className="font-semibold tabular-nums">{clip?.skipped_too_large ?? 0}</strong></span>
      </section>
      <History rev={clip?.history_rev ?? 0} />
    </div>
  );
}
