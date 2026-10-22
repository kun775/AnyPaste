<script lang="ts" module>
  export interface MenuEntry {
    label: string;
    icon: string;
    shortcut?: string;
    danger?: boolean;
    run: () => void;
  }
</script>

<script lang="ts">
  interface Props {
    x: number;
    y: number;
    entries: MenuEntry[];
    onclose: () => void;
  }

  let { x, y, entries, onclose }: Props = $props();
  let el: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  // 根据菜单实际尺寸把位置限制在窗口内
  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    pos = {
      left: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    };
  });
</script>

<div class="layer" role="presentation" onclick={onclose} oncontextmenu={(e) => (e.preventDefault(), onclose())}>
  <div class="menu" role="menu" tabindex="-1" bind:this={el} style="left: {pos.left}px; top: {pos.top}px">
    {#each entries as entry (entry.label)}
      <button
        class="mi"
        class:danger={entry.danger}
        role="menuitem"
        onclick={(e) => {
          e.stopPropagation();
          onclose();
          entry.run();
        }}
      >
        <span class="ic">{@html entry.icon}</span>
        {entry.label}
        {#if entry.shortcut}<span class="sc">{entry.shortcut}</span>{/if}
      </button>
    {/each}
  </div>
</div>

<style>
  .layer {
    position: fixed;
    inset: 0;
    z-index: 20;
  }
  .menu {
    position: fixed;
    min-width: 196px;
    padding: 4px;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.2);
  }
  .mi {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    font-size: 13px;
    text-align: left;
  }
  .mi:hover {
    background: var(--card-hover);
  }
  .ic {
    width: 16px;
    display: grid;
    place-items: center;
    color: var(--ink-2);
  }
  .sc {
    margin-left: auto;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .danger,
  .danger .ic {
    color: var(--danger);
  }
</style>
