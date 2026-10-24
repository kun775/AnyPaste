<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    PAGE_SIZE,
    clearTemp,
    clipCounts,
    copyClip,
    deleteClip,
    getSettings,
    hidePopup,
    isPaused,
    listClips,
    onClipsChanged,
    onPopupShown,
    onSettingsChanged,
    openSettings,
    pasteClip,
    setPermanent,
    setPinned,
    type ClipItem,
    type Counts,
    type Settings,
    type Tab,
  } from "../lib/api";
  import ConfirmDialog from "../lib/ConfirmDialog.svelte";
  import { icons } from "../lib/icons";
  import { applyTheme } from "../lib/theme";
  import ClipRow from "./ClipRow.svelte";
  import ContextMenu, { type MenuEntry } from "./ContextMenu.svelte";
  import ImagePreview from "./ImagePreview.svelte";

  interface Confirm {
    title: string;
    message: string;
    action: string;
    run: () => Promise<void>;
  }

  let tab = $state<Tab>("temp");
  let query = $state("");
  let items = $state<ClipItem[]>([]);
  let counts = $state<Counts>({ temp: 0, perm: 0 });
  let sel = $state(0);
  let hasMore = $state(false);
  let loadingMore = $state(false);
  let loaded = $state(false);
  let loadError = $state("");
  let paused = $state(false);
  let now = $state(Date.now());
  let menu = $state<{ x: number; y: number; entries: MenuEntry[] } | null>(null);
  let confirm = $state<Confirm | null>(null);
  let toast = $state<{ text: string; error: boolean } | null>(null);
  let plainDefault = $state(false);
  let preview = $state<ClipItem | null>(null);

  let searchEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLUListElement | undefined = $state();
  let requestSeq = 0;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  /** 重新加载第一页。用请求序号丢弃过期响应，避免快速输入时结果错乱。 */
  async function reload() {
    const seq = ++requestSeq;
    try {
      const [list, c, p] = await Promise.all([listClips(tab, query, null), clipCounts(), isPaused()]);
      if (seq !== requestSeq) return;
      items = list;
      counts = c;
      paused = p;
      hasMore = list.filter((i) => !i.pinned).length === PAGE_SIZE;
      sel = Math.min(sel, Math.max(0, list.length - 1));
      loadError = "";
    } catch (e) {
      if (seq === requestSeq) loadError = String(e);
    } finally {
      if (seq === requestSeq) loaded = true;
    }
  }

  async function loadMore() {
    if (!hasMore || loadingMore || items.length === 0) return;
    const seq = requestSeq;
    // 游标取最后一条非置顶条目；置顶条目只随第一页返回
    const last = items.findLast((i) => !i.pinned);
    if (!last) return;
    loadingMore = true;
    try {
      const more = await listClips(tab, query, { lastCopiedAt: last.lastCopiedAt, id: last.id });
      if (seq !== requestSeq) return;
      items = [...items, ...more];
      hasMore = more.length === PAGE_SIZE;
    } catch (e) {
      showToast(String(e), true);
    } finally {
      loadingMore = false;
    }
  }

  function onScroll() {
    if (!listEl) return;
    if (listEl.scrollTop + listEl.clientHeight >= listEl.scrollHeight - 200) loadMore();
  }

  // 切换 Tab 或输入搜索词时重新加载；输入时稍作防抖
  $effect(() => {
    void tab;
    const q = query;
    const timer = setTimeout(reload, q ? 120 : 0);
    return () => clearTimeout(timer);
  });

  function applySettings(s: Settings) {
    plainDefault = s.plainPasteDefault;
    applyTheme(s.theme);
  }

  onMount(() => {
    const clock = setInterval(() => (now = Date.now()), 30_000);
    getSettings()
      .then((v) => applySettings(v.settings))
      .catch(() => {});
    const unlisten = [
      onClipsChanged(reload),
      onSettingsChanged(applySettings),
      onPopupShown(async () => {
        menu = null;
        confirm = null;
        preview = null;
        query = "";
        sel = 0;
        now = Date.now();
        listEl?.scrollTo({ top: 0 });
        await reload();
        searchEl?.focus();
      }),
    ];
    return () => {
      clearInterval(clock);
      unlisten.forEach((p) => p.then((off) => off()));
    };
  });

  function showToast(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 3000 : 1600);
  }

  /** 执行操作后主动刷新；后端也会发 clips-changed，重复请求由序号去重。 */
  async function run(action: () => Promise<unknown>, done?: string) {
    try {
      await action();
      if (done) showToast(done);
    } catch (e) {
      showToast(String(e), true);
    }
    await reload();
  }

  /** `alternate` 为 true（Shift+Enter）时使用与默认相反的格式。 */
  async function paste(item: ClipItem | undefined, alternate = false) {
    if (!item) return;
    try {
      const { pasted } = await pasteClip(item.id, plainDefault !== alternate);
      if (!pasted) {
        showToast("已复制到剪贴板");
        sel = 0;
        await reload();
      }
    } catch (e) {
      showToast(String(e), true);
    }
  }

  function toggle(item: ClipItem | undefined) {
    if (!item) return;
    const permanent = !item.permanent;
    run(() => setPermanent(item.id, permanent), permanent ? "已设为永久" : "已转为临时，将按保留时长过期");
  }

  function togglePin(item: ClipItem | undefined) {
    if (!item) return;
    const pinned = !item.pinned;
    run(() => setPinned(item.id, pinned), pinned ? "已置顶" : "已取消置顶");
    if (pinned) {
      sel = 0;
      listEl?.scrollTo({ top: 0 });
    }
  }

  function remove(item: ClipItem | undefined) {
    if (!item) return;
    if (item.permanent) {
      confirm = {
        title: "删除永久条目？",
        message: "该条目将被永久删除，此操作无法撤销。",
        action: "删除",
        run: () => run(() => deleteClip(item.id), "已删除"),
      };
    } else {
      run(() => deleteClip(item.id), "已删除");
    }
  }

  function askClearTemp() {
    if (counts.temp === 0) return;
    confirm = {
      title: "清空临时记录？",
      message: `将删除全部 ${counts.temp} 条临时条目，永久条目不受影响。此操作无法撤销。`,
      action: "清空",
      run: () => run(clearTemp, "已清空临时记录"),
    };
  }

  function openMenu(item: ClipItem, x: number, y: number) {
    const entries: MenuEntry[] = [
      { label: "粘贴", icon: icons.paste, shortcut: "Enter", run: () => paste(item) },
    ];
    if (item.thumb) {
      entries.push({ label: "预览原图", icon: icons.eye, run: () => (preview = item) });
    }
    if (item.rich || item.kind === "files") {
      const label = plainDefault ? "保留格式粘贴" : "以纯文本粘贴";
      entries.push({ label, icon: icons.plain, shortcut: "Shift+Enter", run: () => paste(item, true) });
    }
    entries.push(
      { label: "仅复制", icon: icons.copy, run: () => run(() => copyClip(item.id), "已复制到剪贴板") },
      {
        label: item.pinned ? "取消置顶" : "置顶",
        icon: icons.pin,
        shortcut: "Ctrl+T",
        run: () => togglePin(item),
      },
      {
        label: item.permanent ? "转为临时" : "设为永久",
        icon: item.permanent ? icons.clock : icons.star,
        shortcut: "Ctrl+P",
        run: () => toggle(item),
      },
      { label: "删除", icon: icons.trash, shortcut: "Del", danger: true, run: () => remove(item) },
    );
    menu = { x, y, entries };
  }

  async function select(index: number) {
    sel = Math.max(0, Math.min(index, items.length - 1));
    await tick();
    listEl?.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
  }

  function switchTab(next: Tab) {
    if (tab === next) return;
    tab = next;
    sel = 0;
    listEl?.scrollTo({ top: 0 });
  }

  function onKeydown(e: KeyboardEvent) {
    // 输入法组字期间的按键（如回车上屏、Esc 取消候选）交给输入法处理
    if (e.isComposing || e.keyCode === 229) return;
    if (preview) {
      if (e.key === "Escape") {
        e.preventDefault();
        preview = null;
      } else if (e.key === "Enter") {
        e.preventDefault();
        const item = preview;
        preview = null;
        paste(item);
      }
      return;
    }
    if (menu || confirm) {
      if (e.key === "Escape") {
        e.preventDefault();
        menu = null;
        confirm = null;
      }
      return;
    }
    const item = items[sel];
    const inSearch = e.target === searchEl;
    const ctrl = e.ctrlKey || e.metaKey;

    if (e.key === "ArrowDown") {
      e.preventDefault();
      select(sel + 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      select(sel - 1);
    } else if (e.key === "PageDown" || e.key === "PageUp") {
      e.preventDefault();
      select(sel + (e.key === "PageDown" ? 8 : -8));
    } else if (e.key === "Enter") {
      e.preventDefault();
      paste(item, e.shiftKey);
    } else if (e.key === "Tab") {
      e.preventDefault();
      switchTab(tab === "temp" ? "perm" : "temp");
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (query) query = "";
      else hidePopup();
    } else if (ctrl && e.key.toLowerCase() === "p") {
      e.preventDefault();
      toggle(item);
    } else if (ctrl && e.key.toLowerCase() === "t") {
      e.preventDefault();
      togglePin(item);
    } else if (ctrl && /^[1-9]$/.test(e.key)) {
      e.preventDefault();
      paste(items[Number(e.key) - 1]);
    } else if (e.key === "Delete" && !(inSearch && query)) {
      // 搜索框有内容时 Delete 用于编辑文字
      e.preventDefault();
      remove(item);
    } else if (!inSearch && e.key.length === 1 && !ctrl && !e.altKey) {
      searchEl?.focus();
    }
  }

  function onBlur() {
    menu = null;
  }
</script>

<svelte:window onkeydown={onKeydown} onfocus={() => searchEl?.focus()} onblur={onBlur} />

<main class="popup">
  <div class="drag" data-tauri-drag-region title="拖动以移动窗口">{@html icons.grip}</div>
  <header class="head" data-tauri-drag-region>
    <label class="search">
      {@html icons.search}
      <input bind:this={searchEl} bind:value={query} placeholder="搜索剪贴板历史" autocomplete="off" spellcheck="false" />
    </label>
    <button class="icon-btn" title="设置" onclick={openSettings}>{@html icons.gear}</button>
  </header>

  <div class="tabs" role="tablist" data-tauri-drag-region>
    <button class="tab" class:active={tab === "temp"} role="tab" aria-selected={tab === "temp"} onclick={() => switchTab("temp")}>
      临时 <span class="count">{counts.temp}</span>
    </button>
    <button class="tab" class:active={tab === "perm"} role="tab" aria-selected={tab === "perm"} onclick={() => switchTab("perm")}>
      永久 <span class="count">{counts.perm}</span>
    </button>
    {#if tab === "temp" && counts.temp > 0}
      <button class="clear" onclick={askClearTemp}>清空临时</button>
    {/if}
  </div>

  {#if items.length > 0}
    <ul class="list" role="listbox" aria-label="剪贴板条目" bind:this={listEl} onscroll={onScroll}>
      {#each items as item, i (item.id)}
        <ClipRow
          {item}
          index={i}
          selected={i === sel}
          {now}
          onselect={() => (sel = i)}
          onpaste={() => paste(item)}
          ontoggle={() => toggle(item)}
          onpin={() => togglePin(item)}
          onpreview={() => (preview = item)}
          ondelete={() => remove(item)}
          onmenu={(x, y) => openMenu(item, x, y)}
        />
      {/each}
      {#if loadingMore}<li class="more">正在加载…</li>{/if}
    </ul>
  {:else}
    <section class="empty">
      {#if loadError}
        <h3>加载失败</h3>
        <p>{loadError}</p>
      {:else if !loaded}
        <p>正在加载…</p>
      {:else if query.trim()}
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
        <p>复制任意内容后会出现在这里，<br />到期后自动删除</p>
      {/if}
    </section>
  {/if}

  <footer class="foot">
    <span><kbd>↑↓</kbd> 选择</span>
    <span><kbd>Enter</kbd> 粘贴</span>
    <span><kbd>Ctrl+P</kbd> 永久</span>
    <span><kbd>Ctrl+T</kbd> 置顶</span>
    <span><kbd>Del</kbd> 删除</span>
  </footer>

  {#if preview}
    {@const item = preview}
    <ImagePreview
      {item}
      onclose={() => (preview = null)}
      onpaste={() => {
        preview = null;
        paste(item);
      }}
    />
  {/if}

  {#if toast}
    <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
  {/if}

  {#if confirm}
    <ConfirmDialog
      title={confirm.title}
      message={confirm.message}
      action={confirm.action}
      oncancel={() => {
        confirm = null;
        searchEl?.focus();
      }}
      onconfirm={() => {
        // 先取出回调再关闭对话框：置空 confirm 后无法再读取它
        const action = confirm?.run;
        confirm = null;
        searchEl?.focus();
        action?.();
      }}
    />
  {/if}
</main>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} entries={menu.entries} onclose={() => (menu = null)} />
{/if}

<style>
  :global(body) {
    overflow: hidden;
  }
  .popup {
    position: relative;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
  }
  .drag {
    height: 12px;
    display: grid;
    place-items: center;
    color: var(--ink-3);
    cursor: grab;
  }
  .drag :global(svg) {
    pointer-events: none;
  }
  .head {
    display: flex;
    gap: 6px;
    padding: 0 10px 6px;
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
    user-select: text;
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
    align-items: center;
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
  .clear {
    margin-left: auto;
    border: 0;
    background: transparent;
    padding: 4px 6px;
    border-radius: 4px;
    font-size: 12px;
    color: var(--ink-3);
  }
  .clear:hover {
    color: var(--danger);
    background: var(--danger-soft);
  }
  .list {
    flex: 1;
    margin: 0;
    padding: 6px;
    list-style: none;
    overflow-y: auto;
    scrollbar-width: thin;
  }
  .more {
    padding: 8px;
    text-align: center;
    font-size: 12px;
    color: var(--ink-3);
  }
  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    padding: 24px;
    text-align: center;
    color: var(--ink-2);
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
    word-break: break-all;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 8px 12px;
    border-top: 1px solid var(--line);
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: 44px;
    transform: translateX(-50%);
    max-width: calc(100% - 32px);
    padding: 7px 12px;
    border-radius: 6px;
    background: var(--ink);
    color: var(--surface);
    font-size: 12.5px;
    pointer-events: none;
    z-index: 5;
  }
  .toast.error {
    background: var(--danger);
    color: #fff;
  }
</style>
