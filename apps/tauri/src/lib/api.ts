// Typed wrappers for the Rust commands in src-tauri/src/commands.rs. Types mirror the serde structs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Edge = "Left" | "Right" | "Top" | "Bottom";
export interface Attachment { device_id: string; monitor_id: string; edge: Edge }
export interface Calibration { gain: number; min_push: number; dwell_ms: number }
export interface Settings {
  version: number;
  attachments: Attachment[];
  calibration: Record<string, Calibration>;
  entry_hold_ms: number;
  start_with_windows: boolean;
  remote_width: Record<string, number>;
  clipboard_sync: boolean;
  receive_dir: string | null;
  device_links: Record<string, string>;
  device_colors: Record<string, string>;
  appearance: "system" | "light" | "dark";
}
export interface Rect { left: number; top: number; right: number; bottom: number }
export interface Monitor { id: string; rect: Rect }
export interface HostStatus { id: string; name: string | null; connected: boolean; interval_ms: number; notify_avg_ms: number; notify_max_ms: number }
export interface Status { advertising: boolean; hosts: HostStatus[]; active: string | null; notice: string | null }

export const DEFAULT_CALIBRATION: Calibration = { gain: 1.0, min_push: 96, dwell_ms: 300 };
export const DEFAULT_WIDTH = 1366;

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const reachablePeers = () => invoke<string[]>("reachable_peers");
export const getStatus = () => invoke<Status>("get_status");
export const listMonitors = () => invoke<Monitor[]>("list_monitors");
export const switchTo = (deviceId: string | null) => invoke<void>("switch_to", { deviceId });
export const onStatus = (cb: (s: Status) => void): Promise<UnlistenFn> => listen<Status>("status", (e) => cb(e.payload));

export type IpadStatus = "NotPaired" | "Connecting" | { Connected: { wifi: boolean } } | { Unavailable: string };
export interface ClipStatus { enabled: boolean; ipad: IpadStatus | null; synced: number; skipped_too_large: number; notice: string | null }

export const getClipStatus = () => invoke<ClipStatus>("get_clip_status");
export const setClipboardSync = (on: boolean) => invoke<void>("set_clipboard_sync", { on });
export const pairIpad = () => invoke<void>("pair_ipad");
export const onClipStatus = (cb: (s: ClipStatus) => void): Promise<UnlistenFn> => listen<ClipStatus>("clip-status", (e) => cb(e.payload));

export const hostName = (s: Status | null, id: string) => s?.hosts.find((h) => h.id === id)?.name ?? id;
