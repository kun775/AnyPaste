<script lang="ts">
  import { appInfo, type AppInfo } from "../lib/api";

  let info = $state<AppInfo | null>(null);
  let error = $state("");

  appInfo()
    .then((v) => (info = v))
    .catch((e) => (error = String(e)));
</script>

<main class="page">
  <h1>设置</h1>

  <h2>关于</h2>
  {#if error}
    <p class="error">读取应用信息失败：{error}</p>
  {:else if !info}
    <p class="muted">正在加载…</p>
  {:else}
    <div class="row">
      <div><b>AnyPaste {info.version}</b><small>本地剪贴板历史</small></div>
    </div>
    <div class="row">
      <div><b>数据目录</b><small class="path">{info.dataDir}</small></div>
    </div>
    <div class="row">
      <div><b>日志目录</b><small class="path">{info.logDir}</small></div>
    </div>
  {/if}
</main>

<style>
  .page {
    max-width: 720px;
    padding: 20px 28px;
  }
  h1 {
    margin: 0 0 16px;
    font-size: 24px;
    font-weight: 600;
  }
  h2 {
    margin: 16px 0 8px;
    font-size: 14px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    min-height: 60px;
    padding: 12px 16px;
    margin-bottom: 3px;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .row b {
    display: block;
    font-weight: 500;
  }
  .row small {
    display: block;
    margin-top: 2px;
    font-size: 12px;
    color: var(--ink-2);
  }
  .path {
    font-family: var(--mono);
    user-select: text;
  }
  .muted {
    color: var(--ink-2);
  }
  .error {
    color: var(--danger);
  }
</style>
