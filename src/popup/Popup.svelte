<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    PAGE_SIZE,
    clearTemp,
    clipCounts,
    copyClip,
    copySnippet,
    deleteClip,
    deleteSnippet,
    getSettings,
    hidePopup,
    isPaused,
    listClips,
    listSnippets,
    onClipsChanged,
    onPopupShown,
    onSettingsChanged,
    onSnippetsChanged,
    openSettings,
    pasteClip,
    pasteSnippet,
    saveClipAsSnippet,
    setPermanent,
    setPinned,
    setPopupPreview,
    type ClipItem,
    type Counts,
    type KindFilter,
    type Settings,
    type SnippetItem,
    type Tab,
  } from "../lib/api";
  import ConfirmDialog from "../lib/ConfirmDialog.svelte";
  import { ago } from "../lib/format";
  import { icons } from "../lib/icons";
  import { applyTheme } from "../lib/theme";
  import ClipRow from "./ClipRow.svelte";
  import ContextMenu, { type MenuEntry } from "./ContextMenu.svelte";
  import ImagePreview from "./ImagePreview.svelte";
  import SnippetRow from "./SnippetRow.svelte";
  import TextPreview from "./TextPreview.svelte";

  type View = Tab | "snippets";
  type Row = { kind: "clip"; item: ClipItem } | { kind: "snippet"; item: SnippetItem };

  /** 类型筛选项，顺序即 Ctrl+F 的切换顺序。 */
  const FILTERS: { value: KindFilter | null; label: string; icon: string }[] = [
    { value: null, label: "全部", icon: icons.all },
    { value: "text", label: "文本", icon: icons.text },
    { value: "link", label: "链接", icon: icons.link },
    { value: "image", label: "图片", icon: icons.image },
    { value: "files", label: "文件", icon: icons.files },
  ];

  interface Confirm {
    title: string;
    message: string;
    action: string;
    run: () => Promise<void>;
  }

  const VIEWS: View[] = ["temp", "perm", "snippets"];

  let tab = $state<View>("temp");
  let filter = $state<KindFilter | null>(null);
  let query = $state("");
  let items = $state<ClipItem[]>([]);
  let snippets = $state<SnippetItem[]>([]);
  let counts = $state<Counts>({ temp: 0, perm: 0, snippets: 0 });
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
  let preview = $state<Row | null>(null);
  let imageFullscreen = $state(false);

  let searchEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLUListElement | undefined = $state();
  let requestSeq = 0;
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  /** 重新加载第一页。用请求序号丢弃过期响应，避免快速输入时结果错乱。 */
  async function reload() {
    const seq = ++requestSeq;
    const current = tab;
    try {
      const countsPromise = clipCounts();
      const pausedPromise = isPaused();
      const list = current === "snippets" ? await listSnippets(query.trim() || null) : await listClips(current, filter, query, null);
      const [c, p] = await Promise.all([countsPromise, pausedPromise]);
      if (seq !== requestSeq) return;
      if (current === "snippets") {
        snippets = list as SnippetItem[];
        items = [];
      } else {
        items = list as ClipItem[];
        snippets = [];
      }
      counts = c;
      paused = p;
      hasMore = current !== "snippets" && (list as ClipItem[]).filter((i) => !i.pinned).length === PAGE_SIZE;
      sel = Math.min(sel, Math.max(0, list.length - 1));
      loadError = "";
    } catch (e) {
      if (seq === requestSeq) loadError = String(e);
    } finally {
      if (seq === requestSeq) loaded = true;
    }
  }

  async function loadMore() {
    const current = tab;
    if (current === "snippets" || !hasMore || loadingMore || items.length === 0) return;
    const seq = requestSeq;
    // 游标取最后一条非置顶条目；置顶条目只随第一页返回
    const last = items.findLast((i) => !i.pinned);
    if (!last) return;
    loadingMore = true;
    try {
      const more = await listClips(current, filter, query, {
        lastCopiedAt: last.lastCopiedAt,
        id: last.id,
        pasteCount: last.pasteCount,
      });
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

  // 切换 Tab、筛选或输入搜索词时重新加载；输入时稍作防抖
  $effect(() => {
    void tab;
    void filter;
    const q = query;
    const timer = setTimeout(reload, q ? 120 : 0);
    return () => clearTimeout(timer);
  });

  let sortedByFrequency: boolean | undefined;

  function applySettings(s: Settings) {
    // 排序方式变化后重新加载，正在显示的列表立即按新规则排列
    const sortChanged = sortedByFrequency !== undefined && sortedByFrequency !== s.sortByFrequency;
    sortedByFrequency = s.sortByFrequency;
    plainDefault = s.plainPasteDefault;
    applyTheme(s.theme);
    if (sortChanged) reload();
  }

  onMount(() => {
    searchEl?.focus();
    const clock = setInterval(() => (now = Date.now()), 30_000);
    getSettings()
      .then((v) => applySettings(v.settings))
      .catch(() => {});
    const unlisten = [
      onClipsChanged(reload),
      onSnippetsChanged(reload),
      onSettingsChanged(applySettings),
      onPopupShown(async () => {
        menu = null;
        confirm = null;
        preview = null;
        imageFullscreen = false;
        query = "";
        filter = null;
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

  const rows = $derived<Row[]>(
    tab === "snippets"
      ? snippets.map((item) => ({ kind: "snippet" as const, item }))
      : items.map((item) => ({ kind: "clip" as const, item })),
  );
  const searchPlaceholder = $derived(tab === "snippets" ? "搜索常用片段" : "搜索剪贴板历史");
  const filterLabel = $derived(filter ? (FILTERS.find((f) => f.value === filter)?.label ?? "") : "");

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

  /** `alternate` 为 true（Shift+Enter）时使用与默认相反的格式。返回是否实际粘贴并隐藏了弹窗。 */
  async function paste(row: Row | undefined, alternate = false): Promise<boolean> {
    if (!row) return false;
    try {
      const { pasted } =
        row.kind === "snippet" ? await pasteSnippet(row.item.id) : await pasteClip(row.item.id, plainDefault !== alternate);
      if (!pasted) showToast("已复制到剪贴板");
      return pasted;
    } catch (e) {
      showToast(String(e), true);
      return false;
    }
  }

  function closePreview() {
    const wasImage = preview?.kind === "clip" && isImage(preview.item);
    imageFullscreen = false;
    preview = null;
    if (wasImage) {
      setPopupPreview({ mode: "restore" }).catch(() => {});
    }
    searchEl?.focus();
  }

  async function pasteFromPreview(row: Row, alternate = false) {
    const wasImage = row.kind === "clip" && isImage(row.item);
    const pasted = await paste(row, alternate);
    imageFullscreen = false;
    preview = null;
    // 真正粘贴时后端 hide_popup 已经隐藏并重置窗口尺寸，避免先缩回闪烁；仅复制时弹窗保持可见，需要恢复原尺寸
    if (!pasted) {
      if (wasImage) {
        setPopupPreview({ mode: "restore" }).catch(() => {});
      }
      searchEl?.focus();
    }
  }

  function copy(row: Row | undefined) {
    if (!row) return;
    run(() => (row.kind === "snippet" ? copySnippet(row.item.id) : copyClip(row.item.id)), "已复制到剪贴板");
  }

  async function saveAsSnippet(item: ClipItem | undefined) {
    if (!item || (item.kind !== "text" && item.kind !== "rich")) return;
    try {
      const snippet = await saveClipAsSnippet(item.id);
      showToast(`已存为片段「${snippet.title}」`);
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

  function remove(row: Row | undefined) {
    if (!row) return;
    if (row.kind === "snippet") {
      confirm = {
        title: "删除片段？",
        message: `「${row.item.title}」将从常用片段中删除，此操作无法撤销。`,
        action: "删除",
        run: () => run(() => deleteSnippet(row.item.id), "已删除片段"),
      };
      return;
    }
    const item = row.item;
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

  function openMenu(row: Row, x: number, y: number) {
    if (row.kind === "snippet") {
      menu = {
        x,
        y,
        entries: [
          { label: "粘贴", icon: icons.paste, shortcut: "Enter", run: () => paste(row) },
          { label: "查看全文", icon: icons.eye, shortcut: "Space", run: () => (preview = row) },
          { label: "仅复制", icon: icons.copy, run: () => copy(row) },
          { label: "删除", icon: icons.trash, shortcut: "Del", danger: true, run: () => remove(row) },
        ],
      };
      return;
    }
    const item = row.item;
    const entries: MenuEntry[] = [
      { label: "粘贴", icon: icons.paste, shortcut: "Enter", run: () => paste(row) },
    ];
    entries.push({
      label: isImage(item) ? "预览原图" : "查看全文",
      icon: icons.eye,
      shortcut: "Space",
      run: () => (preview = row),
    });
    if (item.rich || item.kind === "files") {
      const label = plainDefault ? "保留格式粘贴" : "以纯文本粘贴";
      entries.push({ label, icon: icons.plain, shortcut: "Shift+Enter", run: () => paste(row, true) });
    }
    entries.push(
      { label: "仅复制", icon: icons.copy, run: () => copy(row) },
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
    );
    if (item.kind === "text" || item.kind === "rich") {
      entries.push({
        label: "存为常用片段",
        icon: icons.snippet,
        shortcut: "Ctrl+S",
        run: () => saveAsSnippet(item),
      });
    }
    entries.push({ label: "删除", icon: icons.trash, shortcut: "Del", danger: true, run: () => remove(row) });
    menu = { x, y, entries };
  }

  async function select(index: number) {
    sel = Math.max(0, Math.min(index, rows.length - 1));
    await tick();
    listEl?.querySelector(".sel")?.scrollIntoView({ block: "nearest" });
  }

  function switchTab(next: View) {
    if (tab === next) return;
    tab = next;
    sel = 0;
    loaded = false;
    listEl?.scrollTo({ top: 0 });
  }

  function cycleTab(step: number) {
    const i = VIEWS.indexOf(tab);
    switchTab(VIEWS[(i + step + VIEWS.length) % VIEWS.length]);
  }

  function setFilter(next: KindFilter | null) {
    if (filter === next) return;
    filter = next;
    sel = 0;
    listEl?.scrollTo({ top: 0 });
  }

  function cycleFilter(step: number) {
    const i = FILTERS.findIndex((f) => f.value === filter);
    setFilter(FILTERS[(i + step + FILTERS.length) % FILTERS.length].value);
  }

  /** 图片条目与带缩略图的图片文件用图片预览，其余用全文预览。 */
  function isImage(item: ClipItem) {
    return item.kind === "image" || item.thumb !== null;
  }

  function onKeydown(e: KeyboardEvent) {
    // 输入法组字期间的按键（如回车上屏、Esc 取消候选）交给输入法处理
    if (e.isComposing || e.keyCode === 229) return;
    if (preview) {
      const previewingImage = preview.kind === "clip" && isImage(preview.item);
      if (previewingImage && !e.ctrlKey && !e.metaKey && !e.altKey && (e.key === "f" || e.key === "F" || e.key === "F11")) {
        e.preventDefault();
        imageFullscreen = !imageFullscreen;
      } else if (e.key === "Escape") {
        e.preventDefault();
        if (previewingImage && imageFullscreen) {
          imageFullscreen = false;
        } else {
          closePreview();
        }
      } else if (e.key === " ") {
        e.preventDefault();
        closePreview();
      } else if (e.key === "Enter") {
        e.preventDefault();
        pasteFromPreview(preview, e.shiftKey);
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
    const row = rows[sel];
    const clip = row?.kind === "clip" ? row.item : undefined;
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
      paste(row, e.shiftKey);
    } else if (e.key === "Tab") {
      e.preventDefault();
      cycleTab(e.shiftKey ? -1 : 1);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (query) query = "";
      else hidePopup();
    } else if (e.key === " " && !query) {
      // 搜索框为空时空格用于预览；已有搜索词时空格正常输入
      e.preventDefault();
      if (row) preview = row;
    } else if (ctrl && e.key.toLowerCase() === "f") {
      e.preventDefault();
      if (tab !== "snippets") cycleFilter(e.shiftKey ? -1 : 1);
    } else if (ctrl && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (clip) saveAsSnippet(clip);
    } else if (ctrl && e.key.toLowerCase() === "p") {
      e.preventDefault();
      if (clip) toggle(clip);
    } else if (ctrl && e.key.toLowerCase() === "t") {
      e.preventDefault();
      if (clip) togglePin(clip);
    } else if (ctrl && /^[1-9]$/.test(e.key)) {
      e.preventDefault();
      paste(rows[Number(e.key) - 1]);
    } else if (e.key === "Delete" && !(inSearch && query)) {
      // 搜索框有内容时 Delete 用于编辑文字
      e.preventDefault();
      remove(row);
    } else if (!inSearch && e.key.length === 1 && !ctrl && !e.altKey) {
      searchEl?.focus();
    }
  }

  function onBlur() {
    menu = null;
  }
</script>

<svelte:window
  onkeydown={onKeydown}
  onfocus={() => {
    if (!preview && !confirm && !menu) searchEl?.focus();
  }}
  onblur={onBlur}
/>

<main class="popup">
  <div class="drag" data-tauri-drag-region title="拖动以移动窗口">{@html icons.grip}</div>
  <header class="head" data-tauri-drag-region>
    <label class="search">
      {@html icons.search}
      <input bind:this={searchEl} bind:value={query} placeholder={searchPlaceholder} autocomplete="off" spellcheck="false" />
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
    <button
      class="tab snippet-tab"
      class:active={tab === "snippets"}
      role="tab"
      aria-selected={tab === "snippets"}
      onclick={() => switchTab("snippets")}
    >
      片段 <span class="count">{counts.snippets}</span>
    </button>
    {#if tab !== "snippets"}
      <div class="filters" role="group" aria-label="按类型筛选">
        {#each FILTERS as f (f.label)}
          <button
            class="filter"
            class:active={filter === f.value}
            aria-pressed={filter === f.value}
            aria-label={f.label}
            title="{f.label}（Ctrl+F 切换）"
            onclick={() => setFilter(f.value)}
          >
            {@html f.icon}
          </button>
        {/each}
      </div>
    {/if}
    {#if tab === "temp" && counts.temp > 0}
      <button class="clear" title="清空临时记录" aria-label="清空临时记录" onclick={askClearTemp}>{@html icons.trash}</button>
    {/if}
  </div>

  {#if rows.length > 0}
    <ul
      class="list"
      role="listbox"
      aria-label={tab === "snippets" ? "常用片段" : "剪贴板条目"}
      bind:this={listEl}
      onscroll={onScroll}
    >
      {#each rows as row, i (`${row.kind}-${row.item.id}`)}
        {#if row.kind === "clip"}
          <ClipRow
            item={row.item}
            index={i}
            selected={i === sel}
            {now}
            {query}
            onselect={() => (sel = i)}
            onpaste={() => paste(row)}
            ontoggle={() => toggle(row.item)}
            onpin={() => togglePin(row.item)}
            onpreview={() => (preview = row)}
            ondelete={() => remove(row)}
            onmenu={(x, y) => openMenu(row, x, y)}
          />
        {:else}
          <SnippetRow
            item={row.item}
            index={i}
            selected={i === sel}
            {now}
            {query}
            onselect={() => (sel = i)}
            onpaste={() => paste(row)}
            onpreview={() => (preview = row)}
            oncopy={() => copy(row)}
            ondelete={() => remove(row)}
            onmenu={(x, y) => openMenu(row, x, y)}
          />
        {/if}
      {/each}
      {#if loadingMore && tab !== "snippets"}<li class="more">正在加载…</li>{/if}
    </ul>
  {:else}
    <section class="empty">
      {#if loadError}
        <h3>加载失败</h3>
        <p>{loadError}</p>
      {:else if !loaded}
        <p>正在加载…</p>
      {:else if tab === "snippets" && query.trim()}
        <h3>没有匹配的片段</h3>
        <p>没有找到包含“{query.trim()}”的常用片段</p>
      {:else if tab === "snippets"}
        <h3>还没有常用片段</h3>
        <p>在文本条目上按 <kbd>Ctrl+S</kbd> 保存，<br />或到 设置 → 片段 中新建</p>
      {:else if query.trim()}
        <h3>没有匹配的记录</h3>
        <p>
          没有找到包含“{query.trim()}”的{tab === "perm" ? "永久" : "临时"}{filterLabel}条目
        </p>
        {#if filter}<button class="link-btn" onclick={() => setFilter(null)}>在全部类型中搜索</button>{/if}
      {:else if filter}
        <h3>没有{filterLabel}条目</h3>
        <p>{tab === "perm" ? "永久" : "临时"}列表中还没有这种类型的内容</p>
        <button class="link-btn" onclick={() => setFilter(null)}>显示全部类型</button>
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
    <span><kbd>Space</kbd> 预览</span>
    {#if tab !== "snippets"}
      <span><kbd>Ctrl+P</kbd> 永久</span>
      <span><kbd>Ctrl+S</kbd> 存片段</span>
    {/if}
    <span><kbd>Tab</kbd> 切换</span>
    <span><kbd>Del</kbd> 删除</span>
  </footer>

  {#if preview}
    {@const row = preview}
    {@const pasteIt = () => pasteFromPreview(row)}
    {#if row.kind === "clip"}
      {@const item = row.item}
      {#if isImage(item)}
        <ImagePreview
          {item}
          fullscreen={imageFullscreen}
          onfullscreen={(next) => (imageFullscreen = next)}
          onclose={closePreview}
          onpaste={pasteIt}
        />
      {:else}
        <TextPreview
          source="clip"
          id={item.id}
          preview={item.preview}
          title={item.kind === "files" ? "文件列表" : item.rich ? "富文本（纯文本视图）" : "全文"}
          sub={[item.sourceApp, ago(item.lastCopiedAt, now)].filter(Boolean).join(" · ")}
          mono={item.kind === "files"}
          {query}
          onclose={closePreview}
          onpaste={pasteIt}
        />
      {/if}
    {:else}
      <TextPreview
        source="snippet"
        id={row.item.id}
        preview={row.item.preview}
        title={row.item.title}
        sub={`常用片段 · 更新于 ${ago(row.item.updatedAt, now)}`}
        {query}
        onclose={closePreview}
        onpaste={pasteIt}
      />
    {/if}
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
    gap: 2px;
    padding: 2px 6px 0;
    border-bottom: 1px solid var(--line);
  }
  .tab {
    position: relative;
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
    padding: 8px 6px 10px;
    border: 0;
    background: transparent;
    font-size: 12.5px;
    white-space: nowrap;
    color: var(--ink-2);
  }
  .tab.active {
    color: var(--ink);
    font-weight: 600;
  }
  .tab.active::after {
    content: "";
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: -1px;
    height: 3px;
    border-radius: 3px;
    background: var(--accent);
  }
  .tab.snippet-tab.active::after {
    background: var(--snippet);
  }
  .tab.snippet-tab .count {
    background: var(--snippet-soft);
    color: var(--snippet);
  }
  .count {
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    display: inline-grid;
    place-items: center;
    font-size: 10.5px;
    font-weight: 500;
    background: var(--line);
    color: var(--ink-2);
  }
  .filters {
    flex: none;
    margin-left: auto;
    display: flex;
    gap: 0;
    padding: 1px;
    border-radius: 6px;
    background: var(--line);
  }
  .filter {
    width: 23px;
    height: 22px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    display: grid;
    place-items: center;
    color: var(--ink-3);
  }
  .filter :global(svg) {
    width: 14px;
    height: 14px;
  }
  .filter:hover {
    color: var(--ink);
  }
  .filter.active {
    background: var(--card);
    color: var(--accent);
    box-shadow: 0 0 0 1px var(--line);
  }
  .clear {
    width: 26px;
    height: 26px;
    border: 0;
    background: transparent;
    border-radius: 4px;
    display: grid;
    place-items: center;
    color: var(--ink-3);
  }
  .link-btn {
    margin: 10px auto 0;
    padding: 4px 8px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    font-size: 12.5px;
    color: var(--accent);
  }
  .link-btn:hover {
    background: var(--accent-soft);
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
