import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Tab = "temp" | "perm";
export type ClipKind = "text" | "rich" | "image" | "files";
/** 列表的类型筛选；`text` 含富文本与链接，`link` 为整段是网址的文本。 */
export type KindFilter = "text" | "link" | "image" | "files";

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
  /** 通过 AnyPaste 实际粘贴的次数，用于按使用频率排序。 */
  pasteCount: number;
}

export interface ClipText {
  text: string;
  /** 文本的总字符数。 */
  chars: number;
  truncated: boolean;
}

export interface Counts {
  temp: number;
  perm: number;
  snippets: number;
}

/** 用户保存的常用文本片段；`preview` 只含开头部分。 */
export interface SnippetItem {
  id: number;
  title: string;
  preview: string;
  chars: number;
  updatedAt: number;
  pasteCount: number;
}

/** 编辑片段时读取的完整内容。 */
export interface SnippetDraft {
  id: number;
  title: string;
  content: string;
}

export interface Cursor {
  lastCopiedAt: number;
  id: number;
  /** 按频率分页时的粘贴次数；按时间排序时后端会忽略。 */
  pasteCount: number;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  logDir: string;
  repoUrl: string;
  portable: boolean;
}

export interface UpdateInfo {
  version: string;
  notes: string | null;
  date: string | null;
}

export interface UpdateStatus {
  checking: boolean;
  installing: boolean;
  /** 下载进度 0–1；总大小未知时为 null。 */
  progress: number | null;
  available: UpdateInfo | null;
  lastCheckedAt: number | null;
  error: string | null;
  portable: boolean;
}

export type Theme = "system" | "light" | "dark";
export type PopupPosition = "cursor" | "center";

/** 与后端 settings::Settings 字段一致。 */
export interface Settings {
  closeToTray: boolean;
  paused: boolean;
  theme: Theme;
  popupPosition: PopupPosition;
  /** 在「永久」和「片段」中按实际粘贴次数从高到低排列（置顶条目始终在最前）；「临时」始终按最近复制时间排列。 */
  sortByFrequency: boolean;
  plainPasteDefault: boolean;
  /** 弹出列表隐藏多少秒后释放内存；0 表示常驻。 */
  popupReleaseSecs: number;
  hotkeyOpen: string;
  hotkeyPermanent: string;
  takeOverWinV: boolean;
  notifyPermanent: boolean;
  autoCheckUpdate: boolean;
  autoInstallUpdate: boolean;
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

export type PreviewMode =
  | { mode: "restore" }
  | { mode: "fit"; width: number; height: number }
  | { mode: "fullscreen" };

export const hidePopup = () => invoke<void>("hide_popup");
export const setPopupPreview = (mode: PreviewMode) => invoke<void>("set_popup_preview", { mode });
export const openSettings = () => invoke<void>("open_settings");
export const isPaused = () => invoke<boolean>("is_paused");
export const appInfo = () => invoke<AppInfo>("app_info");

export const listClips = (tab: Tab, kind: KindFilter | null, query: string, before: Cursor | null) =>
  invoke<ClipItem[]>("list_clips", { tab, kind, query: query || null, before });
export const clipCounts = () => invoke<Counts>("clip_counts");
export const pasteClip = (id: number, plain: boolean) => invoke<{ pasted: boolean }>("paste_clip", { id, plain });
export const copyClip = (id: number) => invoke<void>("copy_clip", { id });
export const setPermanent = (id: number, permanent: boolean) => invoke<void>("set_permanent", { id, permanent });
export const setPinned = (id: number, pinned: boolean) => invoke<void>("set_pinned", { id, pinned });
/** 图片条目的原图 data URL。 */
export const clipImage = (id: number) => invoke<string>("clip_image", { id });
/** 条目的完整文本（文件条目为换行分隔的路径）；超长时只返回开头部分。 */
export const clipText = (id: number) => invoke<ClipText>("clip_text", { id });
export const deleteClip =(id: number) => invoke<void>("delete_clip", { id });
export const clearTemp = () => invoke<number>("clear_temp");

export const listSnippets = (query: string | null) => invoke<SnippetItem[]>("list_snippets", { query });
export const getSnippet = (id: number) => invoke<SnippetDraft>("get_snippet", { id });
export const saveSnippet = (id: number | null, title: string, content: string) =>
  invoke<SnippetItem>("save_snippet", { id, title, content });
export const deleteSnippet = (id: number) => invoke<void>("delete_snippet", { id });
export const saveClipAsSnippet = (id: number) => invoke<SnippetItem>("save_clip_as_snippet", { id });
export const pasteSnippet = (id: number) => invoke<{ pasted: boolean }>("paste_snippet", { id });
export const copySnippet = (id: number) => invoke<void>("copy_snippet", { id });
export const snippetText = (id: number) => invoke<ClipText>("snippet_text", { id });

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

export const getUpdateStatus = () => invoke<UpdateStatus>("get_update_status");
export const checkUpdate = () => invoke<UpdateInfo | null>("check_update");
export const installUpdate = () => invoke<void>("install_update");
export const openLink = (link: "repo" | "releases" | "issues") => invoke<void>("open_link", { link });
export const onUpdateStatus = (cb: (s: UpdateStatus) => void): Promise<UnlistenFn> =>
  listen<UpdateStatus>("update-status", (e) => cb(e.payload));

export const onClipsChanged = (cb: () => void): Promise<UnlistenFn> => listen("clips-changed", cb);
export const onSnippetsChanged = (cb: () => void): Promise<UnlistenFn> => listen("snippets-changed", cb);
export const onPopupShown = (cb: () => void): Promise<UnlistenFn> => listen("popup-shown", cb);
export const onSettingsChanged = (cb: (s: Settings) => void): Promise<UnlistenFn> =>
  listen<Settings>("settings-changed", (e) => cb(e.payload));
