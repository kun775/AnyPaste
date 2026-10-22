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

export const onClipsChanged = (cb: () => void): Promise<UnlistenFn> => listen("clips-changed", cb);
export const onPopupShown = (cb: () => void): Promise<UnlistenFn> => listen("popup-shown", cb);
