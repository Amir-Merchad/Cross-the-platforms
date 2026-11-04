// The hub's shared state: settings with autosave, engine status, devices and the transfer log.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  getClipStatus, getSettings, getStatus, listMonitors, onClipStatus, onStatus, reachablePeers, saveSettings,
  type ClipStatus, type Monitor, type Settings, type Status,
} from "./api";
import { applyEvent, type Row } from "./activity";
import { buildDevices, displayName, type Device } from "./devices";
import { onTransfer, pairedPeers, queuedFiles, type Peer } from "./transfer";

export type Tab = "desk" | "transfers" | "clipboard" | "settings";

/** Runs saves one at a time, in call order. A failure rejects its own call and does not stop later ones. */
export function makeSaver(save: (s: Settings) => Promise<void>): (s: Settings) => Promise<void> {
  let tail: Promise<void> = Promise.resolve();
  return (s) => {
    const run = tail.then(() => save(s));
    tail = run.catch(() => {});
    return run;
  };
}

/** Anything but "light" and "dark" follows the system. */
export const resolveDark = (appearance: string, systemDark: boolean): boolean =>
  appearance === "light" ? false : appearance === "dark" ? true : systemDark;

/** Mirrors tray.rs status_line, in the canvas wording. */
export function derivedLine(status: Status | null): string {
  if (!status) return "";
  if (status.active) return `Your keyboard and mouse are on ${displayName(status.hosts.find((h) => h.id === status.active)?.name)}.`;
  return status.notice ?? "Your keyboard and mouse are on Windows.";
}

export type Hub = {
  settings: Settings; update: (fn: (s: Settings) => Settings) => void;
  status: Status | null; clip: ClipStatus | null; monitors: Monitor[]; peers: Peer[]; reachable: string[]; waiting: Record<string, number>;
  devices: Device[]; rows: Row[]; say: (text: string) => void; statusLine: string;
  tab: Tab; setTab: (t: Tab) => void; selected: string | null; select: (key: string | null) => void;
};

const Ctx = createContext<Hub | null>(null);
export const useHub = (): Hub => {
  const h = useContext(Ctx);
  if (!h) throw new Error("useHub outside HubProvider");
  return h;
};

export function HubProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [clip, setClip] = useState<ClipStatus | null>(null);
  const [monitors, setMonitors] = useState<Monitor[]>([]);
  const [peers, setPeers] = useState<Peer[]>([]);
  const [reachable, setReachable] = useState<string[]>([]);
  const [waiting, setWaiting] = useState<Record<string, number>>({});
  const [rows, setRows] = useState<Row[]>([]);
  const [said, setSaid] = useState("");
  const [tab, setTab] = useState<Tab>("desk");
  const [selected, select] = useState<string | null>(null);

  const current = useRef<Settings | null>(null);
  const lastSaved = useRef<Settings | null>(null);
  const sayTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const say = useCallback((text: string) => {
    setSaid(text);
    clearTimeout(sayTimer.current);
    sayTimer.current = setTimeout(() => setSaid(""), 8000);
  }, []);

  const saver = useRef(makeSaver(saveSettings));
  const update = useCallback((fn: (s: Settings) => Settings) => {
    if (!current.current) return;
    const next = fn(current.current);
    current.current = next;
    setSettings(next);
    saver.current(next).then(
      () => { lastSaved.current = next; },
      (e) => { current.current = lastSaved.current; setSettings(lastSaved.current); say(`Could not save: ${String(e)}`); },
    );
  }, [say]);

  useEffect(() => {
    getSettings().then((s) => { current.current = lastSaved.current = s; setSettings(s); });
    getStatus().then(setStatus);
    getClipStatus().then(setClip);
    listMonitors().then(setMonitors);
    const offs = [onStatus(setStatus), onClipStatus(setClip), onTransfer((e) => {
      setRows((r) => applyEvent(r, e, Date.now()));
      if (e.type === "Paired") pairedPeers().then(setPeers);
    })];
    const poll = () => { pairedPeers().then(setPeers); reachablePeers().then(setReachable); queuedFiles().then(setWaiting); };
    poll();
    const fast = setInterval(() => listMonitors().then(setMonitors), 3000);
    const slow = setInterval(poll, 5000);
    return () => { offs.forEach((o) => o.then((f) => f())); clearInterval(fast); clearInterval(slow); clearTimeout(sayTimer.current); };
  }, []);

  const appearance = settings?.appearance;
  useEffect(() => {
    if (appearance === undefined) return;
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => document.documentElement.classList.toggle("dark", resolveDark(appearance, mq.matches));
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [appearance]);

  const devices = useMemo(() => (settings ? buildDevices(status, peers, settings, reachable, waiting, monitors) : []), [status, peers, settings, reachable, waiting, monitors]);

  if (!settings) return null;
  const hub: Hub = { settings, update, status, clip, monitors, peers, reachable, waiting, devices, rows, say, statusLine: said || derivedLine(status), tab, setTab, selected, select };
  return <Ctx.Provider value={hub}>{children}</Ctx.Provider>;
}
