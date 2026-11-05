import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Radio } from "@base-ui/react/radio";
import { RadioGroup } from "@base-ui/react/radio-group";
import { Tune } from "@/components/DevicePanel";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import type { Settings as Prefs } from "@/lib/api";
import { displayName, hiddenDevices, showHost } from "@/lib/devices";
import { useHub } from "@/lib/hub";

const CARD = "flex-none rounded-[20px] bg-card px-5";
const LINE = "flex min-h-14 items-center justify-between gap-3 border-t border-border first:border-t-0";
const HOTKEYS = [["Jump to a device", "Ctrl+Alt+1 to 9"], ["Back to Windows", "Ctrl+Alt+Q"], ["Paste the clipboard on the device in control", "Ctrl+Alt+V"], ["Send Explorer's selected files to it", "Ctrl+Alt+F"], ["Copy a screenshot of the iPad", "Ctrl+Alt+S"], ["Emergency, always returns", "Esc Esc Esc"]];
const LOOKS: [Prefs["appearance"], string][] = [["system", "System"], ["light", "Light"], ["dark", "Dark"]];
const DIAG = "grid grid-cols-[2fr_1fr_1fr_1fr] items-center gap-3";

export default function Settings() {
  const { settings, update, status } = useHub();
  const [version, setVersion] = useState("");
  useEffect(() => { getVersion().then(setVersion, () => {}); }, []);

  return (
    <div className="grid min-h-0 flex-1 grid-cols-2 content-start items-start gap-3 overflow-auto text-[15px] data-[hidden]:hidden">
      <section className={`${CARD} py-1.5`}>
        <label className={LINE}>
          Start with Windows
          <Switch size="lg" checked={settings.start_with_windows} onCheckedChange={(on) => update((s) => ({ ...s, start_with_windows: on }))} />
        </label>
        <div className={LINE}>
          <span id="appearance-label">Appearance</span>
          <RadioGroup
            aria-labelledby="appearance-label" value={settings.appearance} className="flex gap-1 rounded-xl bg-background p-1"
            onValueChange={(v) => update((s) => ({ ...s, appearance: v as Prefs["appearance"] }))}
          >
            {LOOKS.map(([id, label]) => (
              <Radio.Root
                key={id} value={id}
                className="cursor-default rounded-[9px] px-3 py-1 outline-none focus-visible:ring-3 focus-visible:ring-ring/50 data-checked:bg-card data-checked:font-medium"
              >{label}</Radio.Root>
            ))}
          </RadioGroup>
        </div>
        <label className={LINE}>
          <span className="flex flex-col">
            Bluetooth
            <span className="text-[13px] text-muted-foreground">For phones. The iPad works over Wi-Fi without it.</span>
          </span>
          <Switch size="lg" checked={settings.bluetooth_enabled} onCheckedChange={(on) => update((s) => ({ ...s, bluetooth_enabled: on }))} />
        </label>
        <div className="border-t border-border py-3.5">
          <Tune
            label="Hold at the edge before crossing" value={settings.entry_hold_ms} show={(v) => `${v} ms`} min={100} max={800} step={50}
            onCommit={(entry_hold_ms) => update((s) => ({ ...s, entry_hold_ms }))}
          />
        </div>
      </section>
      <section className={`${CARD} py-3.5`}>
        <div className="mb-1.5 font-semibold">Hotkeys</div>
        {HOTKEYS.map(([what, keys]) => (
          <div key={what} className="flex min-h-11 items-center justify-between gap-3 border-t border-border"><span>{what}</span><span className="text-muted-foreground">{keys}</span></div>
        ))}
      </section>
      {hiddenDevices(settings).length > 0 && (
        <section className={`${CARD} col-span-full py-3.5`}>
          <div className="font-semibold">Hidden devices</div>
          <div className="mb-1.5 text-[13px] text-muted-foreground">Bridge hid these because they looked like the iPad on Wi-Fi.</div>
          {hiddenDevices(settings).map((d) => (
            <div key={d.id} className="flex min-h-11 items-center justify-between gap-3 border-t border-border">
              <span className="truncate">{d.name}</span>
              <Button variant="secondary" aria-label={`Show ${d.name}`} onClick={() => update((s) => showHost(s, d.id))}>Show</Button>
            </div>
          ))}
        </section>
      )}
      <section className={`${CARD} col-span-full py-3.5`}>
        <div className="font-semibold">Diagnostics</div>
        <div className="mb-1.5 text-[13px] text-muted-foreground">Timings only, never keystrokes or contents.</div>
        <div className={`${DIAG} min-h-9 text-[13px] text-muted-foreground`}><span>Device</span><span>Link</span><span>Report interval</span><span>Send avg / max</span></div>
        {status?.hosts.map((h) => (
          <div key={h.id} className={`${DIAG} min-h-10 border-t border-border tabular-nums`}>
            <span className="truncate">{displayName(h.name)}</span>
            <span>{h.connected ? "Connected" : "Offline"}</span>
            <span>{h.interval_ms ? `${h.interval_ms} ms` : "—"}</span>
            <span>{status.active === h.id ? `${h.notify_avg_ms.toFixed(1)} / ${h.notify_max_ms.toFixed(1)} ms` : "Not in use"}</span>
          </div>
        ))}
      </section>
      <p className="col-span-full text-[13px] text-muted-foreground">Closing the window keeps Bridge running in the tray.{version && ` Cross the Platforms ${version}.`}</p>
    </div>
  );
}
