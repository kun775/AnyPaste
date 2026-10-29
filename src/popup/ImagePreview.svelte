<script lang="ts">
  import { tick } from "svelte";
  import { clipImage, type ClipItem } from "../lib/api";
  import { fileNames, formatSize } from "../lib/format";

  interface Props {
    item: ClipItem;
    onclose: () => void;
    onpaste: () => void;
  }

  /** 按下后移动超过该距离视为拖动，松开时不切换缩放。 */
  const DRAG_THRESHOLD = 4;

  let { item, onclose, onpaste }: Props = $props();
  let src = $state<string | null>(null);
  let error = $state("");
  let actual = $state(false);
  let natural = $state<{ w: number; h: number } | null>(null);
  let stage: HTMLDivElement | undefined = $state();
  let drag: { x: number; y: number; left: number; top: number; moved: boolean } | null = null;

  // 先显示缩略图，原图加载完成后替换
  $effect(() => {
    const id = item.id;
    src = null;
    error = "";
    actual = false;
    natural = null;
    clipImage(id)
      .then((url) => {
        if (item.id === id) src = url;
      })
      .catch((e) => (error = String(e)));
  });

  // 图片像素按物理像素 1:1 显示，换算为 CSS 像素时要除以缩放比例
  const cssSize = $derived(
    natural ? { w: natural.w / devicePixelRatio, h: natural.h / devicePixelRatio } : null,
  );
  const canZoom = $derived(src !== null && natural !== null);

  function onLoad(e: Event) {
    if (!src) return;
    const img = e.currentTarget as HTMLImageElement;
    natural = { w: img.naturalWidth, h: img.naturalHeight };
  }

  /** 切换缩放；切到原始尺寸时让点击位置保持在原处。 */
  async function toggle(clientX?: number, clientY?: number) {
    const img = stage?.querySelector("img");
    if (!canZoom || !stage || !img) return;
    const before = img.getBoundingClientRect();
    actual = !actual;
    if (!actual) return;
    const rect = stage.getBoundingClientRect();
    const x = clientX ?? rect.left + rect.width / 2;
    const y = clientY ?? rect.top + rect.height / 2;
    const fx = (x - before.left) / before.width;
    const fy = (y - before.top) / before.height;
    // 等 DOM 更新为原始尺寸后，把同一相对位置滚动到指针下方
    await tick();
    const after = img.getBoundingClientRect();
    stage.scrollLeft += after.left + fx * after.width - x;
    stage.scrollTop += after.top + fy * after.height - y;
  }

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0 || !stage) return;
    drag = { x: e.clientX, y: e.clientY, left: stage.scrollLeft, top: stage.scrollTop, moved: false };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag || !stage) return;
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    if (!drag.moved) {
      drag.moved = true;
      stage.setPointerCapture(e.pointerId);
    }
    stage.scrollLeft = drag.left - dx;
    stage.scrollTop = drag.top - dy;
  }

  /** 未拖动的单击：点图片切换缩放，点空白处关闭。 */
  function onPointerUp(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d || d.moved || e.button !== 0) return;
    if ((e.target as HTMLElement).tagName === "IMG") toggle(e.clientX, e.clientY);
    else onclose();
  }
</script>

<!-- 按键（Esc 关闭、Enter 粘贴）由弹窗的全局键盘处理负责 -->
<div class="preview" role="dialog" aria-modal="true" aria-label="图片预览" tabindex="-1">
  <div
    class="stage"
    class:actual
    bind:this={stage}
    role="presentation"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={() => (drag = null)}
  >
    <img
      src={src ?? item.thumb ?? ""}
      alt="图片 {item.preview}"
      class:loading={!src}
      class:zoomable={canZoom}
      style:width={actual && cssSize ? `${cssSize.w}px` : null}
      style:height={actual && cssSize ? `${cssSize.h}px` : null}
      style:max-width={!actual && cssSize ? `min(100%, ${cssSize.w}px)` : null}
      style:max-height={!actual && cssSize ? `min(100%, ${cssSize.h}px)` : null}
      draggable="false"
      onload={onLoad}
    />
  </div>
  <div class="bar">
    <span>
      {item.kind === "files" ? fileNames(item.preview)[0] : `${item.preview} · ${formatSize(item.sizeBytes)}`}{error
        ? ` · ${error}`
        : ""}
    </span>
    <button class="btn" disabled={!canZoom} onclick={() => toggle()}>{actual ? "适应窗口" : "原始尺寸"}</button>
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
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    padding: 12px;
    overflow: hidden;
    background: repeating-conic-gradient(var(--line) 0 25%, transparent 0 50%) 0 0 / 16px 16px;
  }
  .stage.actual {
    overflow: auto;
    scrollbar-width: thin;
    cursor: grab;
  }
  .stage.actual:active {
    cursor: grabbing;
  }
  /* margin: auto 在内容溢出时不会像 align/justify-content: center 那样裁掉左上部分 */
  img {
    margin: auto;
    width: 100%;
    height: 100%;
    object-fit: contain;
    flex-shrink: 0;
  }
  .stage:not(.actual) img.zoomable {
    width: auto;
    height: auto;
    cursor: zoom-in;
  }
  .stage.actual img {
    max-width: none;
    max-height: none;
    cursor: zoom-out;
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
    white-space: nowrap;
  }
  .btn:disabled {
    opacity: 0.5;
  }
</style>
