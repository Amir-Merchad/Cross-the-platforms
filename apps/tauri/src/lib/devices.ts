// One list of devices from Bluetooth hosts and file peers, merged by the stored links. No React.
import type { Row } from "./activity";
import type { Attachment, ClipStatus, Monitor, Settings, Status } from "./api";
import { formatRate, type Peer, type PeerKind } from "./transfer";

export type Device = {
  key: string; hostId: string | null; peerId: string | null; name: string; kind: PeerKind | null;
  connected: boolean; reachable: boolean; waiting: number; placement: Attachment | null; missingScreen: boolean; colour: string;
};

export const DEFAULT_COLOURS = ["#FF7A4D", "#23C48F", "#4DA9FF", "#F76CAE", "#FFC531"];

export const displayName = (name: string | null | undefined): string => name?.trim() || "Unnamed device";

/** The one peer whose name is identical to the host's, if there is exactly one. */
export function linkByName(hostName: string | null, peers: Peer[]): string | null {
  if (!hostName?.trim()) return null;
  const same = peers.filter((p) => p.name === hostName);
  return same.length === 1 ? same[0].id : null;
}

export function buildDevices(
  status: Status | null, peers: Peer[], settings: Settings, reachable: string[], waiting: Record<string, number>, monitors: Monitor[],
): Device[] {
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
    };
  };
  const linked = new Set<string>();
  const out: Device[] = [];
  for (const h of status?.hosts ?? []) {
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
    device_links: without(settings.device_links, (k, v) => k === d.hostId || v === d.peerId),
    device_colors: without(settings.device_colors, (k) => k === d.hostId || k === d.peerId),
  };
}

/** What a device shows on the map. `off` drops its colour, so state never rests on hue alone. */
export function deviceLook(d: Device, active: string | null, btOff: boolean): { sub: string; off: boolean } {
  if (d.hostId && d.hostId === active) return { sub: "In control", off: false };
  if (!d.hostId) return d.reachable ? { sub: "Files only", off: false } : { sub: "Bridge closed", off: true };
  if (btOff) return { sub: "No Bluetooth", off: true };
  if (!d.connected) return d.reachable ? { sub: "Files only", off: false } : { sub: "Offline", off: true };
  if (d.peerId && !d.reachable && d.waiting > 0) return { sub: "Bridge closed", off: true };
  if (d.missingScreen) return { sub: "Screen not connected", off: false };
  return { sub: d.placement ? "Connected" : "Not placed", off: false };
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
export function panelLines(d: Device, c: { active: string | null; btOff: boolean; clip: ClipStatus | null; where: string; hotkey: number | null }) {
  const inControl = !!d.hostId && d.hostId === c.active;
  const offline = !!d.hostId && !d.connected && !c.btOff;
  const why = inControl || offline ? "" : !d.hostId ? "It needs Bluetooth first." : c.btOff ? "Keyboard and mouse sharing is paused."
    : d.missingScreen ? "Its screen is not connected." : !d.placement ? "Put it next to a screen edge first." : "";
  const hid = inControl ? "In control" : !d.hostId ? "Needs Bluetooth" : c.btOff ? "Bluetooth is off" : !d.connected ? "Not connected"
    : d.missingScreen ? "Bluetooth, screen not connected" : d.placement ? `Bluetooth, ${c.where}` : "Bluetooth, not placed";
  return {
    key: d.missingScreen ? "Screen not connected" : d.placement ? `Ctrl+Alt+${c.hotkey}` : "Not placed",
    button: inControl ? "Back to Windows" : offline ? `${d.name} is offline` : `Switch to ${d.name}`,
    canSwitch: inControl || (!offline && !why), why, hid, files: filesLine(d), clip: clipLine(d, c.clip),
  };
}

/** Mirrors tray.rs status_line, in the canvas wording. */
function restingLine(status: Status): string {
  if (status.active) return `Your keyboard and mouse are on ${displayName(status.hosts.find((h) => h.id === status.active)?.name)}.`;
  return status.notice ?? "Your keyboard and mouse are on Windows.";
}

/** The top bar's line when nothing has just been said. Input comes first: that is what must never surprise. */
export function deskLine(status: Status | null, rows: Row[], devices: Device[], selected: Device | null, btOff: boolean): string {
  if (!status) return "";
  if (status.active) return `${restingLine(status)} Push back across the edge, or press Ctrl+Alt+Q.`;
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
