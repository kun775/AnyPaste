// 开发用模拟后端：在普通浏览器中打开 `popup.html?mock` 即可调试界面，无需启动 Tauri。
// 仅在 `import.meta.env.DEV` 下被动态导入，不会进入生产包。
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { ClipItem, Cursor, Tab } from "./api";

const MIN = 60_000;
const HOUR = 60 * MIN;
const TTL = 24 * HOUR;

// 1×1 蓝色 PNG，作为图片缩略图占位
const THUMB =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

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

export function installMock() {
  let items = fixtures();
  const visible = (tab: Tab, query: string | null) =>
    items
      .filter((i) => i.permanent === (tab === "perm"))
      .filter((i) => !query || i.preview.toLowerCase().includes(query.toLowerCase()))
      .sort((a, b) => b.lastCopiedAt - a.lastCopiedAt || b.id - a.id);

  mockWindows("popup");
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      const id = args.id as number;
      console.debug("[mock]", cmd, args);
      switch (cmd) {
        case "list_clips": {
          const before = args.before as Cursor | null;
          let list = visible(args.tab as Tab, args.query as string | null);
          if (before) {
            list = list.filter(
              (i) => i.lastCopiedAt < before.lastCopiedAt || (i.lastCopiedAt === before.lastCopiedAt && i.id < before.id),
            );
          }
          return list.slice(0, 100);
        }
        case "clip_counts":
          return { temp: visible("temp", null).length, perm: visible("perm", null).length };
        case "is_paused":
          return false;
        case "paste_clip":
        case "copy_clip":
          items = items.map((i) => (i.id === id ? { ...i, lastCopiedAt: Date.now() } : i));
          return cmd === "paste_clip" ? { pasted: false } : null;
        case "set_permanent":
          items = items.map((i) =>
            i.id === id
              ? { ...i, permanent: args.permanent as boolean, expiresAt: args.permanent ? null : Date.now() + TTL }
              : i,
          );
          return null;
        case "delete_clip":
          items = items.filter((i) => i.id !== id);
          return null;
        case "clear_temp": {
          const n = items.filter((i) => !i.permanent).length;
          items = items.filter((i) => i.permanent);
          return n;
        }
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
