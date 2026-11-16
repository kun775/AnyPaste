<script lang="ts">
  import { onMount } from "svelte";
  import {
    deleteSnippet,
    getSnippet,
    listSnippets,
    onSnippetsChanged,
    saveSnippet,
    type SnippetItem,
  } from "../lib/api";
  import ConfirmDialog from "../lib/ConfirmDialog.svelte";
  import { ago } from "../lib/format";
  import { icons } from "../lib/icons";
  import SnippetEditor from "./SnippetEditor.svelte";

  interface Props {
    toast: (text: string, error?: boolean) => void;
  }

  type Editing = { snippet: SnippetItem | null; content: string };

  let { toast }: Props = $props();
  let snippets = $state<SnippetItem[]>([]);
  let query = $state("");
  let loaded = $state(false);
  let loadError = $state("");
  let editing = $state<Editing | null>(null);
  let deleting = $state<SnippetItem | null>(null);
  let now = $state(Date.now());
  let requestSeq = 0;

  async function refresh() {
    const seq = ++requestSeq;
    try {
      const list = await listSnippets(query.trim() || null);
      if (seq !== requestSeq) return;
      snippets = list;
      loadError = "";
    } catch (e) {
      if (seq === requestSeq) loadError = String(e);
    } finally {
      if (seq === requestSeq) loaded = true;
    }
  }

  // 输入搜索词时稍作防抖
  $effect(() => {
    const q = query;
    const timer = setTimeout(refresh, q ? 120 : 0);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    const clock = setInterval(() => (now = Date.now()), 30_000);
    const off = onSnippetsChanged(refresh);
    return () => {
      clearInterval(clock);
      off.then((f) => f());
    };
  });

  function openNew() {
    editing = { snippet: null, content: "" };
  }

  async function openEdit(item: SnippetItem) {
    try {
      const draft = await getSnippet(item.id);
      editing = { snippet: item, content: draft.content };
    } catch (e) {
      toast(String(e), true);
    }
  }

  async function save(title: string, content: string) {
    const current = editing;
    if (!current) return;
    const saved = await saveSnippet(current.snippet?.id ?? null, title, content);
    editing = null;
    toast(current.snippet ? `已更新片段「${saved.title}」` : `已创建片段「${saved.title}」`);
    await refresh();
  }

  async function doDelete() {
    const target = deleting;
    deleting = null;
    if (!target) return;
    try {
      await deleteSnippet(target.id);
      toast("已删除片段");
      await refresh();
    } catch (e) {
      toast(String(e), true);
    }
  }
</script>

<div class="row">
  <div class="rt">
    <b>常用片段</b>
    <small>把经常发送的文字保存起来，在弹出列表的「片段」标签中随时粘贴。片段只保存在本机。</small>
  </div>
  <button class="btn primary" onclick={openNew}>新建片段</button>
</div>

<div class="search">
  {@html icons.search}
  <input
    bind:value={query}
    aria-label="搜索片段"
    placeholder="搜索片段标题或内容"
    autocomplete="off"
    spellcheck="false"
  />
  {#if query}
    <button
      class="clear"
      type="button"
      title="清空搜索"
      aria-label="清空搜索"
      onclick={() => {
        query = "";
        void refresh();
      }}>×</button
    >
  {/if}
</div>

{#if loadError}
  <p class="error">读取片段失败：{loadError}</p>
{:else if !loaded}
  <p class="muted">正在加载…</p>
{:else if snippets.length === 0}
  <div class="empty">
    {#if query.trim()}
      <h3>没有匹配的片段</h3>
      <p>没有找到包含“{query.trim()}”的片段。</p>
    {:else}
      <h3>还没有常用片段</h3>
      <p>把常用的回复、地址或命令保存下来，下次直接粘贴。</p>
      <button class="btn" onclick={openNew}>新建第一个片段</button>
    {/if}
  </div>
{:else}
  <ul class="list">
    {#each snippets as item (item.id)}
      <li class="item">
        <div class="head">
          <b>{item.title}</b>
          <span class="meta">{item.chars.toLocaleString()} 字符 · 更新于 {ago(item.updatedAt, now)}</span>
        </div>
        <p>{item.preview}</p>
        <div class="actions">
          <button class="btn small" onclick={() => openEdit(item)}>编辑</button>
          <button class="btn small danger" onclick={() => (deleting = item)}>删除</button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

{#if deleting}
  <ConfirmDialog
    title="删除片段？"
    message={`「${deleting.title}」将从常用片段中删除，此操作无法撤销。`}
    action="删除"
    oncancel={() => (deleting = null)}
    onconfirm={doDelete}
  />
{/if}

{#if editing}
  <SnippetEditor
    snippet={editing.snippet}
    content={editing.content}
    onsave={save}
    oncancel={() => (editing = null)}
  />
{/if}

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    margin: 4px 0 8px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-bottom-color: var(--line-strong);
    border-radius: 6px;
    background: var(--card);
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
    color: var(--ink);
    font-size: 13px;
    user-select: text;
  }
  .clear {
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    color: var(--ink-3);
    font-size: 16px;
    line-height: 1;
  }
  .clear:hover {
    background: var(--card-hover);
    color: var(--ink);
  }
  .list {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .item {
    padding: 12px 14px;
    margin-bottom: 3px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--card);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .head b {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13.5px;
    font-weight: 600;
  }
  .meta {
    flex: none;
    color: var(--ink-3);
    font-size: 11.5px;
  }
  .item p {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    margin: 5px 0 0;
    color: var(--ink-2);
    font-size: 12.5px;
    line-height: 1.55;
    word-break: break-all;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 8px;
  }
  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(1.07);
  }
  .btn.small {
    height: 28px;
    padding: 0 10px;
    font-size: 12.5px;
  }
  .btn.danger {
    color: var(--danger);
  }
  .empty {
    display: grid;
    place-content: center;
    min-height: 180px;
    padding: 24px;
    border: 1px dashed var(--line-strong);
    border-radius: 8px;
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
    margin: 0 0 12px;
    font-size: 12.5px;
    line-height: 1.6;
  }
  .empty .btn {
    justify-self: center;
  }
</style>
