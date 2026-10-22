import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Tab = "temp" | "perm";
export type ClipKind = "text" | "rich" | "image" | "files";

/** 与后端 store::list::PAGE_SIZE 保持一致。 */
export const PAGE_SIZE = 100;

export interface ClipItem {
  id: number;
  kind: ClipKind;
  preview: string;
  rich: boolean;
  thumb: string | null;
  sizeBytes: number;
  sourceApp: string | null;
  permanent: boolean;
  lastCopiedAt: number;
  expiresAt: number | null;
}

export interface Counts {
  temp: number;
  perm: number;
}

export interface Cursor {
  lastCopiedAt: number;
  id: number;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  logDir: string;
}

export type Theme = "system" | "light" | "dark";
export type PopupPosition = "cursor" | "center";

/** 与后端 settings::Settings 字段一致。 */
export interface Settings {
  closeToTray: boolean;
  paused: boolean;
  theme: Theme;
  popupPosition: PopupPosition;
  plainPasteDefault: boolean;
  hotkeyOpen: string;
  hotkeyPermanent: string;
  takeOverWinV: boolean;
  notifyPermanent: boolean;
  terminalApps: string[];
  ignoredApps: string[];
  tempTtlSecs: number;
  tempMaxItems: number;
  maxItemBytes: number;
  captureText: boolean;
  captureRich: boolean;
  captureImage: boolean;
  captureFiles: boolean;
}

export interface SettingsView {
  settings: Settings;
  autostart: boolean;
  winVReleased: boolean;
  winVActive: boolean;
}

export interface StorageStats {
  temp: number;
  perm: number;
  bytes: number;
}

export const hidePopup = () => invoke<void>("hide_popup");
export const openSettings = () => invoke<void>("open_settings");
export const isPaused = () => invoke<boolean>("is_paused");
export const appInfo = () => invoke<AppInfo>("app_info");

export const listClips = (tab: Tab, query: string, before: Cursor | null) =>
  invoke<ClipItem[]>("list_clips", { tab, query: query || null, before });
export const clipCounts = () => invoke<Counts>("clip_counts");
export const pasteClip = (id: number, plain: boolean) => invoke<{ pasted: boolean }>("paste_clip", { id, plain });
export const copyClip = (id: number) => invoke<void>("copy_clip", { id });
export const setPermanent = (id: number, permanent: boolean) => invoke<void>("set_permanent", { id, permanent });
export const deleteClip = (id: number) => invoke<void>("delete_clip", { id });
export const clearTemp = () => invoke<number>("clear_temp");

export const getSettings = () => invoke<SettingsView>("get_settings");
export const updateSettings = (settings: Settings) => invoke<SettingsView>("update_settings", { settings });
export const setAutostart = (enabled: boolean) => invoke<SettingsView>("set_autostart", { enabled });
export const suspendHotkeys = (suspend: boolean) => invoke<void>("suspend_hotkeys", { suspend });
export const restartExplorer = () => invoke<SettingsView>("restart_explorer");
export const openDataDir = () => invoke<void>("open_data_dir");
export const storageStats = () => invoke<StorageStats>("storage_stats");

export const onClipsChanged = (cb: () => void): Promise<UnlistenFn> => listen("clips-changed", cb);
export const onPopupShown = (cb: () => void): Promise<UnlistenFn> => listen("popup-shown", cb);
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));
