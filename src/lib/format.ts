const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** 相对时间，如“5 分钟前”。 */
export function ago(t: number, now: number): string {
  const d = now - t;
  if (d < MINUTE) return "刚刚";
  if (d < HOUR) return `${Math.floor(d / MINUTE)} 分钟前`;
  if (d < DAY) return `${Math.floor(d / HOUR)} 小时前`;
  const days = Math.floor(d / DAY);
  return days === 1 ? "昨天" : `${days} 天前`;
}

/** 临时条目剩余时间，如“剩 3 小时”。 */
export function remaining(expiresAt: number, now: number): string {
  const r = expiresAt - now;
  if (r <= 0) return "即将删除";
  if (r < HOUR) return `剩 ${Math.max(1, Math.floor(r / MINUTE))} 分钟`;
  if (r < DAY) return `剩 ${Math.floor(r / HOUR)} 小时`;
  return `剩 ${Math.floor(r / DAY)} 天`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function isUrl(text: string): boolean {
  return /^https?:\/\/\S+$/i.test(text.trim());
}

export interface Segment {
  text: string;
  hit: boolean;
}

/** 把文本按搜索词（不区分大小写）切分成片段，用于高亮命中部分。 */
export function highlight(text: string, query: string): Segment[] {
  const q = query.trim();
  if (!q || !text) return [{ text, hit: false }];
  const re = new RegExp(q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi");
  const out: Segment[] = [];
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index > last) out.push({ text: text.slice(last, m.index), hit: false });
    out.push({ text: m[0], hit: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last), hit: false });
  return out;
}

/** 命中位置靠后时从命中处附近截取，保证两行预览内能看到搜索词。 */
export function excerpt(text: string, query: string, lead = 24, threshold = 48): string {
  const q = query.trim().toLowerCase();
  if (!q) return text;
  const at = text.toLowerCase().indexOf(q);
  if (at < threshold) return text;
  return "…" + text.slice(at - lead).trimStart();
}

/** 文本是单个 CSS 颜色值（十六进制或 rgb/hsl 函数）时返回该颜色，用于显示色块。 */
export function parseColor(text: string): string | null {
  const t = text.trim();
  if (t.length > 48) return null;
  const hex = /^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(t);
  const fn = /^(?:rgb|hsl)a?\([^()]+\)$/i.test(t);
  if (!hex && !fn) return null;
  return typeof CSS !== "undefined" && CSS.supports("color", t) ? t : null;
}

export function fileNames(text: string): string[] {
  return text
    .split("\n")
    .filter(Boolean)
    .map((p) => p.split(/[\\/]/).pop() || p);
}
