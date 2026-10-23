// 内联 SVG 图标，与原型保持一致；仅用于 {@html}，内容为常量。
const svg = (size: number, body: string, extra = 'fill="none" stroke="currentColor" stroke-width="1.4"') =>
  `<svg width="${size}" height="${size}" viewBox="0 0 16 16" ${extra} stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

export const icons = {
  search: svg(16, '<circle cx="7" cy="7" r="4.5"/><path d="M10.5 10.5L14 14"/>'),
  gear: svg(
    16,
    '<circle cx="8" cy="8" r="2.2"/><path d="M8 1.5v1.8M8 12.7v1.8M14.5 8h-1.8M3.3 8H1.5M12.6 3.4l-1.3 1.3M4.7 11.3l-1.3 1.3M12.6 12.6l-1.3-1.3M4.7 4.7L3.4 3.4"/>',
  ),
  text: svg(16, '<path d="M3 4h10M3 8h10M3 12h6"/>'),
  link: svg(
    16,
    '<path d="M7 9a3 3 0 0 0 4.2 0l2-2a3 3 0 0 0-4.2-4.2l-.8.8M9 7a3 3 0 0 0-4.2 0l-2 2a3 3 0 0 0 4.2 4.2l.8-.8"/>',
  ),
  rich: svg(16, '<path d="M4 3h5a2.5 2.5 0 0 1 0 5H4zM4 8h6a2.5 2.5 0 0 1 0 5H4z"/>'),
  image: svg(
    16,
    '<rect x="2" y="3" width="12" height="10" rx="1.5"/><circle cx="6" cy="6.5" r="1.2"/><path d="M2.5 12l3.5-3.5 2.5 2.5 2-2 3 3"/>',
  ),
  files: svg(16, '<path d="M2 4.5A1.5 1.5 0 0 1 3.5 3H6l1.5 1.5h5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z"/>'),
  star: svg(14, '<path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z"/>'),
  starFill: svg(14, '<path d="M8 1.8l1.9 3.9 4.3.6-3.1 3 .7 4.3L8 11.6l-3.8 2 .7-4.3-3.1-3 4.3-.6z"/>', 'fill="currentColor"'),
  trash: svg(14, '<path d="M2.5 4h11M6 4V2.5h4V4M4 4l.7 9.5h6.6L12 4M6.8 6.5v4.5M9.2 6.5v4.5"/>'),
  paste: svg(14, '<rect x="3" y="3" width="10" height="11.5" rx="1.5"/><path d="M6 3V2h4v1"/>'),
  plain: svg(14, '<path d="M3 3.5h10M8 3.5v9"/>'),
  copy: svg(
    14,
    '<rect x="5" y="5" width="8.5" height="8.5" rx="1.5"/><path d="M11 5V3.5A1.5 1.5 0 0 0 9.5 2h-6A1.5 1.5 0 0 0 2 3.5v6A1.5 1.5 0 0 0 3.5 11H5"/>',
  ),
  clock: svg(14, '<circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5"/>'),
  pin: svg(14, '<path d="M10 2.5l3.5 3.5-2.5 1-2 2 .5 3-1 1-2.5-2.5L3 13.5M6 8.5L3.5 6l1-1 3 .5 2-2z"/>'),
  pinFill: svg(
    14,
    '<path d="M10 2.5l3.5 3.5-2.5 1-2 2 .5 3-1 1-2.5-2.5-2.5-2.5 1-1 3 .5 2-2z" fill="currentColor"/><path d="M6 10L3 13"/>',
  ),
  eye: svg(14, '<path d="M1.5 8S4 3.5 8 3.5 14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/>'),
  grip: svg(16, '<path d="M5 6.5h6M5 9.5h6"/>'),
};
