<script lang="ts">
  import { suspendHotkeys } from "../lib/api";
  import { formatHotkey, hotkeyFromEvent } from "../lib/hotkey";

  interface Props {
    value: string;
    onchange: (accel: string) => Promise<void>;
  }

  let { value, onchange }: Props = $props();
  let recording = $state(false);
  let hint = $state("");

  async function start() {
    if (recording) return;
    // 录制期间暂停全局热键，否则按下已注册的组合会被系统拦截，窗口收不到
    await suspendHotkeys(true).catch(() => {});
    recording = true;
    hint = "";
  }

  async function stop() {
    if (!recording) return;
    recording = false;
    await suspendHotkeys(false).catch(() => {});
  }

  async function onkeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") {
      await stop();
      return;
    }
    const accel = hotkeyFromEvent(e);
    if (accel === null) return;
    if (accel === "") {
      hint = "需要同时按下 Ctrl、Alt、Shift 或 Win";
      return;
    }
    recording = false;
    try {
      await onchange(accel);
    } finally {
      await suspendHotkeys(false).catch(() => {});
    }
  }
</script>

<div class="wrap">
  <button class="hk" class:rec={recording} onclick={start} {onkeydown} onblur={stop} title="点击后按下新的组合键，Esc 取消">
    {recording ? "请按下新的组合键…" : formatHotkey(value)}
  </button>
  {#if hint && recording}<small>{hint}</small>{/if}
</div>

<style>
  .wrap {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
  }
  .hk {
    min-width: 168px;
    height: 32px;
    padding: 0 12px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    font-size: 12.5px;
  }
  .hk.rec {
    border-color: var(--accent);
    color: var(--accent);
  }
  small {
    font-size: 11.5px;
    color: var(--danger);
  }
</style>
