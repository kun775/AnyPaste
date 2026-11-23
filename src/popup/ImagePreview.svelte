<script lang="ts">
  import { tick } from "svelte";
  import { clipImage, setPopupPreview, type ClipItem } from "../lib/api";
  import { fileNames, formatSize } from "../lib/format";

  interface Props {
    item: ClipItem;
    fullscreen: boolean;
    onfullscreen: (next: boolean) => void;
    onclose: () => void;
    onpaste: () => void;
  }

  /** 按下后移动超过该距离视为拖动，松开时不切换缩放。 */
  const DRAG_THRESHOLD = 4;

  function parseDimensions(preview: string): { w: number; h: number } | null {
    const m = /^(\d+)[×x](\d+)$/.exec(preview.trim());
    if (!m) return null;
    const w = Number(m[1]);
    const h = Number(m[2]);
    return w > 0 && h > 0 ? { w, h } : null;
  }

  let { item, fullscreen, onfullscreen, onclose, onpaste }: Props = $props();
  let src = $state<string | null>(null);
  let error = $state("");
  let actual = $state(false);
  let natural = $state<{ w: number; h: number } | null>(null);
  let stage: HTMLDivElement | undefined = $state();
  let stageW = $state(0);
  let stageH = $state(0);
  let drag: { x: number; y: number; left: number; top: number; moved: boolean } | null = null;
  let lastApplied = "";

  // 先显示缩略图，原图加载完成后替换
  $effect(() => {
    const id = item.id;
    src = null;
    error = "";
    actual = false;
    natural = null;
    let cancelled = false;
    clipImage(id)
      .then((url) => {
        if (!cancelled && item.id === id) src = url;
      })
      .catch((e) => {
        if (!cancelled) error = String(e);
      });
    return () => {
      cancelled = true;
    };
  });

  // 图片条目可立即从 `W×H` 预估尺寸展开预览框，减少闪烁；图片文件在原图加载后按真实像素展开
  const targetSize = $derived(natural ?? (item.kind === "image" ? parseDimensions(item.preview) : null));

  $effect(() => {
    if (fullscreen) {
      if (lastApplied !== "fullscreen") {
        lastApplied = "fullscreen";
        setPopupPreview({ mode: "fullscreen" }).catch(() => {});
      }
      return;
    }
    if (!targetSize) return;
    const key = `fit:${targetSize.w}x${targetSize.h}`;
    if (lastApplied !== key) {
      lastApplied = key;
      setPopupPreview({ mode: "fit", width: targetSize.w, height: targetSize.h }).catch(() => {});
    }
  });

  // 切换全屏时重置缩放，确保第一时间看到完整画面
  $effect(() => {
    void fullscreen;
    actual = false;
  });

  // 图片像素按物理像素 1:1 显示，换算为 CSS 像素时要除以缩放比例
  const cssSize = $derived(
    natural ? { w: natural.w / devicePixelRatio, h: natural.h / devicePixelRatio } : null,
  );
  const stagePad = $derived(fullscreen ? 40 : 24);
  const fitsActual = $derived(
    cssSize !== null &&
      stageW > 0 &&
      stageH > 0 &&
      cssSize.w <= stageW - stagePad + 2 &&
      cssSize.h <= stageH - stagePad + 2,
  );
  const canZoom = $derived(src !== null && natural !== null && (!fitsActual || actual));

  const infoText = $derived.by(() => {
    const base =
      item.kind === "files"
        ? [fileNames(item.preview)[0], natural ? `${natural.w}×${natural.h}` : ""].filter(Boolean).join(" · ")
        : `${item.preview} · ${formatSize(item.sizeBytes)}`;
    return error ? `${base} · ${error}` : base;
  });

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

  /** 未拖动的单击：超大图点图片切换缩放；点空白处退出全屏或关闭预览。 */
  function onPointerUp(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d || d.moved || e.button !== 0) return;
    if ((e.target as HTMLElement).tagName === "IMG") {
      if (canZoom) toggle(e.clientX, e.clientY);
    } else if (fullscreen) {
      onfullscreen(false);
    } else {
      onclose();
    }
  }
</script>

<!-- 按键（Esc/空格关闭、F/F11 全屏、Enter 粘贴）由弹窗的全局键盘处理负责 -->
<div class="preview" class:fullscreen role="dialog" aria-modal="true" aria-label="图片预览" tabindex="-1">
  <div
    class="stage"
    class:actual
    bind:this={stage}
    bind:clientWidth={stageW}
    bind:clientHeight={stageH}
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
    <span class="info" data-tauri-drag-region={fullscreen ? undefined : ""} title={fullscreen ? undefined : "拖动以移动预览窗口"}>
      {infoText}
    </span>
    {#if canZoom}
      <button class="btn" onclick={() => toggle()}>{actual ? "适应窗口" : "原始尺寸"}</button>
    {/if}
    <button class="btn" title="全屏预览（F）" onclick={() => onfullscreen(!fullscreen)}>
      {fullscreen ? "退出全屏" : "全屏"}
    </button>
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
  .stage {
    flex: 1;
    min-height: 0;
    display: flex;
    padding: 12px;
    overflow: hidden;
    background: repeating-conic-gradient(var(--line) 0 25%, transparent 0 50%) 0 0 / 16px 16px;
  }
  .preview.fullscreen .stage {
    padding: 20px;
    background: #18191c repeating-conic-gradient(rgba(255, 255, 255, 0.05) 0 25%, transparent 0 50%) 0 0 / 16px 16px;
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
  .stage:not(.actual) img:not(.loading) {
    width: auto;
    height: auto;
  }
  .stage:not(.actual) img.zoomable {
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
  .bar .info {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preview:not(.fullscreen) .bar .info {
    cursor: grab;
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
  .btn:disabled {
    opacity: 0.5;
  }
</style>
