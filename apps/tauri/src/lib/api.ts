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
  bluetooth_enabled: boolean;
  retired_hosts: string[];
  /** Hosts the owner brought back from Hidden devices: never retired again. */
  kept_hosts: string[];
  /** Retired host id → its name when it retired, for Hidden devices. */
  retired_names: Record<string, string>;
  /** Free layout: device rectangles in desktop pixels. Empty means "made from attachments". */
  layout: Placement[];
}
export interface Placement { device_id: string; rect: Rect }
export interface Rect { left: number; top: number; right: number; bottom: number }
export interface Monitor { id: string; rect: Rect }
export interface HostStatus { id: string; name: string | null; connected: boolean; interval_ms: number; notify_avg_ms: number; notify_max_ms: number }
export type IpadInputState = "NotSetUp" | "Connecting" | "Ready" | "OnCall" | "NeedsUsb" | "NotFound" | "NeedsDeveloperMode";
export interface Status {
  advertising: boolean; hosts: HostStatus[]; active: string | null; notice: string | null;
  /** The Wi-Fi iPad's input link; absent from older engines. */
  ipad?: IpadInputState | null;
  bluetooth_off?: boolean;
}

export const DEFAULT_CALIBRATION: Calibration = { gain: 1.0, min_push: 96, dwell_ms: 300 };
export const DEFAULT_WIDTH = 1366;

export const getSettings = () => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings) => invoke<void>("save_settings", { settings });
export const reachablePeers = () => invoke<string[]>("reachable_peers");
export const getStatus = () => invoke<Status>("get_status");
export const listMonitors = () => invoke<Monitor[]>("list_monitors");
export const switchTo = (deviceId: string | null) => invoke<void>("switch_to", { deviceId });
export const openBluetoothSettings = () => invoke<void>("open_bluetooth_settings");
export const onStatus = (cb: (s: Status) => void): Promise<UnlistenFn> => listen<Status>("status", (e) => cb(e.payload));

export type IpadStatus = "NotPaired" | "Connecting" | { Connected: { wifi: boolean } } | { Unavailable: string };
export interface ClipStatus { enabled: boolean; ipad: IpadStatus | null; synced: number; skipped_too_large: number; notice: string | null; history_rev: number }

export const getClipStatus = () => invoke<ClipStatus>("get_clip_status");
export const setClipboardSync = (on: boolean) => invoke<void>("set_clipboard_sync", { on });
export const pairIpad = () => invoke<void>("pair_ipad");
/** Puts a files pairing code on the iPad's clipboard; the result arrives as a clipboard notice. */
export const inviteToIpad = () => invoke<void>("invite_to_ipad");
export const onClipStatus = (cb: (s: ClipStatus) => void): Promise<UnlistenFn> => listen<ClipStatus>("clip-status", (e) => cb(e.payload));


export interface HistoryEntry { id: number; kind: "text" | "image"; text: string | null; width: number; height: number; bytes: number; from: string; at: number; pinned: boolean }
export interface HistoryList { enabled: boolean; entries: HistoryEntry[] }
export const historyList = (query: string) => invoke<HistoryList>("history_list", { query });
export const historyCopy = (id: number) => invoke<void>("history_copy", { id });
export const historyPin = (id: number, pinned: boolean) => invoke<void>("history_pin", { id, pinned });
export const historyDelete = (id: number) => invoke<void>("history_delete", { id });
export const historyClear = () => invoke<void>("history_clear");
export const historyEnable = (on: boolean) => invoke<void>("history_enable", { on });

export interface Note { stamp: string; device: string; text: string }
export const notesList = () => invoke<Note[]>("notes_list");
export const notesOpen = () => invoke<void>("notes_open");
export const onNotesChanged = (cb: () => void): Promise<UnlistenFn> => listen("notes-changed", () => cb());
