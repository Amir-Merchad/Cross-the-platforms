// The hub's shared state: settings with autosave, engine status, devices and the transfer log.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  getClipStatus, getSettings, getStatus, listMonitors, onClipStatus, onStatus, reachablePeers, saveSettings,
  type ClipStatus, type Monitor, type Settings, type Status,
} from "./api";
import { applyEvent, type Row } from "./activity";
import { nameLinks, type AddKind } from "./addflow";
import { buildDevices, deskLine, migrateIpad, type Device } from "./devices";
import { onTransfer, pairedPeers, queuedFiles, type Peer } from "./transfer";

/** "add" is the Add a device page: reached from the top bar button and the device panel, not a tab. */
export type Tab = "desk" | "transfers" | "clipboard" | "settings" | "add";

/** What Add a device opens with. `hostId` or `peerId` is a device that already has that side. */
export type AddOpts = { kind: AddKind; hostId?: string; peerId?: string };

/** Runs saves one at a time, in call order. A failure rejects its own call and does not stop later ones. */
export function makeSaver(save: (s: Settings) => Promise<void>): (s: Settings) => Promise<void> {
  let tail: Promise<void> = Promise.resolve();
  return (s) => {
    const run = tail.then(() => save(s));
    tail = run.catch(() => {});
    return run;
  };
}

/**
 * Settings that apply at once and save in order. After a failure, state goes back to the last saved value
 * only once no save is pending, so a later save that already includes the failed edit is not undone.
 */
export function makeStore(initial: Settings, save: (s: Settings) => Promise<void>, onChange: (s: Settings) => void, onError: (e: unknown) => void) {
  const queue = makeSaver(save);
  let current = initial, lastSaved = initial, pending = 0, lastError: unknown;
  const update = (fn: (s: Settings) => Settings): Promise<void> => {
    current = fn(current);
    const next = current;
    onChange(next);
    pending++;
    return queue(next).then(() => { lastSaved = next; }, (e) => { lastError = e; }).then(() => {
      if (--pending === 0 && current !== lastSaved) { current = lastSaved; onChange(lastSaved); onError(lastError); }
    });
  };
  return { update, current: () => current };
}

/** Null when the transfer service is down. Never rejects, so nothing waits forever for a list that will not come. */
export const loadPeers = (list: () => Promise<Peer[]>): Promise<Peer[] | null> => list().catch(() => null);

/** Anything but "light" and "dark" follows the system. */
export const resolveDark = (appearance: string, systemDark: boolean): boolean =>
  appearance === "light" ? false : appearance === "dark" ? true : systemDark;

/** Advertising takes up to 12 s to start and blips on restart, so only a longer gap counts as Bluetooth being off. */
const BLUETOOTH_GRACE_MS = 15000;

export type Hub = {
  settings: Settings; update: (fn: (s: Settings) => Settings) => void;
  status: Status | null; clip: ClipStatus | null; monitors: Monitor[]; peers: Peer[]; loaded: boolean; reachable: string[]; waiting: Record<string, number>;
  devices: Device[]; rows: Row[]; say: (text: string) => void; statusLine: string; btOff: boolean; refreshPeers: () => Promise<void>;
  tab: Tab; setTab: (t: Tab) => void; addOpts: AddOpts; openAdd: (o?: AddOpts) => void; selected: string | null; select: (key: string | null) => void; current: Device | null;
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
  const [peersLoaded, setPeersLoaded] = useState(false);
  const [filesDown, setFilesDown] = useState(false);
  const [reachable, setReachable] = useState<string[]>([]);
  const [waiting, setWaiting] = useState<Record<string, number>>({});
  const [rows, setRows] = useState<Row[]>([]);
  const [said, setSaid] = useState("");
  const [tab, setTab] = useState<Tab>("desk");
  const [addOpts, setAddOpts] = useState<AddOpts>({ kind: "ipad" });
  const [selected, select] = useState<string | null>(null);
  const [graceOver, setGraceOver] = useState(false);

  const store = useRef<ReturnType<typeof makeStore> | null>(null);
  const sayTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const say = useCallback((text: string) => {
    setSaid(text);
    clearTimeout(sayTimer.current);
    sayTimer.current = setTimeout(() => setSaid(""), 8000);
  }, []);

  const update = useCallback((fn: (s: Settings) => Settings) => { store.current?.update(fn); }, []);
  const openAdd = useCallback((o: AddOpts = { kind: "ipad" }) => { setAddOpts(o); setTab("add"); }, []);
  const refreshPeers = useCallback(() => loadPeers(pairedPeers).then((p) => { if (p) setPeers(p); setFilesDown(!p); setPeersLoaded(true); }), []);

  useEffect(() => {
    getSettings().then((s) => { store.current = makeStore(s, saveSettings, setSettings, () => say("That did not save, so it went back. Try again.")); setSettings(s); });
    getStatus().then(setStatus);
    getClipStatus().then(setClip);
    listMonitors().then(setMonitors);
    const offs = [onStatus(setStatus), onClipStatus(setClip), onTransfer((e) => {
      setRows((r) => applyEvent(r, e, Date.now()));
      if (e.type === "Paired") refreshPeers();
      if (e.type === "Queued") setWaiting((w) => ({ ...w, [e.peer]: e.files }));
    })];
    const poll = () => { refreshPeers(); reachablePeers().then(setReachable, () => {}); queuedFiles().then(setWaiting, () => {}); };
    poll();
    const fast = setInterval(() => listMonitors().then(setMonitors), 3000);
    const slow = setInterval(poll, 5000);
    return () => { offs.forEach((o) => o.then((f) => f())); clearInterval(fast); clearInterval(slow); clearTimeout(sayTimer.current); };
  }, [say, refreshPeers]);

  const advertising = status?.advertising;
  useEffect(() => {
    if (advertising !== false) return;
    const t = setTimeout(() => setGraceOver(true), BLUETOOTH_GRACE_MS);
    return () => { clearTimeout(t); setGraceOver(false); };
  }, [advertising]);
  // Bluetooth switched off in Settings is a choice, not a fault: no banner, and the Wi-Fi iPad still works.
  const btOff = advertising === false && graceOver && settings?.bluetooth_enabled !== false;

  const appearance = settings?.appearance;
  useEffect(() => {
    if (appearance === undefined) return;
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const apply = () => document.documentElement.classList.toggle("dark", resolveDark(appearance, mq.matches));
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [appearance]);

  // The iPad moves from its Bluetooth host to ipad-wifi once; migrateIpad returns the same object when done.
  const migrated = useRef(false); // once per run: a failed save reverts, and must not retry in a loop
  useEffect(() => {
    if (!settings || !peersLoaded || migrated.current) return;
    if (migrateIpad(settings, peers) !== settings) { migrated.current = true; update((s) => migrateIpad(s, peers)); }
  }, [settings, peers, peersLoaded, update]);

  // Identical names link by themselves. Once written the link exists, so this finds nothing and does not loop.
  const hosts = status?.hosts;
  const tried = useRef(new Set<string>()); // a failed save reverts settings; do not retry the same host forever
  useEffect(() => {
    if (!settings || !hosts || !peersLoaded) return;
    const found = Object.fromEntries(Object.entries(nameLinks(hosts, peers, settings.device_links)).filter(([h]) => !tried.current.has(h)));
    Object.keys(found).forEach((h) => tried.current.add(h));
    if (Object.keys(found).length) update((s) => ({ ...s, device_links: { ...found, ...s.device_links } }));
  }, [settings, hosts, peers, peersLoaded, update]);

  const devices = useMemo(() => (settings ? buildDevices(status, peers, settings, reachable, waiting, monitors) : []), [status, peers, settings, reachable, waiting, monitors]);

  if (!settings) return null;
  const current = devices.find((d) => d.key === selected) ?? devices[0] ?? null;
  const statusLine = said || deskLine(status, rows, devices, current, btOff, filesDown);
  const hub: Hub = { settings, update, status, clip, monitors, peers, loaded: peersLoaded && status !== null, reachable, waiting, devices, rows, say, statusLine, btOff, refreshPeers, tab, setTab, addOpts, openAdd, selected, select, current };
  return <Ctx.Provider value={hub}>{children}</Ctx.Provider>;
}
