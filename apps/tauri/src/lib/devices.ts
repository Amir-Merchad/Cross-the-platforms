// One list of devices from Bluetooth hosts and file peers, merged by the stored links. No React.
import type { Row } from "./activity";
import type { Attachment, ClipStatus, Edge, IpadInputState, Monitor, Settings, Status } from "./api";
import { besideMap } from "./space";
import { formatRate, type Peer, type PeerKind } from "./transfer";

export type Device = {
  key: string; hostId: string | null; peerId: string | null; name: string; kind: PeerKind | null;
  connected: boolean; reachable: boolean; waiting: number; placement: Attachment | null; missingScreen: boolean; colour: string;
  /** Placed beside another device (free layout) instead of at a screen edge: that device's host id and edge. */
  beside: { anchor: string; edge: Edge } | null;
};

/** At a screen edge or beside another device. */
export const isPlaced = (d: Pick<Device, "placement" | "beside">): boolean => !!d.placement || !!d.beside;

/** The one Wi-Fi iPad's host id (core ipadhid::IPAD_HOST). */
export const IPAD_HOST = "ipad-wifi";
const isWifi = (d: Device) => d.hostId === IPAD_HOST;

/** The iPad link in plain words, never a raw error. */
export function ipadLine(state: IpadInputState | null | undefined): string {
  switch (state) {
    case "Ready": return "Connected over Wi-Fi";
    case "OnCall": return "iPad is on a call";
    case "NeedsUsb": return "Plug in the iPad once";
    case "NotFound": return "iPad not found on Wi-Fi";
    case "NeedsDeveloperMode": return "Turn on Developer Mode";
    case "NotSetUp": return "Plug in by USB and tap Trust";
    default: return "Connecting…";
  }
}

type SeenHost = { id: string; name: string | null; connected: boolean };

/**
 * The Bluetooth hosts that are really the Wi-Fi iPad: linked to an iPad peer, or connected with no link and named like
 * one (hosts report no kind). The name only counts while ipad-wifi is connected, so a Bluetooth-only iPad keeps its link.
 * A host the owner brought back from Hidden devices (kept_hosts) is never retired again.
 */
export function ipadBluetoothHosts(s: Settings, peers: Peer[], hosts: SeenHost[]): string[] {
  const skip = [...(s.retired_hosts ?? []), ...(s.kept_hosts ?? [])];
  const ipads = peers.filter((p) => p.kind === "Ipad");
  const linked = Object.keys(s.device_links).filter((h) => h !== IPAD_HOST && !skip.includes(h) && ipads.some((p) => p.id === s.device_links[h]));
  if (!hosts.some((h) => h.id === IPAD_HOST && h.connected)) return linked;
  // ponytail: any "iPad" name counts, so a second iPad retires too until the owner shows it again in Settings.
  const named = hosts.filter((h) => h.id !== IPAD_HOST && h.connected && !skip.includes(h.id) && !s.device_links[h.id]
    && (h.name?.toLowerCase().includes("ipad") || ipads.some((p) => p.name === h.name)));
  return [...linked, ...named.map((h) => h.id)];
}

/** Hosts that may link to a peer by name: never one that is, or is about to be, retired. */
export function linkableHosts<H extends SeenHost>(s: Settings, peers: Peer[], hosts: H[]): H[] {
  const skip = [...(s.retired_hosts ?? []), ...ipadBluetoothHosts(s, peers, hosts)];
  return hosts.filter((h) => !skip.includes(h.id));
}

/**
 * Moves the iPad from its Bluetooth host to ipad-wifi: its place on the Desk (same list position, so the same hotkey),
 * its files link and its colour. What ipad-wifi already has stays, and the old place goes, so one iPad is never placed
 * twice. The old host is retired. Calibration stays behind on purpose: the Wi-Fi pointer behaves differently.
 * Returns the same object when there is nothing to do.
 */
export function migrateIpad(s: Settings, peers: Peer[], hosts: SeenHost[] = []): Settings {
  const [old] = ipadBluetoothHosts(s, peers, hosts);
  if (!old) return s;
  const move = <T extends { device_id: string }>(xs: T[]): T[] =>
    xs.some((x) => x.device_id === IPAD_HOST) ? xs.filter((x) => x.device_id !== old) : xs.map((x) => (x.device_id === old ? { ...x, device_id: IPAD_HOST } : x));
  const { [old]: oldPeer, ...links } = s.device_links;
  const { [old]: colour, ...colours } = s.device_colors;
  const name = hosts.find((h) => h.id === old)?.name ?? peers.find((p) => p.id === oldPeer)?.name;
  const free = peers.filter((p) => p.kind === "Ipad" && !Object.values(links).includes(p.id));
  const peerId = links[IPAD_HOST] ?? oldPeer ?? (free.length === 1 ? free[0].id : undefined);
  return migrateIpad({
    ...s,
    attachments: move(s.attachments),
    device_links: peerId ? { ...links, [IPAD_HOST]: peerId } : links,
    device_colors: colour ? { [IPAD_HOST]: colour, ...colours } : colours,
    layout: move(s.layout ?? []),
    retired_hosts: [...(s.retired_hosts ?? []), old],
    retired_names: name ? { ...s.retired_names, [old]: name } : s.retired_names,
  }, peers, hosts);
}

export const DEFAULT_COLOURS = ["#FF7A4D", "#23C48F", "#4DA9FF", "#F76CAE", "#FFC531"];

export const displayName = (name: string | null | undefined): string => name?.trim() || "Unnamed device";

/** Retired hosts for Settings. The engine no longer lists them, so the name is the one saved when they retired. */
export const hiddenDevices = (s: Settings): { id: string; name: string }[] =>
  (s.retired_hosts ?? []).map((id) => ({ id, name: displayName(s.retired_names?.[id]) }));

/** Brings a retired host back and keeps it from retiring again. It returns unplaced, like any new device. */
export function showHost(s: Settings, id: string): Settings {
  const { [id]: _name, ...retired_names } = s.retired_names ?? {};
  return { ...s, retired_hosts: (s.retired_hosts ?? []).filter((h) => h !== id), kept_hosts: [...(s.kept_hosts ?? []), id], retired_names };
}

/** The one peer whose name is identical to the host's, if there is exactly one. */
export function linkByName(hostName: string | null, peers: Peer[]): string | null {
  if (!hostName?.trim()) return null;
  const same = peers.filter((p) => p.name === hostName);
  return same.length === 1 ? same[0].id : null;
}

export function buildDevices(
  status: Status | null, peers: Peer[], settings: Settings, reachable: string[], waiting: Record<string, number>, monitors: Monitor[],
): Device[] {
  const beside = besideMap(settings, monitors);
  const make = (host: { id: string; name: string | null; connected: boolean } | null, peer: Peer | null, index: number): Device => {
    const hostId = host?.id ?? null;
    const peerId = peer?.id ?? null;
    const key = (hostId ?? peerId)!;
    const placement = hostId ? settings.attachments.find((a) => a.device_id === hostId) ?? null : null;
    return {
      key, hostId, peerId, name: displayName(host?.name?.trim() ? host.name : peer?.name), kind: peer?.kind ?? null,
      connected: host?.connected ?? false, reachable: !!peerId && reachable.includes(peerId), waiting: peerId ? waiting[peerId] ?? 0 : 0,
      placement, missingScreen: !!placement && !monitors.some((m) => m.id === placement.monitor_id),
      colour: settings.device_colors[key] ?? DEFAULT_COLOURS[index % DEFAULT_COLOURS.length],
      beside: (hostId && !placement && beside.get(hostId)) || null,
    };
  };
  // The engine lists only what it sees: Bluetooth off lists nothing, and an unplaced offline host is dropped.
  const retired = settings.retired_hosts ?? [];
  const listed = (status?.hosts ?? []).filter((h) => !retired.includes(h.id));
  const known = [...settings.attachments.map((a) => a.device_id), ...(settings.layout ?? []).map((p) => p.device_id), ...Object.keys(settings.device_links)].filter((id) => !retired.includes(id));
  const absent = [...new Set(known)].filter((id) => !listed.some((h) => h.id === id)).map((id) => ({ id, name: null, connected: false }));
  const linked = new Set<string>();
  const out: Device[] = [];
  for (const h of [...listed, ...absent].sort((a, b) => a.id.localeCompare(b.id))) {
    const peer = peers.find((p) => p.id === settings.device_links[h.id]) ?? null;
    if (peer) linked.add(peer.id);
    out.push(make(h, peer, out.length));
  }
  for (const p of peers) if (!linked.has(p.id)) out.push(make(null, p, out.length));
  return out;
}

/** Drops placement, link and colour. The Bluetooth bond lives in the OS and is not touched. */
export function forgetDevice(settings: Settings, d: Device): Settings {
  const without = (o: Record<string, string>, drop: (k: string, v: string) => boolean) =>
    Object.fromEntries(Object.entries(o).filter(([k, v]) => !drop(k, v)));
  return {
    ...settings,
    attachments: settings.attachments.filter((a) => a.device_id !== d.hostId),
    layout: (settings.layout ?? []).filter((p) => p.device_id !== d.hostId),
    device_links: without(settings.device_links, (k, v) => k === d.hostId || v === d.peerId),
    device_colors: without(settings.device_colors, (k) => k === d.hostId || k === d.peerId),
  };
}

/** What a device shows on the map. `off` drops its colour, so state never rests on hue alone. */
export function deviceLook(d: Device, active: string | null, btOff: boolean): { sub: string; off: boolean } {
  if (d.hostId && d.hostId === active) return { sub: "In control", off: false };
  if (!d.hostId) return d.reachable ? { sub: "Files only", off: false } : { sub: "Bridge closed", off: true };
  if (btOff && !isWifi(d)) return { sub: "No Bluetooth", off: true };
  const closed = !!d.peerId && !d.reachable && d.waiting > 0;
  if (d.missingScreen) return { sub: "Screen not connected", off: !d.connected || closed };
  if (!d.connected) return { sub: "Offline", off: true };
  if (closed) return { sub: "Bridge closed", off: true };
  return { sub: isPlaced(d) ? "Connected" : "Not placed", off: false };
}

function clipLine(d: Device, clip: ClipStatus | null): string {
  if (!clip) return "Not connected";
  if (!clip.enabled) return "Sync off";
  if (!d.peerId) return "Not paired";
  if (d.kind !== "Ipad") return d.reachable ? "Text only" : "Not connected";
  if (clip.ipad === "NotPaired") return "Not paired";
  if (clip.ipad && typeof clip.ipad === "object" && "Connected" in clip.ipad) return clip.ipad.Connected.wifi ? "Connected over Wi-Fi" : "Connected over USB";
  return "Not connected";
}

function filesLine(d: Device): string {
  if (!d.peerId) return "Not paired for files";
  if (d.waiting > 0) return `${d.waiting} waiting`;
  if (d.reachable) return "Ready";
  return d.kind === "Ipad" ? "Sends when it is back" : "Open Bridge there first";
}

/** The device panel's wording. `where` reads "right of Screen 1". */
export function panelLines(d: Device, c: { active: string | null; btOff: boolean; clip: ClipStatus | null; where: string; hotkey: number | null; ipad?: IpadInputState | null }) {
  if (isWifi(d)) c = { ...c, btOff: false };
  const inControl = !!d.hostId && d.hostId === c.active;
  const offline = !!d.hostId && !d.connected && !c.btOff;
  const why = inControl || offline ? "" : !d.hostId ? "It needs Bluetooth first." : c.btOff ? "Keyboard and mouse sharing is paused."
    : !isPlaced(d) ? "Put it next to a screen edge first." : "";
  const link = isWifi(d) ? "Wi-Fi" : "Bluetooth";
  const hid = inControl ? "In control" : !d.hostId ? "Needs Bluetooth" : c.btOff ? "Bluetooth is off" : !d.connected ? (isWifi(d) ? ipadLine(c.ipad) : "Not connected")
    : d.missingScreen ? `${link}, screen not connected` : isPlaced(d) ? `${link}, ${c.where}` : `${link}, not placed`;
  return {
    key: d.missingScreen ? "Screen not connected" : isPlaced(d) ? `Ctrl+Alt+${c.hotkey}` : "Not placed",
    button: inControl ? "Back to Windows" : offline ? `${d.name} is offline` : `Switch to ${d.name}`,
    canSwitch: inControl || (!offline && !why), why, hid, files: filesLine(d), clip: clipLine(d, c.clip),
  };
}

/** Like tray.rs status_line in the canvas wording, but never the engine notice: those can carry raw errors and ids. */
function restingLine(status: Status): string {
  if (status.active) return `Your keyboard and mouse are on ${displayName(status.hosts.find((h) => h.id === status.active)?.name)}.`;
  return status.advertising || status.bluetooth_off ? "Your keyboard and mouse are on Windows." : "Starting Bluetooth…";
}

/** The top bar's line when nothing has just been said. Input comes first: that is what must never surprise. */
export function deskLine(status: Status | null, rows: Row[], devices: Device[], selected: Device | null, btOff: boolean, filesDown: boolean): string {
  if (!status) return "";
  if (status.active) return `${restingLine(status)} Push back across the edge, or press Ctrl+Alt+Q.`;
  if (filesDown) return "File sending did not start. Quit Bridge from the tray and open it again.";
  const sending = rows.filter((r) => r.outgoing && r.state === "moving").at(-1);
  if (sending) {
    const rate = formatRate(sending.done, sending.now - sending.startedAt);
    return `Sending ${sending.name} to ${displayName(devices.find((d) => d.peerId === sending.peer)?.name)}${rate === "—" ? "" : ` at ${rate}`}.`;
  }
  if (btOff) return "Keyboard and mouse sharing is paused.";
  if (selected?.hostId && !selected.connected && !selected.reachable) return `${selected.name} went offline. Bridge reconnects by itself.`;
  if (selected?.peerId && !selected.reachable && selected.waiting > 0) return `${selected.name} has Bridge closed. Files will wait.`;
  return restingLine(status);
}

/** The activity strip when no file is moving. */
export function idleLine(devices: Device[]): string {
  if (devices.length === 0) return "Files you send show up here.";
  const d = devices.find((x) => x.waiting > 0);
  if (!d) return "No files moving.";
  return d.waiting === 1 ? `1 file waits for ${d.name}. It sends when Bridge opens there.` : `${d.waiting} files wait for ${d.name}. They send when Bridge opens there.`;
}
