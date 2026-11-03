import { useEffect, useMemo, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { getClipStatus, getSettings, getStatus, listMonitors, onClipStatus, onStatus, saveSettings, type ClipStatus, type Monitor, type Settings, type Status } from "@/lib/api";
import { clashes } from "@/lib/layout";
import Devices from "@/pages/Devices";
import Layout from "@/pages/Layout";
import Calibration from "@/pages/Calibration";
import Diagnostics from "@/pages/Diagnostics";
import Clipboard from "@/pages/Clipboard";

export default function App() {
  const [saved, setSaved] = useState<Settings | null>(null);
  const [draft, setDraft] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [clip, setClip] = useState<ClipStatus | null>(null);
  const [monitors, setMonitors] = useState<Monitor[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then((s) => { setSaved(s); setDraft(s); });
    getStatus().then(setStatus);
    listMonitors().then(setMonitors);
    getClipStatus().then(setClip);
    const off = onStatus(setStatus);
    const offClip = onClipStatus(setClip);
    const t = setInterval(() => listMonitors().then(setMonitors), 3000);
    return () => { off.then((f) => f()); offClip.then((f) => f()); clearInterval(t); };
  }, []);

  const clash = useMemo(() => (draft ? clashes(draft.attachments) : null), [draft]);
  const dirty = JSON.stringify(saved) !== JSON.stringify(draft);
  if (!draft) return null;

  const save = async () => {
    try { await saveSettings(draft); setSaved(draft); setError(null); } catch (e) { setError(String(e)); }
  };

  return (
    <main className="flex h-screen flex-col gap-4 p-6">
      <header className="flex items-center justify-between">
        <h1 className="text-xl font-semibold tracking-tight">Cross the Platforms</h1>
        <div className="flex gap-2">
          <Button variant="outline" disabled={!dirty} onClick={() => setDraft(saved)}>Discard</Button>
          <Button disabled={!dirty || !!clash} onClick={save}>Save</Button>
        </div>
      </header>
      {(clash || error) && <Alert variant="destructive"><AlertDescription>{clash ?? error}</AlertDescription></Alert>}
      <Tabs defaultValue="devices" className="flex-1">
        <TabsList>
          <TabsTrigger value="devices">Devices</TabsTrigger>
          <TabsTrigger value="clipboard">Clipboard</TabsTrigger>
          <TabsTrigger value="layout">Layout</TabsTrigger>
          <TabsTrigger value="calibration">Calibration</TabsTrigger>
          <TabsTrigger value="diagnostics">Diagnostics</TabsTrigger>
        </TabsList>
        <TabsContent value="devices"><Devices status={status} settings={draft} /></TabsContent>
        <TabsContent value="clipboard"><Clipboard clip={clip} /></TabsContent>
        <TabsContent value="layout"><Layout status={status} settings={draft} monitors={monitors} onChange={setDraft} /></TabsContent>
        <TabsContent value="calibration"><Calibration status={status} settings={draft} onChange={setDraft} /></TabsContent>
        <TabsContent value="diagnostics"><Diagnostics status={status} /></TabsContent>
      </Tabs>
    </main>
  );
}
