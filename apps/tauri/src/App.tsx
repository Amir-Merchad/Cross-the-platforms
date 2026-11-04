import { Tabs, TabsContent } from "@/components/ui/tabs";
import TopBar from "@/components/TopBar";
import { HubProvider, useHub, type Tab } from "@/lib/hub";
import Devices from "@/pages/Devices";
import Layout from "@/pages/Layout";
import Calibration from "@/pages/Calibration";
import Diagnostics from "@/pages/Diagnostics";
import Clipboard from "@/pages/Clipboard";
import Transfer from "@/pages/Transfer";

const PANE = "data-[hidden]:hidden min-h-0 flex-1 overflow-auto rounded-[20px] bg-card p-4";

// Temporary content: later tasks replace each pane with its new page.
function Shell() {
  const { tab, setTab, settings, update, status, clip, monitors } = useHub();
  const onChange = (next: typeof settings) => update(() => next);
  return (
    <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="h-screen gap-3 bg-background px-4 py-3.5">
      <TopBar />
      <TabsContent value="desk" keepMounted className={PANE}>
        <Devices status={status} settings={settings} />
        <Layout status={status} settings={settings} monitors={monitors} onChange={onChange} />
      </TabsContent>
      <TabsContent value="transfers" keepMounted className={PANE}><Transfer /></TabsContent>
      <TabsContent value="clipboard" keepMounted className={PANE}><Clipboard clip={clip} /></TabsContent>
      <TabsContent value="settings" keepMounted className={PANE}>
        <Calibration status={status} settings={settings} onChange={onChange} />
        <Diagnostics status={status} />
      </TabsContent>
    </Tabs>
  );
}

export default function App() {
  return <HubProvider><Shell /></HubProvider>;
}
