<script lang="ts">
  import { hidePopup, isPaused, openSettings } from "../lib/api";

  type Tab = "temp" | "perm";

  let tab = $state<Tab>("temp");
  let query = $state("");
  let paused = $state(false);
  let searchEl: HTMLInputElement | undefined = $state();

  // 弹窗常驻隐藏，每次显示时刷新状态
  async function onFocus() {
    searchEl?.focus();
    paused = await isPaused().catch(() => false);
  }

  // TODO(M2): 接入真实数据
  const counts = { temp: 0, perm: 0 };

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      if (query) query = "";
      else hidePopup();
    } else if (e.key === "Tab") {
      e.preventDefault();
      tab = tab === "temp" ? "perm" : "temp";
    }
  }
</script>

<svelte:window onkeydown={onKeydown} onfocus={onFocus} />

<main class="popup">
  <header class="head">
    <label class="search">
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4">
        <circle cx="7" cy="7" r="4.5" /><path d="M10.5 10.5L14 14" stroke-linecap="round" />
      </svg>
      <input bind:this={searchEl} bind:value={query} placeholder="搜索剪贴板历史" autocomplete="off" />
    </label>
    <button class="icon-btn" title="设置" onclick={openSettings}>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
        <circle cx="8" cy="8" r="2.2" />
        <path d="M8 1.5v1.8M8 12.7v1.8M14.5 8h-1.8M3.3 8H1.5M12.6 3.4l-1.3 1.3M4.7 11.3l-1.3 1.3M12.6 12.6l-1.3-1.3M4.7 4.7L3.4 3.4" />
      </svg>
    </button>
  </header>

  <div class="tabs" role="tablist">
    <button class="tab" class:active={tab === "temp"} role="tab" aria-selected={tab === "temp"} onclick={() => (tab = "temp")}>
      临时 <span class="count">{counts.temp}</span>
    </button>
    <button class="tab" class:active={tab === "perm"} role="tab" aria-selected={tab === "perm"} onclick={() => (tab = "perm")}>
      永久 <span class="count">{counts.perm}</span>
    </button>
  </div>

  <section class="list">
    <div class="empty">
      {#if query.trim()}
        <h3>没有匹配的记录</h3>
        <p>没有找到包含“{query.trim()}”的{tab === "perm" ? "永久" : "临时"}条目</p>
      {:else if tab === "perm"}
        <h3>还没有永久条目</h3>
        <p>用 <kbd>Ctrl+Shift+C</kbd> 复制，或在临时列表中<br />点击 ☆ 把条目设为永久</p>
      {:else if paused}
        <h3>记录已暂停</h3>
        <p>可在托盘菜单中恢复记录</p>
      {:else}
        <h3>暂无临时记录</h3>
        <p>复制任意内容后会出现在这里，<br />24 小时后自动删除</p>
      {/if}
    </div>
  </section>

  <footer class="foot">
    <span><kbd>↑↓</kbd> 选择</span>
    <span><kbd>Enter</kbd> 粘贴</span>
    <span><kbd>Tab</kbd> 切换</span>
    <span><kbd>Esc</kbd> 关闭</span>
  </footer>
</main>

<style>
  .popup {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .head {
    display: flex;
    gap: 6px;
    padding: 10px 10px 6px;
  }
  .search {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    background: var(--card);
    border: 1px solid var(--line);
    border-bottom-color: var(--line-strong);
    border-radius: 6px;
    color: var(--ink-3);
  }
  .search:focus-within {
    border-bottom: 2px solid var(--accent);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    font-size: 13.5px;
    color: var(--ink);
  }
  .icon-btn {
    width: 34px;
    height: 34px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    display: grid;
    place-items: center;
    color: var(--ink-2);
  }
  .icon-btn:hover {
    background: var(--card-hover);
    color: var(--ink);
  }
  .tabs {
    display: flex;
    gap: 4px;
    padding: 2px 10px 0;
    border-bottom: 1px solid var(--line);
  }
  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px 10px;
    border: 0;
    background: transparent;
    font-size: 13px;
    color: var(--ink-2);
  }
  .tab.active {
    color: var(--ink);
    font-weight: 600;
  }
  .tab.active::after {
    content: "";
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: -1px;
    height: 3px;
    border-radius: 3px;
    background: var(--accent);
  }
  .count {
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 9px;
    display: inline-grid;
    place-items: center;
    font-size: 11px;
    font-weight: 500;
    background: var(--line);
    color: var(--ink-2);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 6px;
  }
  .empty {
    height: 100%;
    display: grid;
    place-content: center;
    text-align: center;
    color: var(--ink-2);
    padding: 24px;
  }
  .empty h3 {
    margin: 0 0 6px;
    font-size: 14px;
    font-weight: 600;
    color: var(--ink);
  }
  .empty p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.6;
  }
  .foot {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--ink-3);
  }
</style>
