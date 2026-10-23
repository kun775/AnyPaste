<script lang="ts">
  import { onMount } from "svelte";
  import {
    checkUpdate,
    getUpdateStatus,
    installUpdate,
    onUpdateStatus,
    openLink,
    type AppInfo,
    type Settings,
    type UpdateStatus,
  } from "../lib/api";
  import { ago } from "../lib/format";

  interface Props {
    info: AppInfo | null;
    settings: Settings;
    save: (patch: Partial<Settings>) => void;
    toast: (text: string, error?: boolean) => void;
  }

  let { info, settings, save, toast }: Props = $props();
  let status = $state<UpdateStatus | null>(null);
  let now = $state(Date.now());

  onMount(() => {
    getUpdateStatus()
      .then((s) => (status = s))
      .catch(() => {});
    const off = onUpdateStatus((s) => {
      status = s;
      now = Date.now();
    });
    const clock = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      clearInterval(clock);
      off.then((f) => f());
    };
  });

  async function check() {
    try {
      const found = await checkUpdate();
      if (!found) toast("已是最新版本");
    } catch (e) {
      toast(String(e), true);
    }
  }

  async function install() {
    try {
      await installUpdate();
    } catch (e) {
      toast(String(e), true);
    }
  }

  function open(link: "repo" | "releases" | "issues") {
    openLink(link).catch((e) => toast(String(e), true));
  }

  const busy = $derived(!!status && (status.checking || status.installing));
  const portable = $derived(info?.portable ?? status?.portable ?? false);
</script>

<div class="row">
  <div class="rt">
    <b>AnyPaste {info?.version ?? ""}{portable ? "（便携版）" : ""}</b>
    <small>本地剪贴板历史</small>
  </div>
</div>

<div class="row col">
  <div class="line">
    <div class="rt">
      <b>软件更新</b>
      <small>
        {#if status?.installing}
          正在下载新版本{status.progress !== null ? ` ${Math.round(status.progress * 100)}%` : "…"}，完成后将自动安装并重新打开
        {:else if status?.checking}
          正在检查更新…
        {:else if status?.available}
          发现新版本 {status.available.version}
        {:else if status?.error}
          <span class="error">{status.error}</span>
        {:else if status?.lastCheckedAt}
          已是最新版本 · 上次检查 {ago(status.lastCheckedAt, now)}
        {:else}
          尚未检查
        {/if}
      </small>
    </div>
    {#if status?.available && !portable}
      <button class="btn primary" disabled={busy} onclick={install}>下载并安装</button>
    {:else if status?.available}
      <button class="btn primary" onclick={() => open("releases")}>前往下载</button>
    {/if}
    <button class="btn" disabled={busy} onclick={check}>{status?.checking ? "检查中…" : "检查更新"}</button>
  </div>
  {#if status?.installing && status.progress !== null}
    <div class="progress" role="progressbar" aria-valuenow={Math.round(status.progress * 100)}>
      <i style="width: {status.progress * 100}%"></i>
    </div>
  {/if}
  {#if status?.available?.notes}
    <pre class="notes">{status.available.notes}</pre>
  {/if}
</div>

<div class="row">
  <div class="rt"><b>自动检查更新</b><small>每天检查一次，发现新版本时发送通知</small></div>
  <label class="switch">
    <input type="checkbox" checked={settings.autoCheckUpdate} onchange={(e) => save({ autoCheckUpdate: e.currentTarget.checked })} />
    <span></span>
  </label>
</div>
<div class="row">
  <div class="rt">
    <b>自动安装更新</b>
    <small>
      {portable ? "便携版不支持自动安装，请从发布页下载新版本" : "发现新版本后在后台下载，并在弹出列表未打开时自动安装（程序会短暂退出并重新打开）"}
    </small>
  </div>
  <label class="switch">
    <input
      type="checkbox"
      checked={settings.autoInstallUpdate && !portable}
      disabled={portable || !settings.autoCheckUpdate}
      onchange={(e) => save({ autoInstallUpdate: e.currentTarget.checked })}
    />
    <span></span>
  </label>
</div>

<h2>项目</h2>
<div class="row">
  <div class="rt"><b>GitHub</b><small class="path">{info?.repoUrl ?? ""}</small></div>
  <button class="btn" onclick={() => open("repo")}>打开主页</button>
  <button class="btn" onclick={() => open("releases")}>发布页</button>
  <button class="btn" onclick={() => open("issues")}>反馈问题</button>
</div>
<div class="row">
  <div class="rt"><b>数据目录</b><small class="path">{info?.dataDir ?? ""}</small></div>
</div>
<div class="row">
  <div class="rt"><b>日志目录</b><small class="path">{info?.logDir ?? ""}</small></div>
</div>

<style>
  .btn.primary {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: transparent;
  }
  .progress {
    height: 4px;
    border-radius: 2px;
    background: var(--line);
    overflow: hidden;
  }
  .progress i {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .notes {
    margin: 0;
    max-height: 160px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--surface);
    font-family: var(--font);
    font-size: 12px;
    line-height: 1.6;
    white-space: pre-wrap;
    color: var(--ink-2);
    user-select: text;
  }
  .switch input:disabled + span {
    opacity: 0.4;
  }
</style>
