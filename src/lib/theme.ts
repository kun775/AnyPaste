import type { Theme } from "./api";

/** 跟随系统时移除 data-theme，由 CSS 媒体查询决定；否则强制指定。 */
export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}
