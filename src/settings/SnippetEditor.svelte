<script lang="ts">
  import { onMount } from "svelte";
  import type { SnippetItem } from "../lib/api";

  interface Props {
    snippet: SnippetItem | null;
    content: string;
    onsave: (title: string, content: string) => Promise<void>;
    oncancel: () => void;
  }

  let { snippet, content, onsave, oncancel }: Props = $props();
  let title = $state("");
  let body = $state("");
  let saving = $state(false);
  let error = $state("");
  let titleEl: HTMLInputElement | undefined = $state();

  // 编辑器每次打开都是新实例；用 effect 同步初始值，避免只捕获 props 的初始快照。
  $effect(() => {
    title = snippet?.title ?? "";
    body = content;
  });

  onMount(() => titleEl?.focus());

  async function save() {
    if (saving) return;
    const clean = title.trim();
    if (!clean) {
      error = "请填写片段标题";
      titleEl?.focus();
      return;
    }
    if (!body.trim()) {
      error = "请填写片段内容";
      return;
    }
    saving = true;
    error = "";
    try {
      await onsave(clean, body);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function cancel() {
    if (!saving) oncancel();
  }
</script>

<div class="backdrop" role="presentation" onclick={cancel}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label={snippet ? "编辑片段" : "新建片段"}
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => {
      e.stopPropagation();
      if (e.key === "Escape") {
        e.preventDefault();
        cancel();
      } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        save();
      }
    }}
  >
    <header>
      <div>
        <h3>{snippet ? "编辑片段" : "新建片段"}</h3>
        <p>保存后可在弹出列表的「片段」标签中直接粘贴。</p>
      </div>
    </header>

    <label class="field">
      <span>标题</span>
      <input bind:this={titleEl} bind:value={title} maxlength="80" placeholder="例如：工作邮件开头" />
    </label>

    <label class="field grow">
      <span>内容</span>
      <textarea bind:value={body} placeholder="输入需要反复使用的文字…" spellcheck="false"></textarea>
    </label>

    <footer>
      <span class="hint">
        {[...body].length.toLocaleString()} 字符 · <kbd>Ctrl+Enter</kbd> 保存 · <kbd>Esc</kbd> 取消
      </span>
      {#if error}<span class="error">{error}</span>{/if}
      <button class="btn" disabled={saving} onclick={cancel}>取消</button>
      <button class="btn primary" disabled={saving} onclick={save}>{saving ? "保存中…" : "保存"}</button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 20;
    display: grid;
    place-items: center;
    padding: 20px;
    background: rgba(0, 0, 0, 0.22);
  }
  .dialog {
    display: flex;
    flex-direction: column;
    width: min(560px, 100%);
    max-height: 100%;
    padding: 18px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28);
    outline: none;
  }
  header {
    margin-bottom: 14px;
  }
  h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  header p {
    margin: 4px 0 0;
    font-size: 12.5px;
    color: var(--ink-2);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
    font-size: 12.5px;
    color: var(--ink-2);
  }
  .field.grow {
    flex: 1;
    min-height: 0;
    margin-bottom: 14px;
  }
  input,
  textarea {
    padding: 8px 10px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    outline: none;
    background: var(--card);
    color: var(--ink);
    font: inherit;
    user-select: text;
  }
  input {
    height: 34px;
  }
  textarea {
    flex: 1;
    min-height: 220px;
    resize: vertical;
    font-size: 13px;
    line-height: 1.6;
  }
  input:focus,
  textarea:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  footer {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .hint {
    flex: 1;
    min-width: 180px;
    font-size: 11.5px;
    color: var(--ink-3);
  }
  .error {
    flex-basis: 100%;
    color: var(--danger);
    font-size: 12px;
  }
  .btn {
    height: 32px;
    padding: 0 14px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--card);
    font-size: 13px;
  }
  .btn:hover:not(:disabled) {
    background: var(--card-hover);
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(1.07);
  }
  @media (max-width: 640px) {
    .dialog {
      padding: 14px;
    }
  }
</style>
