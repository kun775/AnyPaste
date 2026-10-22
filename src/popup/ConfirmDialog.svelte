<script lang="ts">
  interface Props {
    title: string;
    message: string;
    action: string;
    onconfirm: () => void;
    oncancel: () => void;
  }

  let { title, message, action, onconfirm, oncancel }: Props = $props();
  let cancelBtn: HTMLButtonElement | undefined = $state();

  $effect(() => {
    cancelBtn?.focus();
  });
</script>

<div class="backdrop" role="presentation" onclick={oncancel}>
  <div
    class="box"
    role="alertdialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => {
      // 对话框内的按键不再传给列表的全局快捷键
      e.stopPropagation();
      if (e.key === "Escape") oncancel();
    }}
  >
    <div class="content">
      <h4>{title}</h4>
      <p>{message}</p>
    </div>
    <div class="footer">
      <button class="btn" bind:this={cancelBtn} onclick={oncancel}>取消</button>
      <button class="btn danger" onclick={onconfirm}>{action}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 10;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.18);
  }
  .box {
    width: min(300px, calc(100% - 32px));
    overflow: hidden;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.25);
    outline: none;
  }
  .content {
    padding: 18px 18px 14px;
  }
  h4 {
    margin: 0 0 6px;
    font-size: 15px;
  }
  p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--ink-2);
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 18px;
    background: var(--card-hover);
  }
  .btn {
    height: 32px;
    padding: 0 14px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--card);
    font-size: 13px;
  }
  .btn:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }
  .btn.danger {
    background: var(--danger);
    border-color: transparent;
    color: #fff;
  }
</style>
