// Add a device: which step the guide is on, and when to write a link. No React.
import { linkByName, type Device } from "./devices";
import type { PeerKind, Peer } from "./transfer";

export type AddKind = "ipad" | "phone";
export type Seen = { host: boolean; peer: boolean; clip: boolean };

/** Steps are Bluetooth, Files, then Clipboard for the iPad only. */
export const stepCount = (kind: AddKind): number => (kind === "ipad" ? 3 : 2);

/** The first step not seen and not skipped. Equals the step count when finished. */
export function addStep(kind: AddKind, seen: Seen, skipped: number[]): number {
  const done = [seen.host, seen.peer, seen.clip];
  for (let i = 0; i < stepCount(kind); i++) if (!done[i] && !skipped.includes(i)) return i;
  return stepCount(kind);
}

const PEER_KIND: Record<AddKind, PeerKind> = { ipad: "Ipad", phone: "Android" };

type Before = { hosts: string[]; peers: string[] };

/**
 * The host that appeared since the guide opened. One linked to a peer from before is an old device reconnecting.
 * One linked to a newer peer stays: that link is this guide's own, and dropping the host would undo the Bluetooth step.
 */
export const newHost = (before: Before, hosts: { id: string }[], links: Record<string, string>): string | undefined =>
  hosts.find((x) => !before.hosts.includes(x.id) && !before.peers.includes(links[x.id]))?.id;

/** The peer of this kind that appeared since the guide opened. */
export const newPeer = (before: Before, peers: Peer[], kind: AddKind): string | undefined =>
  peers.find((p) => p.kind === PEER_KIND[kind] && !before.peers.includes(p.id))?.id;

/**
 * The host and peer to link once both are new since the guide opened. `anchor` is a device that already has one side,
 * so only the other side must appear. Null when not ready or already linked.
 */
export function newLink(
  before: Before, hosts: { id: string }[], peers: Peer[], links: Record<string, string>,
  anchor: { hostId?: string; peerId?: string }, kind: AddKind,
): { host: string; peer: string } | null {
  const host = anchor.hostId ?? newHost(before, hosts, links);
  const peer = anchor.peerId ?? newPeer(before, peers, kind);
  return host && peer && links[host] !== peer ? { host, peer } : null;
}

/** The guide's last card: what really got set up, which skips can leave at nothing. */
export function finishLine(kind: AddKind, name: string, seen: Seen, placed: boolean): { title: string; body: string } {
  const place = placed ? "It already has its place on the Desk." : "Put it next to a screen edge on the Desk.";
  if (seen.host && seen.peer) return { title: `${name} is ready`, body: place };
  if (seen.host) return { title: `${name} is set up for keyboard and mouse`, body: `${place} Add files from its panel when you want them.` };
  if (seen.peer) return { title: `${name} is ready for ${kind === "ipad" ? "files" : "files and clipboard"}`, body: "Keyboard and mouse need Bluetooth first. Add it from its panel on the Desk." };
  if (seen.clip && kind === "ipad") return { title: "Clipboard is connected", body: "You skipped the other steps. Add a device again to set them up." };
  return { title: "Nothing was added", body: "You skipped every step. Add a device whenever you want." };
}

/** Links for unlinked hosts whose name is identical to exactly one unlinked peer, when no other unlinked host shares that name. */
export function nameLinks(hosts: { id: string; name: string | null }[], peers: Peer[], links: Record<string, string>): Record<string, string> {
  const free = peers.filter((p) => !Object.values(links).includes(p.id));
  const open = hosts.filter((h) => !links[h.id]);
  const out: Record<string, string> = {};
  for (const h of open) {
    if (open.filter((o) => o.name === h.name).length > 1) continue;
    const id = linkByName(h.name, free);
    if (id) out[h.id] = id;
  }
  return out;
}

/** Unlinked devices of the other kind that d could also be. */
export function linkChoices(d: Device, devices: Device[]): Device[] {
  if (d.hostId && d.peerId) return [];
  return devices.filter((o) => o.key !== d.key && !(o.hostId && o.peerId) && (d.hostId ? !o.hostId : !o.peerId));
}
