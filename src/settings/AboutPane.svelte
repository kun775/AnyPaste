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

  type UpdateState = {
    kind: "pending" | "error" | "available" | "current" | "idle";
    title: string;
    detail: string;
  };

  let { info, settings, save, toast }: Props = $props();
  let status = $state<UpdateStatus | null>(null);
  let now = $state(Date.now());
  let action = $state<"check" | "install" | null>(null);

  /** 请求结束得太快时也让状态至少显示片刻，避免用户只看到一次闪烁。 */
  const MIN_FEEDBACK_MS = 500;

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

  async function settleFeedback(started: number) {
    const wait = MIN_FEEDBACK_MS - (Date.now() - started);
    if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
  }

  async function check() {
    if (busy) return;
    const started = Date.now();
    action = "check";
    try {
      const found = await checkUpdate();
      toast(found ? `发现新版本 ${found.version}` : "已是最新版本");
    } catch (e) {
      toast(String(e), true);
    } finally {
      await settleFeedback(started);
      action = null;
    }
  }

  async function install() {
    if (busy) return;
    action = "install";
    try {
      await installUpdate();
    } catch (e) {
      toast(String(e), true);
    } finally {
      action = null;
    }
  }

  function open(link: "repo" | "releases" | "issues") {
    openLink(link).catch((e) => toast(String(e), true));
  }

  const checking = $derived(action === "check" || !!status?.checking);
  const installing = $derived(action === "install" || !!status?.installing);
  const busy = $derived(checking || installing);
  const portable = $derived(info?.portable ?? status?.portable ?? false);
  const available = $derived(status?.available ?? null);
  const error = $derived(status?.error ?? null);
  const percent = $derived(status?.progress != null ? Math.round(status.progress * 100) : null);
  const updateState = $derived.by((): UpdateState => {
    if (installing) {
      return {
        kind: "pending",
        title: percent === null || percent === 0 ? "正在准备更新" : "正在下载新版本",
        detail:
          percent === null || percent === 0
            ? "正在获取更新包，完成后会自动安装并重新打开"
            : `已下载 ${percent}%，完成后会自动安装并重新打开`,
      };
    }
    if (checking) {
      return { kind: "pending", title: "正在检查更新", detail: "正在连接更新服务器，请稍候…" };
    }
    if (error) {
      return { kind: "error", title: "更新未完成", detail: error };
    }
    if (available) {
      return {
        kind: "available",
        title: `发现新版本 ${available.version}`,
        detail: portable
          ? "便携版不支持自动安装，请前往发布页下载并替换程序文件。"
          : "可一键下载并安装，更新后会自动重新打开。",
      };
    }
    if (status?.lastCheckedAt) {
      return { kind: "current", title: "已是最新版本", detail: `上次检查 ${ago(status.lastCheckedAt, now)}` };
    }
    return { kind: "idle", title: "尚未检查更新", detail: "检查是否有可用的新版本。" };
  });
</script>

<section class="brand" aria-label="版本信息">
  <div class="logo" aria-hidden="true">
    <svg width="26" height="26" viewBox="0 0 24 24" fill="none">
      <rect x="5" y="3.5" width="14" height="17" rx="3.2" stroke="currentColor" stroke-width="1.8" />
      <rect x="9" y="1.6" width="6" height="3.6" rx="1.4" fill="currentColor" />
      <path d="M9 10h6M9 14h4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
    </svg>
  </div>
  <div class="brand-copy">
    <div class="brand-name">AnyPaste <span class="version">v{info?.version ?? "…"}</span></div>
    <small>本地剪贴板历史 · 轻快、克制、隐私优先</small>
  </div>
  {#if portable}<span class="tag">便携版</span>{/if}
</section>

<h2>软件更新</h2>
<section class="update-card" class:highlight={updateState.kind === "available"}>
  <div class="update-main">
    <div
      class="state-icon"
      class:pending={updateState.kind === "pending"}
      class:error={updateState.kind === "error"}
      class:available={updateState.kind === "available"}
      class:current={updateState.kind === "current"}
    >
      {#if updateState.kind === "pending"}
        <span class="spinner" aria-hidden="true"></span>
      {:else if updateState.kind === "error"}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
          <circle cx="12" cy="12" r="9" />
          <path d="M12 7.5v5.5M12 17h.01" />
        </svg>
      {:else if updateState.kind === "available"}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 4v10m0 0l-4-4m4 4l4-4M5 19h14" />
        </svg>
      {:else if updateState.kind === "current"}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="9" />
          <path d="M8 12.2l2.6 2.6L16.5 9" />
        </svg>
      {:else}
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M20 11a8 8 0 1 0-2.3 6.3M20 5v6h-6" />
        </svg>
      {/if}
    </div>

    <div class="update-copy" aria-live="polite" aria-atomic="true">
      <div class="update-title">
        <b>{updateState.title}</b>
        {#if updateState.kind === "available"}<span class="pill">可更新</span>{/if}
      </div>
      <small class="detail" class:error-text={updateState.kind === "error"}>{updateState.detail}</small>
    </div>

    <div class="actions">
      {#if available && !portable}
        <button class="btn primary" disabled={busy} aria-busy={installing} onclick={install}>
          {#if installing}<span class="spinner" aria-hidden="true"></span>{/if}
          {installing ? "正在更新" : "下载并安装"}
        </button>
      {:else if available}
        <button class="btn primary" onclick={() => open("releases")}>前往下载</button>
      {/if}
      <button class="btn" class:subtle={!!available} disabled={busy} aria-busy={checking} onclick={check}>
        {#if checking}<span class="spinner" aria-hidden="true"></span>{/if}
        {checking ? "检查中" : "检查更新"}
      </button>
    </div>
  </div>

  {#if installing}
    <div class="progress-wrap">
      <div
        class="progress"
        role="progressbar"
        aria-label="更新下载进度"
        aria-valuemin="0"
        aria-valuemax="100"
        aria-valuenow={percent ?? 0}
        aria-valuetext={percent === null ? "正在准备" : `${percent}%`}
      >
        <i
          class:indeterminate={percent === null || percent === 0}
          style={percent !== null && percent > 0 ? `width: ${percent}%` : undefined}
        ></i>
      </div>
      <span class="percent">{percent === null || percent === 0 ? "准备中" : `${percent}%`}</span>
    </div>
  {/if}

  {#if available?.notes}
    <div class="notes-wrap">
      <div class="notes-title">更新内容</div>
      <pre class="notes">{available.notes}</pre>
    </div>
  {/if}
</section>

<h2>自动更新</h2>
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
  .brand {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 15px 16px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background:
      radial-gradient(120% 180% at 0% 0%, var(--accent-soft), transparent 58%),
      var(--card);
  }
  .logo {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 12px;
    color: #fff;
    background: linear-gradient(145deg, #2b8cf0, #0057a8);
    box-shadow: 0 5px 14px rgba(0, 87, 168, 0.26);
  }
  .brand-copy {
    flex: 1;
    min-width: 0;
  }
  .brand-name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    font-weight: 600;
  }
  .brand-copy small {
    display: block;
    margin-top: 3px;
    font-size: 12px;
    color: var(--ink-2);
  }
  .version {
    padding: 2px 6px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: var(--surface);
    color: var(--ink-2);
    font-family: var(--mono);
    font-size: 11.5px;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .tag {
    flex: none;
    padding: 4px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11.5px;
  }
  .update-card {
    padding: 14px 16px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--card);
    transition: background 0.2s, border-color 0.2s, box-shadow 0.2s;
  }
  .update-card.highlight {
    border-color: var(--line-strong);
    background:
      linear-gradient(135deg, var(--accent-soft), transparent 46%),
      var(--card);
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.05);
  }
  .update-main {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .state-icon {
    display: grid;
    flex: none;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 10px;
    color: var(--ink-2);
    background: var(--surface);
  }
  .state-icon.pending,
  .state-icon.available {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .state-icon.error {
    color: var(--danger);
    background: var(--danger-soft);
  }
  .state-icon.current {
    color: var(--ok);
    background: var(--ok-soft);
  }
  .update-copy {
    flex: 1;
    min-width: 0;
  }
  .update-title {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .update-title b {
    font-size: 13.5px;
    font-weight: 600;
  }
  .pill {
    padding: 3px 7px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    line-height: 1.2;
  }
  .detail {
    display: block;
    margin-top: 3px;
    font-size: 12px;
    line-height: 1.55;
    color: var(--ink-2);
    overflow-wrap: anywhere;
  }
  .detail.error-text {
    color: var(--danger);
  }
  .actions {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 8px;
  }
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
  }
  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.12);
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(1.07);
  }
  .btn.subtle {
    background: transparent;
  }
  .btn:disabled[aria-busy="true"] {
    opacity: 1;
    cursor: progress;
  }
  .spinner {
    width: 13px;
    height: 13px;
    flex: none;
    border: 1.6px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  .state-icon .spinner {
    width: 16px;
    height: 16px;
    border-width: 1.8px;
  }
  .progress-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 13px;
  }
  .progress {
    flex: 1;
    height: 6px;
    overflow: hidden;
    border-radius: 999px;
    background: var(--line);
  }
  .progress i {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent-soft);
    transition: width 0.25s ease;
  }
  .progress i.indeterminate {
    width: 34%;
    animation: indeterminate 1.15s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  }
  .percent {
    min-width: 46px;
    color: var(--ink-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .notes-wrap {
    margin-top: 13px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .notes-title {
    margin-bottom: 6px;
    color: var(--ink-2);
    font-size: 12px;
    font-weight: 600;
  }
  .notes {
    margin: 0;
    max-height: 160px;
    overflow: auto;
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--surface);
    color: var(--ink-2);
    font-family: var(--font);
    font-size: 12px;
    line-height: 1.6;
    white-space: pre-wrap;
    user-select: text;
  }
  .switch input:disabled + span {
    opacity: 0.4;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @keyframes indeterminate {
    from {
      transform: translateX(-110%);
    }
    to {
      transform: translateX(330%);
    }
  }
  @media (max-width: 640px) {
    .update-main {
      flex-wrap: wrap;
    }
    .update-copy {
      flex-basis: calc(100% - 48px);
    }
    .actions {
      width: 100%;
      justify-content: flex-start;
      padding-left: 48px;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2s;
    }
    .progress i.indeterminate {
      animation-duration: 2.4s;
    }
  }
</style>
