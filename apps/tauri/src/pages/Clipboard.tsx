import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { pairIpad, setClipboardSync } from "@/lib/api";
import { clipNotice, ipadLine, needsRepair } from "@/lib/clip";
import { useHub } from "@/lib/hub";
import { RAISED, cn } from "@/lib/utils";

const CARD = "flex-none rounded-[20px] bg-card";
const ROW = "flex min-h-16 items-center justify-between gap-3 border-t border-border first:border-t-0";

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
          <div><div className="font-medium">{ipad?.name ?? "iPad"}</div><div className="text-[13px] text-muted-foreground">Text and images, both ways</div></div>
          <span className="flex items-center gap-3">
            <span className={cn(repair ? "text-destructive" : "text-muted-foreground")}>{on ? ipadLine(clip?.ipad ?? null) : "Sync off"}</span>
            <Button variant={repair ? "default" : "secondary"} className={RAISED} onClick={pair}>Pair over USB</Button>
          </span>
        </div>
        {others.map((d) => (
          <div key={d.key} className={ROW}>
            <div>
              <div className="font-medium">{d.name}</div>
              <div className="text-[13px] text-muted-foreground">{d.kind === "Android" ? "Text only. From the phone, use Bridge or its Quick Settings tile." : "Text only"}</div>
            </div>
            <span className="text-muted-foreground">{!on ? "Sync off" : d.reachable ? "Connected" : "Not connected"}</span>
          </div>
        ))}
      </section>
      {notice && <p role="status" className="flex-none px-1 text-muted-foreground">{notice}</p>}
      <section className={`${CARD} flex gap-12 px-5 py-3.5`}>
        <span>Synced since start <strong className="font-semibold tabular-nums">{clip?.synced ?? 0}</strong></span>
        <span>Skipped, too large <strong className="font-semibold tabular-nums">{clip?.skipped_too_large ?? 0}</strong></span>
      </section>
    </div>
  );
}
