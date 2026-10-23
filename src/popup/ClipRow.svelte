<script lang="ts">
  import type { ClipItem } from "../lib/api";
  import { ago, fileNames, formatSize, isUrl, remaining } from "../lib/format";
  import { icons } from "../lib/icons";

  interface Props {
    item: ClipItem;
    index: number;
    selected: boolean;
    now: number;
    onselect: () => void;
    onpaste: () => void;
    ontoggle: () => void;
    onpin: () => void;
    onpreview: () => void;
    ondelete: () => void;
    onmenu: (x: number, y: number) => void;
  }

  let { item, index, selected, now, onselect, onpaste, ontoggle, onpin, onpreview, ondelete, onmenu }: Props =
    $props();

  const link = $derived(item.kind === "text" && isUrl(item.preview));
  const icon = $derived(link ? icons.link : icons[item.kind]);
  const files = $derived(item.kind === "files" ? fileNames(item.preview) : []);
  // 剩余比例用于过期进度条；临时条目的总时长为复制时间到过期时间
  const ttl = $derived.by(() => {
    if (item.permanent || item.expiresAt === null) return null;
    const total = Math.max(1, item.expiresAt - item.lastCopiedAt);
    const ratio = Math.min(1, Math.max(0, (item.expiresAt - now) / total));
    return { ratio, label: remaining(item.expiresAt, now), soon: ratio < 0.1 };
  });

  function stop(fn: () => void) {
    return (e: MouseEvent) => {
      e.stopPropagation();
      fn();
    };
  }
</script>

<!-- 单击即粘贴；鼠标移动（而非进入）时才更新选中项，避免键盘滚动列表时被光标下的条目抢走选中 -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<li
  class="item"
  class:perm={item.permanent}
  class:pinned={item.pinned}
  class:sel={selected}
  role="option"
  aria-selected={selected}
  title="单击粘贴"
  onclick={onpaste}
  onmousemove={() => {
    if (!selected) onselect();
  }}
  oncontextmenu={(e) => {
    e.preventDefault();
    onselect();
    onmenu(e.clientX, e.clientY);
  }}
>
  <div class="kind">{@html icon}</div>
  <div class="body">
    {#if item.kind === "image" && item.thumb}
      <img class="thumb" src={item.thumb} alt="图片 {item.preview}" />
    {:else if item.kind === "files"}
      {#if item.thumb}<img class="thumb" src={item.thumb} alt="图片文件 {files[0]}" />{/if}
      <div class="preview">{files.length} 个文件：{files.join("、")}</div>
    {:else}
      <div class="preview" class:link>{item.preview}</div>
    {/if}
    <div class="meta">
      {#if item.pinned}<span class="pin-mark" title="已置顶">{@html icons.pinFill}</span>{/if}
      {#if item.rich}<span class="badge">富文本</span>{/if}
      {#if item.sourceApp}<span>{item.sourceApp}</span><span class="dot"></span>{/if}
      <span>{ago(item.lastCopiedAt, now)}</span>
      {#if item.kind === "image"}
        <span class="dot"></span><span>{item.preview} · {formatSize(item.sizeBytes)}</span>
      {/if}
      {#if ttl}
        <span class="dot"></span>
        <span class="ttl" class:soon={ttl.soon} title="临时条目到期后自动删除">
          <span class="bar"><i style="width: {ttl.ratio * 100}%"></i></span>{ttl.label}
        </span>
      {/if}
    </div>
  </div>
  {#if index < 9}<span class="num">{index + 1}</span>{/if}
  <div class="actions">
    {#if item.thumb}
      <button class="act" title="预览原图" onclick={stop(onpreview)}>{@html icons.eye}</button>
    {/if}
    <button class="act pin" class:on={item.pinned} title={item.pinned ? "取消置顶" : "置顶"} onclick={stop(onpin)}>
      {@html item.pinned ? icons.pinFill : icons.pin}
    </button>
    <button
      class="act star"
      class:on={item.permanent}
      title={item.permanent ? "转为临时" : "设为永久"}
      onclick={stop(ontoggle)}
    >
      {@html item.permanent ? icons.starFill : icons.star}
    </button>
    <button class="act del" title="删除" onclick={stop(ondelete)}>{@html icons.trash}</button>
  </div>
</li>

<style>
  .item {
    position: relative;
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 10px;
    padding: 9px 10px;
    margin-bottom: 4px;
    border-radius: 6px;
    background: var(--card);
    border: 1px solid var(--line);
    content-visibility: auto;
    contain-intrinsic-size: auto 64px;
  }
  .item:hover {
    background: var(--card-hover);
  }
  .item.sel {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .kind {
    width: 28px;
    height: 28px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    background: var(--accent-soft);
    color: var(--accent);
  }
  .perm .kind {
    background: var(--perm-soft);
    color: var(--perm);
  }
  .body {
    min-width: 0;
    padding-right: 14px;
  }
  .preview {
    font-size: 13px;
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
    white-space: pre-line;
  }
  .preview.link {
    color: var(--accent);
  }
  .thumb {
    display: block;
    max-width: 100%;
    max-height: 140px;
    min-width: 40px;
    min-height: 40px;
    object-fit: contain;
    border-radius: 4px;
    border: 1px solid var(--line);
    background: repeating-conic-gradient(var(--line) 0 25%, transparent 0 50%) 0 0 / 12px 12px;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .dot {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--ink-3);
  }
  .badge {
    font-size: 10.5px;
    line-height: 16px;
    padding: 0 5px;
    border-radius: 3px;
    background: var(--line);
    color: var(--ink-2);
  }
  .ttl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .ttl.soon {
    color: var(--danger);
  }
  .bar {
    width: 28px;
    height: 3px;
    border-radius: 2px;
    background: var(--line-strong);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--ink-3);
  }
  .ttl.soon .bar i {
    background: var(--danger);
  }
  .num {
    position: absolute;
    right: 8px;
    top: 8px;
    font-size: 11px;
    color: var(--ink-3);
    font-variant-numeric: tabular-nums;
  }
  .actions {
    position: absolute;
    right: 6px;
    top: 6px;
    display: none;
    gap: 2px;
    padding: 2px;
    border-radius: 6px;
    background: var(--card-hover);
  }
  .item:hover .actions,
  .item.sel .actions {
    display: flex;
  }
  .item:hover .num,
  .item.sel .num {
    display: none;
  }
  .act {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    display: grid;
    place-items: center;
    color: var(--ink-2);
  }
  .act:hover {
    background: var(--card);
    color: var(--ink);
  }
  .act.star:hover,
  .act.star.on {
    color: var(--perm);
  }
  .act.del:hover {
    color: var(--danger);
  }
  .act.pin:hover,
  .act.pin.on {
    color: var(--accent);
  }
  .pin-mark {
    display: inline-grid;
    place-items: center;
    color: var(--accent);
  }
  .item.pinned:not(.sel) {
    border-color: var(--accent-soft);
  }
</style>
