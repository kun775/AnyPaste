<script lang="ts">
  interface Props {
    items: string[];
    placeholder?: string;
    onchange: (items: string[]) => void;
  }

  let { items, placeholder = "输入程序名，如 notepad.exe", onchange }: Props = $props();
  let draft = $state("");

  function add() {
    const name = draft.trim();
    if (!name) return;
    if (!items.some((i) => i.toLowerCase() === name.toLowerCase())) onchange([...items, name]);
    draft = "";
  }
</script>

<div class="chips">
  {#each items as item (item)}
    <span class="chip">
      {item}
      <button title="移除 {item}" aria-label="移除 {item}" onclick={() => onchange(items.filter((i) => i !== item))}>×</button>
    </span>
  {/each}
  <input
    bind:value={draft}
    {placeholder}
    spellcheck="false"
    onkeydown={(e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        add();
      }
    }}
    onblur={add}
  />
</div>

<style>
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 26px;
    padding: 0 4px 0 10px;
    border-radius: 13px;
    background: var(--line);
    font-size: 12px;
  }
  .chip button {
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--ink-2);
    line-height: 1;
  }
  .chip button:hover {
    background: var(--line-strong);
  }
  input {
    flex: 1;
    min-width: 180px;
    height: 26px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px dashed var(--line-strong);
    background: transparent;
    font-size: 12px;
    user-select: text;
  }
</style>
