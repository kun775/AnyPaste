// 热键在后端以 global-shortcut 写法保存，如 `ctrl+shift+c`；界面显示为 `Ctrl + Shift + C`。

const MODIFIER_LABELS: Record<string, string> = {
  ctrl: "Ctrl",
  control: "Ctrl",
  alt: "Alt",
  shift: "Shift",
  super: "Win",
  win: "Win",
};

const KEY_LABELS: Record<string, string> = {
  space: "Space",
  backquote: "`",
  minus: "-",
  equal: "=",
  bracketleft: "[",
  bracketright: "]",
  backslash: "\\",
  semicolon: ";",
  quote: "'",
  comma: ",",
  period: ".",
  slash: "/",
  arrowup: "↑",
  arrowdown: "↓",
  arrowleft: "←",
  arrowright: "→",
};

export function formatHotkey(accel: string): string {
  return accel
    .split("+")
    .filter(Boolean)
    .map((p) => MODIFIER_LABELS[p] ?? KEY_LABELS[p] ?? (p.length === 1 ? p.toUpperCase() : p[0].toUpperCase() + p.slice(1)))
    .join(" + ");
}

const MODIFIER_CODES = new Set(["ControlLeft", "ControlRight", "AltLeft", "AltRight", "ShiftLeft", "ShiftRight", "MetaLeft", "MetaRight"]);

/**
 * 把按键事件转为热键写法。只按下修饰键时返回 null（继续等待）；
 * 没有修饰键时返回 "" 表示无效。使用 `code` 而非 `key`，与键盘布局和输入法无关。
 */
export function hotkeyFromEvent(e: KeyboardEvent): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("ctrl");
  if (e.altKey) mods.push("alt");
  if (e.shiftKey) mods.push("shift");
  if (e.metaKey) mods.push("super");
  if (mods.length === 0) return "";
  let key = e.code;
  if (/^Key[A-Z]$/.test(key)) key = key.slice(3);
  else if (/^Digit\d$/.test(key)) key = key.slice(5);
  return [...mods, key.toLowerCase()].join("+");
}
