import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  dataDir: string;
  logDir: string;
}

export const hidePopup = () => invoke<void>("hide_popup");
export const openSettings = () => invoke<void>("open_settings");
export const isPaused = () => invoke<boolean>("is_paused");
export const appInfo = () => invoke<AppInfo>("app_info");
