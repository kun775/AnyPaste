<script lang="ts">
  import { clipText, type ClipItem, type ClipText } from "../lib/api";
  import { ago, highlight } from "../lib/format";

  interface Props {
    item: ClipItem;
    query: string;
    now: number;
    onclose: () => void;
    onpaste: () => void;
  }

  /** 超过该长度不做搜索高亮，避免生成过多节点。 */
  const HIGHLIGHT_MAX = 50_000;

  let { item, query, now, onclose, onpaste }: Props = $props();
  let full = $state<ClipText | null>(null);
  let error = $state("");

  // 先显示列表里的预览文字，全文加载完成后替换
  $effect(() => {
    const id = item.id;
    full = null;
    error = "";
    clipText(id)
      .then((t) => {
        if (item.id === id) full = t;
      })
      .catch((e) => (error = String(e)));
  });

  const text = $derived(full?.text ?? item.preview);
  const segments = $derived(text.length <= HIGHLIGHT_MAX ? highlight(text, query) : [{ text, hit: false }]);
  const stats = $derived.by(() => {
    if (!full) return "正在加载…";
    if (item.kind === "files") return `${full.text.split("\n").filter(Boolean).length} 个文件`;
    const lines = full.text.split("\n").length;
    const base = `${full.chars.toLocaleString()} 字符 · ${lines.toLocaleString()} 行`;
    return full.truncated ? `${base}（仅显示开头部分）` : base;
  });
</script>

<!-- 按键（Esc/空格关闭、Enter 粘贴）由弹窗的全局键盘处理负责 -->
<div class="preview" role="dialog" aria-modal="true" aria-label="内容预览" tabindex="-1">
  <div class="head">
    <span class="title">{item.kind === "files" ? "文件列表" : item.rich ? "富文本（纯文本视图）" : "全文"}</span>
    <span class="sub">{[item.sourceApp, ago(item.lastCopiedAt, now)].filter(Boolean).join(" · ")}</span>
  </div>
  <!-- prettier-ignore -->
  <pre class="content" class:mono={item.kind === "files"}>{#each segments as seg}{#if seg.hit}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}</pre>
  <div class="bar">
    <span class:error={!!error}>{error || stats}</span>
    <button class="btn primary" onclick={onpaste}>粘贴</button>
    <button class="btn" onclick={onclose}>关闭</button>
  </div>
</div>

<style>
  .preview {
    position: absolute;
    inset: 0;
    z-index: 15;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    outline: none;
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 12px 14px 8px;
  }
  .title {
    font-size: 13px;
    font-weight: 600;
  }
  .sub {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .content {
    flex: 1;
    min-height: 0;
    margin: 0 10px 10px;
    padding: 10px 12px;
    overflow: auto;
    scrollbar-width: thin;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--card);
    font-family: var(--font);
    font-size: 13px;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    tab-size: 4;
    user-select: text;
    cursor: text;
  }
  .content.mono {
    font-family: var(--mono);
    font-size: 12px;
  }
  mark {
    padding: 0 1px;
    border-radius: 2px;
    background: var(--mark);
    color: inherit;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--ink-2);
  }
  .bar span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar .error {
    color: var(--danger);
  }
  .btn {
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--card);
    font-size: 12.5px;
    white-space: nowrap;
  }
  .btn:hover {
    background: var(--card-hover);
  }
  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .btn.primary:hover {
    background: var(--accent);
    filter: brightness(1.08);
  }
</style>
