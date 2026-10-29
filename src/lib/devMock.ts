// 开发用模拟后端：在普通浏览器中打开 `popup.html?mock` 即可调试界面，无需启动 Tauri。
// 仅在 `import.meta.env.DEV` 下被动态导入，不会进入生产包。
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { ClipItem, Cursor, Settings, SyncConfig, SyncStatus, Tab, UpdateStatus } from "./api";

const DEFAULT_SETTINGS: Settings = {
  closeToTray: true,
  paused: false,
  theme: "system",
  popupPosition: "cursor",
  plainPasteDefault: false,
  popupReleaseSecs: 60,
  hotkeyOpen: "alt+v",
  hotkeyPermanent: "ctrl+shift+c",
  takeOverWinV: false,
  notifyPermanent: true,
  autoCheckUpdate: true,
  autoInstallUpdate: false,
  terminalApps: ["WindowsTerminal.exe", "conhost.exe", "pwsh.exe"],
  ignoredApps: ["KeePassXC.exe", "1Password.exe"],
  tempTtlSecs: 86400,
  tempMaxItems: 500,
  maxItemBytes: 10 * 1024 * 1024,
  captureText: true,
  captureRich: true,
  captureImage: true,
  captureFiles: true,
};

const MIN = 60_000;
const HOUR = 60 * MIN;
const TTL = 24 * HOUR;

// 渐变示意图，作为图片缩略图占位
const THUMB =
  "data:image/svg+xml," +
  encodeURIComponent(
    '<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><defs><linearGradient id="g" x2="1" y2="1">' +
      '<stop offset="0" stop-color="#5b8def"/><stop offset=".55" stop-color="#9b6cf0"/><stop offset="1" stop-color="#f08bb4"/>' +
      '</linearGradient></defs><rect width="320" height="180" fill="url(#g)"/><circle cx="230" cy="150" r="70" fill="#fff" opacity=".25"/></svg>',
  );
// 原图与示例条目标注的 1280×720 一致，便于调试原始尺寸预览
const FULL = THUMB.replace(encodeURIComponent('width="320" height="180"'), encodeURIComponent('width="1280" height="720" viewBox="0 0 320 180"'));

function fixtures(): ClipItem[] {
  const now = Date.now();
  let id = 0;
  const make = (kind: ClipItem["kind"], preview: string, app: string, ago: number, extra: Partial<ClipItem> = {}) => {
    const lastCopiedAt = now - ago;
    return {
      id: ++id,
      kind,
      preview,
      rich: kind === "rich",
      thumb: null,
      sizeBytes: preview.length,
      sourceApp: app,
      permanent: false,
      pinned: false,
      lastCopiedAt,
      expiresAt: lastCopiedAt + TTL,
      ...extra,
    } satisfies ClipItem;
  };
  const list = [
    make("text", "会议改到周四下午 3 点，地点 B2 会议室，记得带上上季度的数据报表。", "WeChat.exe", 2 * MIN),
    make("text", "https://tauri.app/plugin/global-shortcut/", "chrome.exe", 14 * MIN),
    make("rich", "季度总结：用户增长 23%，留存率提升至 41%。", "WINWORD.EXE", 48 * MIN),
    make("image", "1280×720", "SnippingTool.exe", 3 * HOUR, { thumb: THUMB, sizeBytes: 245_000 }),
    make("files", "C:\\Users\\me\\design-v2.fig\nC:\\Users\\me\\logo.svg", "explorer.exe", 9 * HOUR),
    make("files", "C:\\Users\\me\\WeChat Files\\temp\\b810118b64edc447ef17d8e02.jpg", "Weixin.exe", 5 * MIN, {
      thumb: THUMB,
    }),
    make("text", "顺丰 SF1402938475612", "msedge.exe", 23 * HOUR + 20 * MIN),
    make("text", "huang@example.com", "Outlook.exe", 5 * 24 * HOUR, { permanent: true, expiresAt: null }),
    make("text", "git log --oneline --graph --decorate -20", "WindowsTerminal.exe", 2 * 24 * HOUR, {
      permanent: true,
      expiresAt: null,
    }),
  ];
  for (let i = 0; i < 230; i++) list.push(make("text", `批量条目 ${i + 1}`, "Code.exe", 30 * HOUR + i * MIN));
  return list;
}

export function installMock(window: "popup" | "settings") {
  let items = fixtures();
  let settings = { ...DEFAULT_SETTINGS };
  let autostart = false;
  let winVActive = false;
  let update: UpdateStatus = {
    checking: false,
    installing: false,
    progress: null,
    available: null,
    lastCheckedAt: null,
    error: null,
    portable: false,
  };
  const view = () => ({ settings, autostart, winVReleased: settings.takeOverWinV, winVActive });
  let sync: SyncConfig = {
    enabled: false,
    backend: "webdav",
    webdavEndpoint: "",
    webdavUsername: "",
    s3Endpoint: "",
    s3Bucket: "",
    s3Region: "auto",
    s3AccessKeyId: "",
    root: "/AnyPaste",
    encrypt: true,
    intervalMins: 15,
  };
  let saved = { password: false, secretKey: false, passphrase: false };
  let syncStatus: SyncStatus = { state: "disabled", lastSyncAt: null, lastError: null, lastReport: null };
  const syncView = () => ({
    config: sync,
    hasPassword: saved.password,
    hasSecretKey: saved.secretKey,
    hasPassphrase: saved.passphrase,
    status: syncStatus,
  });
  const visible = (tab: Tab, query: string | null) =>
    items
      .filter((i) => i.permanent === (tab === "perm"))
      .filter((i) => !query || i.preview.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => b.lastCopiedAt - a.lastCopiedAt || b.id - a.id);

  mockWindows(window);
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      const id = args.id as number;
      console.debug("[mock]", cmd, args);
      switch (cmd) {
        case "list_clips": {
          const before = args.before as Cursor | null;
          const all = visible(args.tab as Tab, args.query as string | null);
          const pinned = before ? [] : all.filter((i) => i.pinned);
          let list = all.filter((i) => !i.pinned);
          if (before) {
            list = list.filter(
              (i) => i.lastCopiedAt < before.lastCopiedAt || (i.lastCopiedAt === before.lastCopiedAt && i.id < before.id),
            );
          }
          return [...pinned, ...list.slice(0, 100)];
        }
        case "clip_counts":
          return { temp: visible("temp", null).length, perm: visible("perm", null).length };
        case "is_paused":
          return false;
        case "paste_clip":
          return { pasted: false };
        case "copy_clip":
          return null;
        case "set_permanent":
          items = items.map((i) =>
            i.id === id
              ? { ...i, permanent: args.permanent as boolean, expiresAt: args.permanent ? null : Date.now() + TTL }
              : i,
          );
          return null;
        case "set_pinned":
          items = items.map((i) => (i.id === id ? { ...i, pinned: args.pinned as boolean } : i));
          return null;
        case "clip_image":
          return FULL;
        case "delete_clip":
          items = items.filter((i) => i.id !== id);
          return null;
        case "clear_temp": {
          const n = items.filter((i) => !i.permanent).length;
          items = items.filter((i) => i.permanent);
          return n;
        }
        case "get_settings":
          return view();
        case "update_settings": {
          const next = args.settings as Settings;
          // 模拟后端校验：热键被占用
          if (next.hotkeyOpen === "ctrl+alt+delete") throw "热键 Ctrl+Alt+Delete 注册失败，可能已被其他程序占用";
          settings = next;
          if (!settings.takeOverWinV) winVActive = false;
          setTimeout(() => emit("settings-changed", settings));
          return view();
        }
        case "set_autostart":
          autostart = args.enabled as boolean;
          return view();
        case "restart_explorer":
          winVActive = settings.takeOverWinV;
          return view();
        case "get_sync":
          return syncView();
        case "test_sync": {
          const c = args.config as SyncConfig;
          if (!/^https?:\/\//.test(c.backend === "webdav" ? c.webdavEndpoint : c.s3Endpoint)) {
            throw "服务器地址需以 http:// 或 https:// 开头";
          }
          return "连接成功，可读写；云端尚无数据";
        }
        case "save_sync": {
          const s = args.secrets as Record<string, string>;
          saved = {
            password: s.password !== undefined ? !!s.password : saved.password,
            secretKey: s.secretKey !== undefined ? !!s.secretKey : saved.secretKey,
            passphrase: s.passphrase !== undefined ? !!s.passphrase : saved.passphrase,
          };
          const c = args.config as SyncConfig;
          if (c.enabled && c.encrypt && !saved.passphrase) throw "已开启端到端加密，请填写同步口令";
          sync = c;
          syncStatus = { ...syncStatus, state: c.enabled ? "syncing" : "disabled" };
          if (c.enabled) {
            setTimeout(() => {
              syncStatus = { state: "idle", lastSyncAt: Date.now(), lastError: null, lastReport: { uploaded: 2, downloaded: 1, deleted: 0 } };
              emit("sync-status", syncStatus);
            }, 300);
          }
          return syncView();
        }
        case "sync_now":
          if (!sync.enabled) throw "请先启用云同步";
          return null;
        case "storage_stats":
          return { temp: visible("temp", null).length, perm: visible("perm", null).length, bytes: 18_400_000 };
        case "app_info":
          return {
            version: "0.3.0-dev",
            dataDir: "C:\\Users\\me\\AppData\\Roaming\\AnyPaste",
            logDir: "C:\\Users\\me\\AppData\\Roaming\\AnyPaste\\logs",
            repoUrl: "https://github.com/kun775/AnyPaste",
            portable: false,
          };
        case "get_update_status":
          return update;
        case "check_update":
          update = {
            ...update,
            lastCheckedAt: Date.now(),
            available: { version: "0.4.0", notes: "- 新增功能示例\n- 修复问题示例", date: null },
          };
          setTimeout(() => emit("update-status", update));
          return update.available;
        case "install_update":
          update = { ...update, installing: true, progress: 0.35 };
          setTimeout(() => emit("update-status", update));
          return null;
        case "open_link":
          return null;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
