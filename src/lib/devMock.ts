// 开发用模拟后端：在普通浏览器中打开 `popup.html?mock` 即可调试界面，无需启动 Tauri。
// 仅在 `import.meta.env.DEV` 下被动态导入，不会进入生产包。
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type {
  ClipItem,
  Cursor,
  KindFilter,
  Settings,
  SnippetItem,
  SyncConfig,
  SyncStatus,
  Tab,
  UpdateStatus,
} from "./api";

const DEFAULT_SETTINGS: Settings = {
  closeToTray: true,
  paused: false,
  theme: "system",
  popupPosition: "cursor",
  sortByFrequency: true,
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

// 长文本示例：列表只含前 400 个字符，全文由 clip_text 返回
const LONG_TEXT = Array.from(
  { length: 40 },
  (_, i) => `第 ${i + 1} 段：剪贴板历史会保留完整内容，列表中只显示开头两行，按空格可以查看全文并确认后再粘贴。`,
).join("\n");

const isLink = (text: string) => /^https?:\/\/\S+$/i.test(text.trim());

function matchesKind(item: ClipItem, kind: KindFilter | null): boolean {
  if (!kind) return true;
  const textual = item.kind === "text" || item.kind === "rich";
  if (kind === "text") return textual;
  if (kind === "link") return textual && isLink(item.preview);
  return item.kind === kind;
}

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
      pasteCount: 0,
      ...extra,
    } satisfies ClipItem;
  };
  const list = [
    make("text", "会议改到周四下午 3 点，地点 B2 会议室，记得带上上季度的数据报表。", "WeChat.exe", 2 * MIN, {
      pasteCount: 12,
    }),
    make("text", "https://tauri.app/plugin/global-shortcut/", "chrome.exe", 14 * MIN, { pasteCount: 7 }),
    make("rich", "季度总结：用户增长 23%，留存率提升至 41%。", "WINWORD.EXE", 48 * MIN, { pasteCount: 4 }),
    make("image", "1280×720", "SnippingTool.exe", 3 * HOUR, { thumb: THUMB, sizeBytes: 245_000 }),
    make("files", "C:\\Users\\me\\design-v2.fig\nC:\\Users\\me\\logo.svg", "explorer.exe", 9 * HOUR),
    make("files", "C:\\Users\\me\\WeChat Files\\temp\\b810118b64edc447ef17d8e02.jpg", "Weixin.exe", 5 * MIN, {
      thumb: THUMB,
    }),
    make("text", "顺丰 SF1402938475612", "msedge.exe", 23 * HOUR + 20 * MIN),
    make("text", "#4CC2FF", "Figma.exe", 20 * MIN),
    make("text", "rgba(194, 124, 0, 0.5)", "Code.exe", 25 * MIN),
    make("text", LONG_TEXT.slice(0, 400), "Code.exe", 30 * MIN, { sizeBytes: LONG_TEXT.length }),
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
  type MockSnippet = SnippetItem & { content: string };
  const snippetOf = (id: number, title: string, content: string, updatedAt: number): MockSnippet => ({
    id,
    title,
    content,
    preview: [...content].slice(0, 400).join(""),
    chars: [...content].length,
    updatedAt,
  });
  let snippets: MockSnippet[] = [
    snippetOf(
      1,
      "工作邮件开头",
      "你好，\n\n附件是本周的进度报告，请查收。如有问题随时联系。\n\n谢谢！",
      Date.now() - 2 * HOUR,
    ),
    snippetOf(2, "公司地址", "上海市浦东新区示例路 88 号 A 座 12 楼", Date.now() - 26 * HOUR),
    snippetOf(3, "常用审查命令", "cargo clippy --all-targets -- -D warnings\ncargo test", Date.now() - 3 * 24 * HOUR),
  ];
  let nextSnippetId = 100;
  const snippetItem = (s: MockSnippet): SnippetItem => ({
    id: s.id,
    title: s.title,
    preview: s.preview,
    chars: s.chars,
    updatedAt: s.updatedAt,
  });
  const emitSnippets = () => emit("snippets-changed");
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
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
  const emitUpdate = () => emit("update-status", update);
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
  const fullClipText = (item: ClipItem | undefined) => {
    const preview = item?.preview ?? "";
    return LONG_TEXT.startsWith(preview) && preview.length === 400 ? LONG_TEXT : preview;
  };
  const snippetPreview = (content: string, query: string) => {
    const at = query ? content.toLowerCase().indexOf(query.toLowerCase()) : -1;
    const start = at > 48 ? at - 24 : 0;
    return (start > 0 ? "…" : "") + content.slice(start, start + 400);
  };
  const snippetTitleOf = (text: string) => {
    const first = text.split(/\r?\n/).map((line) => line.trim()).find(Boolean) ?? "";
    return first ? (first.length > 40 ? `${first.slice(0, 40)}…` : first) : "剪贴板片段";
  };

  mockWindows(window);
  mockIPC(
    async (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      const id = args.id as number;
      console.debug("[mock]", cmd, args);
      switch (cmd) {
        case "list_clips": {
          const before = args.before as Cursor | null;
          const kind = (args.kind ?? null) as KindFilter | null;
          const byFrequency = settings.sortByFrequency;
          const all = visible(args.tab as Tab, args.query as string | null).filter((i) => matchesKind(i, kind));
          const pinned = before ? [] : all.filter((i) => i.pinned);
          let list = all
            .filter((i) => !i.pinned)
            .sort((a, b) =>
              byFrequency
                ? b.pasteCount - a.pasteCount || b.lastCopiedAt - a.lastCopiedAt || b.id - a.id
                : b.lastCopiedAt - a.lastCopiedAt || b.id - a.id,
            );
          if (before) {
            list = list.filter((i) =>
              byFrequency
                ? i.pasteCount < before.pasteCount ||
                  (i.pasteCount === before.pasteCount &&
                    (i.lastCopiedAt < before.lastCopiedAt ||
                      (i.lastCopiedAt === before.lastCopiedAt && i.id < before.id)))
                : i.lastCopiedAt < before.lastCopiedAt || (i.lastCopiedAt === before.lastCopiedAt && i.id < before.id),
            );
          }
          return [...pinned, ...list.slice(0, 100)];
        }
        case "clip_counts":
          return { temp: visible("temp", null).length, perm: visible("perm", null).length, snippets: snippets.length };
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
        case "clip_text": {
          const text = fullClipText(items.find((i) => i.id === id));
          return { text, chars: [...text].length, truncated: false };
        }
        case "delete_clip":
          items = items.filter((i) => i.id !== id);
          return null;
        case "clear_temp": {
          const n = items.filter((i) => !i.permanent).length;
          items = items.filter((i) => i.permanent);
          return n;
        }
        case "list_snippets": {
          const q = ((args.query as string | null) ?? "").trim().toLowerCase();
          return snippets
            .filter((s) => !q || s.title.toLowerCase().includes(q) || s.content.toLowerCase().includes(q))
            .sort((a, b) => b.updatedAt - a.updatedAt || b.id - a.id)
            .map((s) => ({ ...snippetItem(s), preview: snippetPreview(s.content, q) }));
        }
        case "get_snippet": {
          const s = snippets.find((item) => item.id === id);
          if (!s) throw "片段不存在或已删除";
          return { id: s.id, title: s.title, content: s.content };
        }
        case "save_snippet": {
          const title = (args.title as string).trim();
          const content = args.content as string;
          if (!title) throw "片段标题不能为空";
          if (!content.trim()) throw "片段内容不能为空";
          const sid = args.id as number | null;
          let saved: MockSnippet;
          if (sid) {
            const index = snippets.findIndex((s) => s.id === sid);
            if (index < 0) throw "片段不存在或已删除";
            saved = snippetOf(sid, title, content, Date.now());
            snippets = snippets.map((s) => (s.id === sid ? saved : s));
          } else {
            saved = snippetOf(++nextSnippetId, title, content, Date.now());
            snippets = [...snippets, saved];
          }
          emitSnippets();
          return snippetItem(saved);
        }
        case "delete_snippet": {
          const count = snippets.length;
          snippets = snippets.filter((s) => s.id !== id);
          if (snippets.length === count) throw "片段不存在或已删除";
          emitSnippets();
          return null;
        }
        case "save_clip_as_snippet": {
          const clip = items.find((i) => i.id === id);
          if (!clip) throw "条目不存在或已删除";
          if (clip.kind !== "text" && clip.kind !== "rich") throw "只有文本条目可以存为片段";
          const text = fullClipText(clip);
          const saved = snippetOf(++nextSnippetId, snippetTitleOf(text), text, Date.now());
          snippets = [...snippets, saved];
          emitSnippets();
          return snippetItem(saved);
        }
        case "snippet_text": {
          const s = snippets.find((item) => item.id === id);
          if (!s) throw "片段不存在或已删除";
          return { text: s.content, chars: s.chars, truncated: false };
        }
        case "paste_snippet":
          return { pasted: false };
        case "copy_snippet":
          return null;
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
            version: "0.5.0-dev",
            dataDir: "C:\\Users\\me\\AppData\\Roaming\\AnyPaste",
            logDir: "C:\\Users\\me\\AppData\\Roaming\\AnyPaste\\logs",
            repoUrl: "https://github.com/kun775/AnyPaste",
            portable: false,
          };
        case "get_update_status":
          return update;
        case "check_update":
          update = { ...update, checking: true, error: null };
          emitUpdate();
          await sleep(700);
          update = {
            ...update,
            checking: false,
            lastCheckedAt: Date.now(),
            available: {
              version: "0.5.1",
              notes: "- 常用片段支持搜索、预览与快捷粘贴\n- 更新按钮即时反馈\n- 下载进度更清晰",
              date: null,
            },
          };
          emitUpdate();
          return update.available;
        case "install_update":
          update = { ...update, installing: true, progress: null, error: null };
          emitUpdate();
          await sleep(600);
          for (const progress of [0.08, 0.26, 0.51, 0.73, 0.9, 1]) {
            update = { ...update, progress };
            emitUpdate();
            await sleep(350);
          }
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
