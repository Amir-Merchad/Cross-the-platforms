// File transfer: typed wrappers for the transfer commands in src-tauri/src/commands.rs, plus formatting.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type PeerKind = "Windows" | "Android" | "Ipad";
export interface Peer { id: string; name: string; kind: PeerKind }
export interface FileMeta { name: string; size: number }
/** Mirrors core `node::Event` (serde tag "type"). Clipboard text never reaches the window. */
export type TransferEvent =
  | ({ type: "Paired" } & Peer)
  | { type: "Incoming"; from: string; files: FileMeta[] }
  | { type: "Started"; id: number; peer: string; outgoing: boolean; name: string; files: number; total: number; paths: string[] }
  | { type: "Requeued"; id: number }
  | { type: "Progress"; id: number; done: number; total: number }
  | { type: "Received"; from: string; path: string; size: number }
  | { type: "Failed"; id: number; reason: string }
  | { type: "Queued"; peer: string; files: number };

export const pairInvite = () => invoke<{ qr: string; expires_in_s: number }>("pair_invite");
export const pairedPeers = () => invoke<Peer[]>("paired_peers");
export const queuedFiles = () => invoke<Record<string, number>>("queued_files");
export const forgetPeer = (id: string) => invoke<void>("forget_peer", { id });
export const sendFiles = (peerId: string, paths: string[]) => invoke<void>("send_files", { peerId, paths });
export const receiveDir = () => invoke<string>("receive_dir");
export const setReceiveDir = (path: string) => invoke<void>("set_receive_dir", { path });
export const openReceiveDir = () => invoke<void>("open_receive_dir");
export const onTransfer = (cb: (e: TransferEvent) => void): Promise<UnlistenFn> => listen<TransferEvent>("transfer", (e) => cb(e.payload));

export function queuedLine(name: string, files: number): string {
  return files === 1 ? `1 file waiting — it goes when Bridge opens on ${name}` : `${files} files waiting — they go when Bridge opens on ${name}`;
}

/** 0–100, and 0 when there's nothing to send. */
export function progressPercent(done: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, (done / total) * 100));
}

export function formatRate(bytes: number, ms: number): string {
  if (ms <= 0) return "—";
  const perS = (bytes / ms) * 1000;
  return perS >= 1e6 ? `${(perS / 1e6).toFixed(1)} MB/s` : `${Math.round(perS / 1e3)} KB/s`;
}

export function formatBytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
  if (n >= 1e3) return `${Math.round(n / 1e3)} KB`;
  return `${n} B`;
}
