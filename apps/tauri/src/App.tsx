import { Tabs, TabsContent } from "@/components/ui/tabs";
import TopBar from "@/components/TopBar";
import { HubProvider, useHub, type Tab } from "@/lib/hub";
import AddDevice from "@/pages/AddDevice";
import Desk from "@/pages/Desk";
import Clipboard from "@/pages/Clipboard";
import Settings from "@/pages/Settings";
import Transfers from "@/pages/Transfers";

const PAGE = "flex min-h-0 flex-1 flex-col data-[hidden]:hidden";

function Shell() {
  const { tab, setTab } = useHub();
  return (
    <Tabs value={tab} onValueChange={(v) => setTab(v as Tab)} className="h-screen gap-3 bg-background px-4 py-3.5">
      <TopBar />
      <TabsContent value="desk" keepMounted className={`${PAGE} gap-3 text-[15px]!`}><Desk /></TabsContent>
      <TabsContent value="transfers" keepMounted className={PAGE}><Transfers /></TabsContent>
      <TabsContent value="clipboard" keepMounted className={PAGE}><Clipboard /></TabsContent>
      <TabsContent value="settings" keepMounted className={PAGE}><Settings /></TabsContent>
      {/* Mounted only while open, so it snapshots the devices on entry. */}
      {tab === "add" && <div className={PAGE}><AddDevice /></div>}
    </Tabs>
  );
}

export default function App() {
  return <HubProvider><Shell /></HubProvider>;
}
