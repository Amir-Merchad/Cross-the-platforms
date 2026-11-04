// One list of devices from Bluetooth hosts and file peers, merged by the stored links. No React.
import type { Attachment, Monitor, Settings, Status } from "./api";
import type { Peer, PeerKind } from "./transfer";

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
