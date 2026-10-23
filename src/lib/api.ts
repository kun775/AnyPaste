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
  /** 置顶条目随第一页返回，排在最前，不计入分页。 */
  pinned: boolean;
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
  /** 弹出列表隐藏多少秒后释放内存；0 表示常驻。 */
  popupReleaseSecs: number;
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

export type SyncBackend = "webdav" | "s3";

/** 与后端 sync::SyncConfig 字段一致。 */
export interface SyncConfig {
  enabled: boolean;
  backend: SyncBackend;
  webdavEndpoint: string;
  webdavUsername: string;
  s3Endpoint: string;
  s3Bucket: string;
  s3Region: string;
  s3AccessKeyId: string;
  root: string;
  encrypt: boolean;
  intervalMins: number;
}

export interface SyncReport {
  uploaded: number;
  downloaded: number;
  deleted: number;
}

export interface SyncStatus {
  state: "disabled" | "idle" | "syncing" | "error";
  lastSyncAt: number | null;
  lastError: string | null;
  lastReport: SyncReport | null;
}

export interface SyncView {
  config: SyncConfig;
  hasPassword: boolean;
  hasSecretKey: boolean;
  hasPassphrase: boolean;
  status: SyncStatus;
}

/** 省略的字段表示不修改，空字符串表示清除。 */
export interface SecretsInput {
  password?: string;
  secretKey?: string;
  passphrase?: string;
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
export const setPinned = (id: number, pinned: boolean) => invoke<void>("set_pinned", { id, pinned });
/** 图片条目的原图 data URL。 */
export const clipImage = (id: number) => invoke<string>("clip_image", { id });
export const deleteClip = (id: number) => invoke<void>("delete_clip", { id });
export const clearTemp = () => invoke<number>("clear_temp");

export const getSettings = () => invoke<SettingsView>("get_settings");
export const updateSettings = (settings: Settings) => invoke<SettingsView>("update_settings", { settings });
export const setAutostart = (enabled: boolean) => invoke<SettingsView>("set_autostart", { enabled });
export const suspendHotkeys = (suspend: boolean) => invoke<void>("suspend_hotkeys", { suspend });
export const restartExplorer = () => invoke<SettingsView>("restart_explorer");
export const openDataDir = () => invoke<void>("open_data_dir");
export const storageStats = () => invoke<StorageStats>("storage_stats");

export const getSync = () => invoke<SyncView>("get_sync");
export const saveSync = (config: SyncConfig, secrets: SecretsInput) => invoke<SyncView>("save_sync", { config, secrets });
export const testSync = (config: SyncConfig, secrets: SecretsInput) => invoke<string>("test_sync", { config, secrets });
export const syncNow = () => invoke<void>("sync_now");
export const onSyncStatus = (cb: (s: SyncStatus) => void): Promise<UnlistenFn> =>
  listen<SyncStatus>("sync-status", (e) => cb(e.payload));

export const onClipsChanged =(cb: () => void): Promise<UnlistenFn> => listen("clips-changed", cb);
export const onPopupShown = (cb: () => void): Promise<UnlistenFn> => listen("popup-shown", cb);
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));
