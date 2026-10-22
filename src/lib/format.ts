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

export function fileNames(text: string): string[] {
  return text
    .split("\n")
    .filter(Boolean)
    .map((p) => p.split(/[\\/]/).pop() || p);
}
