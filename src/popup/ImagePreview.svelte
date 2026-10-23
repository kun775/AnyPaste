<script lang="ts">
  import { clipImage, type ClipItem } from "../lib/api";
  import { formatSize } from "../lib/format";

  interface Props {
    item: ClipItem;
    onclose: () => void;
    onpaste: () => void;
  }

  let { item, onclose, onpaste }: Props = $props();
  let src = $state<string | null>(null);
  let error = $state("");

  // 先显示缩略图，原图加载完成后替换
  $effect(() => {
    const id = item.id;
    src = null;
    error = "";
    clipImage(id)
      .then((url) => {
        if (item.id === id) src = url;
      })
      .catch((e) => (error = String(e)));
  });
</script>

<!-- 按键（Esc 关闭、Enter 粘贴）由弹窗的全局键盘处理负责 -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="preview" role="dialog" aria-modal="true" aria-label="图片预览" tabindex="-1" onclick={onclose}>
  <img src={src ?? item.thumb ?? ""} alt="图片 {item.preview}" class:loading={!src} />
  <div class="bar" role="presentation" onclick={(e) => e.stopPropagation()}>
    <span>{item.preview} · {formatSize(item.sizeBytes)}{error ? ` · ${error}` : ""}</span>
    <button class="btn" onclick={onpaste}>粘贴</button>
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
  img {
    flex: 1;
    min-height: 0;
    width: 100%;
    padding: 12px;
    object-fit: contain;
    background: repeating-conic-gradient(var(--line) 0 25%, transparent 0 50%) 0 0 / 16px 16px;
  }
  img.loading {
    filter: blur(2px);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
    background: var(--surface);
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
  .btn {
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--card);
    font-size: 12.5px;
  }
</style>
