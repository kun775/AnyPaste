<script lang="ts">
  import type { SnippetItem } from "../lib/api";
  import { ago, excerpt, highlight } from "../lib/format";
  import { icons } from "../lib/icons";

  interface Props {
    item: SnippetItem;
    index: number;
    selected: boolean;
    now: number;
    /** 当前搜索词，用于高亮命中部分。 */
    query: string;
    onselect: () => void;
    onpaste: () => void;
    onpreview: () => void;
    oncopy: () => void;
    ondelete: () => void;
    onmenu: (x: number, y: number) => void;
  }

  let { item, index, selected, now, query, onselect, onpaste, onpreview, oncopy, ondelete, onmenu }: Props = $props();

  const shown = $derived(excerpt(item.preview, query));
  const titleSegments = $derived(highlight(item.title, query));
  const segments = $derived(highlight(shown, query));

  function stop(fn: () => void) {
    return (e: MouseEvent) => {
      e.stopPropagation();
      fn();
    };
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<li
  class="item"
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
  <div class="kind">{@html icons.snippet}</div>
  <div class="body">
    <!-- prettier-ignore -->
    <div class="title">{#each titleSegments as seg}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</div>
    <!-- prettier-ignore -->
    <div class="preview">{#each segments as seg}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</div>
    <div class="meta">
      <span class="badge">常用片段</span>
      <span>{item.chars.toLocaleString()} 字符</span>
      <span class="dot"></span>
      <span>更新于 {ago(item.updatedAt, now)}</span>
    </div>
  </div>
  {#if index < 9}<span class="num">{index + 1}</span>{/if}
  <div class="actions">
    <button class="act" title="查看全文（空格）" onclick={stop(onpreview)}>{@html icons.eye}</button>
    <button class="act" title="仅复制" onclick={stop(oncopy)}>{@html icons.copy}</button>
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
    contain-intrinsic-size: auto 76px;
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
    background: var(--snippet-soft);
    color: var(--snippet);
  }
  .body {
    min-width: 0;
    padding-right: 14px;
  }
  .title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
    font-weight: 600;
    line-height: 1.4;
  }
  .preview {
    margin-top: 2px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--ink-2);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    word-break: break-all;
  }
  mark {
    padding: 0 1px;
    border-radius: 2px;
    background: var(--mark);
    color: inherit;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin-top: 5px;
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
    background: var(--snippet-soft);
    color: var(--snippet);
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
  .act.del:hover {
    color: var(--danger);
  }
</style>
