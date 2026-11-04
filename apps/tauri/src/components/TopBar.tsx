import { Button } from "@/components/ui/button";
import { TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useHub, type Tab } from "@/lib/hub";
import { RAISED } from "@/lib/utils";

const TABS: [Tab, string][] = [["desk", "Desk"], ["transfers", "Transfers"], ["clipboard", "Clipboard"], ["settings", "Settings"]];

/** Must render inside <Tabs>. */
export default function TopBar() {
  const { statusLine, setTab } = useHub();
  return (
    <header className="flex h-11 flex-none items-center gap-3.5">
      <h1 className="text-[22px] font-bold">Bridge</h1>
      <TabsList aria-label="Main" className="h-11 gap-1 rounded-xl p-1">
        {TABS.map(([id, label]) => (
          <TabsTrigger key={id} value={id} className="h-9 rounded-[9px] px-3.5 data-active:bg-card dark:data-active:bg-card">{label}</TabsTrigger>
        ))}
      </TabsList>
      <p role="status" className="min-w-0 flex-1 truncate text-sm text-muted-foreground">{statusLine}</p>
      <Button variant="secondary" className={RAISED} onClick={() => setTab("desk")}>Add a device</Button>
    </header>
  );
}
