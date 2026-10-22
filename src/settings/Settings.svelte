<script lang="ts">
  import { onMount } from "svelte";
  import {
    appInfo,
    clearTemp,
    getSettings,
    onClipsChanged,
    onSettingsChanged,
    openDataDir,
    restartExplorer,
    setAutostart,
    storageStats,
    updateSettings,
    type AppInfo,
    type Settings,
    type SettingsView,
    type StorageStats,
  } from "../lib/api";
  import ConfirmDialog from "../lib/ConfirmDialog.svelte";
  import { formatSize } from "../lib/format";
  import { applyTheme } from "../lib/theme";
  import AppList from "./AppList.svelte";
  import HotkeyInput from "./HotkeyInput.svelte";

  type Pane = "general" | "keys" | "storage" | "about";

  const PANES: { id: Pane; label: string }[] = [
    { id: "general", label: "常规" },
    { id: "keys", label: "快捷键" },
    { id: "storage", label: "存储" },
    { id: "about", label: "关于" },
  ];

  const HOUR = 3600;
  const TTL_OPTIONS = [HOUR, 12 * HOUR, 24 * HOUR, 72 * HOUR, 168 * HOUR];
  const MAX_ITEMS_OPTIONS = [200, 500, 1000, 5000, 0];
  const MB = 1024 * 1024;
  const MAX_BYTES_OPTIONS = [MB, 5 * MB, 10 * MB, 50 * MB, 100 * MB];

  let pane = $state<Pane>("general");
  let view = $state<SettingsView | null>(null);
  let stats = $state<StorageStats | null>(null);
  let info = $state<AppInfo | null>(null);
  let loadError = $state("");
  let busy = $state(false);
  let toast = $state<{ text: string; error: boolean } | null>(null);
  let confirmClear = $state(false);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  const s = $derived(view?.settings);

  $effect(() => {
    if (s) applyTheme(s.theme);
  });

  async function refresh() {
    try {
      view = await getSettings();
      loadError = "";
    } catch (e) {
      loadError = String(e);
    }
  }

  async function refreshStats() {
    stats = await storageStats().catch(() => null);
  }

  onMount(() => {
    refresh();
    refreshStats();
    appInfo()
      .then((v) => (info = v))
      .catch(() => {});
    const unlisten = [onSettingsChanged(() => refresh()), onClipsChanged(() => refreshStats())];
    return () => unlisten.forEach((p) => p.then((off) => off()));
  });

  function showToast(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), error ? 4000 : 1600);
  }

  /** 立即保存；失败时恢复原值并提示原因。 */
  async function save(patch: Partial<Settings>) {
    if (!view) return;
    const prev = view;
    view = { ...view, settings: { ...view.settings, ...patch } };
    try {
      view = await updateSettings(view.settings);
    } catch (e) {
      view = prev;
      showToast(String(e), true);
    }
  }

  async function toggleAutostart(enabled: boolean) {
    if (!view) return;
    const prev = view;
    view = { ...view, autostart: enabled };
    try {
      view = await setAutostart(enabled);
    } catch (e) {
      view = prev;
      showToast(`设置开机启动失败：${e}`, true);
    }
  }

  async function doRestartExplorer() {
    busy = true;
    try {
      view = await restartExplorer();
      showToast(view.winVActive ? "已接管 Win + V" : "资源管理器已重启，但 Win + V 仍未释放", !view.winVActive);
    } catch (e) {
      showToast(String(e), true);
    } finally {
      busy = false;
    }
  }

  async function doClearTemp() {
    confirmClear = false;
    try {
      const n = await clearTemp();
      showToast(`已清空 ${n} 条临时记录`);
      refreshStats();
    } catch (e) {
      showToast(String(e), true);
    }
  }

  function ttlLabel(secs: number) {
    if (secs % (24 * HOUR) === 0) return `${secs / (24 * HOUR)} 天`;
    if (secs % HOUR === 0) return `${secs / HOUR} 小时`;
    return `${Math.round(secs / 60)} 分钟`;
  }

  /** 当前值不在预设中时（如手动修改过）也要能显示。 */
  function withCurrent(options: number[], current: number) {
    return options.includes(current) ? options : [...options, current].sort((a, b) => (a || Infinity) - (b || Infinity));
  }
</script>

<div class="window">
  <nav class="nav" aria-label="设置分类">
    {#each PANES as p (p.id)}
      <button class:active={pane === p.id} aria-current={pane === p.id ? "page" : undefined} onclick={() => (pane = p.id)}>
        {p.label}
      </button>
    {/each}
  </nav>

  <main class="pane">
    {#if loadError}
      <p class="error">读取设置失败：{loadError}</p>
    {:else if !view || !s}
      <p class="muted">正在加载…</p>
    {:else if pane === "general"}
      <h1>常规</h1>
      <div class="row">
        <div class="rt"><b>开机自动启动</b><small>登录 Windows 后在后台运行，只显示托盘图标</small></div>
        <label class="switch"><input type="checkbox" checked={view.autostart} onchange={(e) => toggleAutostart(e.currentTarget.checked)} /><span></span></label>
      </div>
      <div class="row">
        <div class="rt"><b>关闭窗口时最小化到托盘</b><small>关闭后仍在后台记录，可从托盘图标重新打开；关闭此项则关闭窗口即退出程序</small></div>
        <label class="switch"><input type="checkbox" checked={s.closeToTray} onchange={(e) => save({ closeToTray: e.currentTarget.checked })} /><span></span></label>
      </div>
      <div class="row">
        <div class="rt"><b>暂停记录</b><small>暂停期间的复制不会写入历史，永久复制不受影响</small></div>
        <label class="switch"><input type="checkbox" checked={s.paused} onchange={(e) => save({ paused: e.currentTarget.checked })} /><span></span></label>
      </div>
      <div class="row">
        <div class="rt"><b>永久复制后显示通知</b></div>
        <label class="switch"><input type="checkbox" checked={s.notifyPermanent} onchange={(e) => save({ notifyPermanent: e.currentTarget.checked })} /><span></span></label>
      </div>

      <h2>弹出列表</h2>
      <div class="row">
        <div class="rt"><b>弹出位置</b></div>
        <select value={s.popupPosition} onchange={(e) => save({ popupPosition: e.currentTarget.value as Settings["popupPosition"] })}>
          <option value="cursor">跟随鼠标光标</option>
          <option value="center">屏幕中央</option>
        </select>
      </div>
      <div class="row">
        <div class="rt"><b>默认粘贴格式</b><small>按住 Shift 回车可临时切换为另一种格式</small></div>
        <select value={s.plainPasteDefault ? "plain" : "original"} onchange={(e) => save({ plainPasteDefault: e.currentTarget.value === "plain" })}>
          <option value="original">保留原格式</option>
          <option value="plain">纯文本</option>
        </select>
      </div>
      <div class="row">
        <div class="rt"><b>主题</b></div>
        <select value={s.theme} onchange={(e) => save({ theme: e.currentTarget.value as Settings["theme"] })}>
          <option value="system">跟随系统</option>
          <option value="light">浅色</option>
          <option value="dark">深色</option>
        </select>
      </div>
    {:else if pane === "keys"}
      <h1>快捷键</h1>
      <div class="row">
        <div class="rt"><b>打开剪贴板历史</b><small>在光标附近弹出列表</small></div>
        <HotkeyInput value={s.hotkeyOpen} onchange={(v) => save({ hotkeyOpen: v })} />
      </div>
      <div class="row">
        <div class="rt"><b>复制并永久保存</b><small>复制选中内容并直接存入永久列表；未选中内容时，把最近一条临时记录设为永久</small></div>
        <HotkeyInput value={s.hotkeyPermanent} onchange={(v) => save({ hotkeyPermanent: v })} />
      </div>
      <div class="row col">
        <div class="line">
          <div class="rt"><b>接管 Win + V</b><small>用 AnyPaste 替代系统剪贴板历史；关闭后恢复系统的 Win + V</small></div>
          <label class="switch"><input type="checkbox" checked={s.takeOverWinV} onchange={(e) => save({ takeOverWinV: e.currentTarget.checked })} /><span></span></label>
        </div>
        {#if s.takeOverWinV && !view.winVActive}
          <div class="note">
            <span>需要重启资源管理器才能生效，期间任务栏会短暂消失。</span>
            <button class="btn" disabled={busy} onclick={doRestartExplorer}>{busy ? "正在重启…" : "立即重启资源管理器"}</button>
          </div>
        {:else if s.takeOverWinV}
          <div class="status ok">已接管 Win + V</div>
        {:else if view.winVReleased}
          <div class="note"><span>系统的 Win + V 仍处于禁用状态，重启资源管理器后恢复。</span>
            <button class="btn" disabled={busy} onclick={doRestartExplorer}>{busy ? "正在重启…" : "立即重启资源管理器"}</button>
          </div>
        {/if}
      </div>
      <h2>终端类程序</h2>
      <div class="row col">
        <div class="rt"><b>永久复制时改用 Ctrl + Insert</b><small>终端中 Ctrl + C 在没有选中内容时会中断正在运行的命令</small></div>
        <AppList items={s.terminalApps} onchange={(v) => save({ terminalApps: v })} />
      </div>
    {:else if pane === "storage"}
      <h1>存储</h1>
      <div class="stats">
        <div class="stat"><b>{stats?.temp ?? "–"}</b><small>临时条目</small></div>
        <div class="stat"><b>{stats?.perm ?? "–"}</b><small>永久条目</small></div>
        <div class="stat"><b>{stats ? formatSize(stats.bytes) : "–"}</b><small>占用空间</small></div>
      </div>
      <h2>临时条目</h2>
      <div class="row">
        <div class="rt"><b>保留时长</b><small>从最后一次复制开始计时，到期自动删除；修改后对已有条目同样生效</small></div>
        <select value={s.tempTtlSecs} onchange={(e) => save({ tempTtlSecs: Number(e.currentTarget.value) })}>
          {#each withCurrent(TTL_OPTIONS, s.tempTtlSecs) as v (v)}<option value={v}>{ttlLabel(v)}</option>{/each}
        </select>
      </div>
      <div class="row">
        <div class="rt"><b>最多保留</b><small>超出后删除最早的临时条目</small></div>
        <select value={s.tempMaxItems} onchange={(e) => save({ tempMaxItems: Number(e.currentTarget.value) })}>
          {#each withCurrent(MAX_ITEMS_OPTIONS, s.tempMaxItems) as v (v)}<option value={v}>{v === 0 ? "不限制" : `${v} 条`}</option>{/each}
        </select>
      </div>
      <div class="row">
        <div class="rt"><b>单条内容上限</b><small>超过上限的内容不记录</small></div>
        <select value={s.maxItemBytes} onchange={(e) => save({ maxItemBytes: Number(e.currentTarget.value) })}>
          {#each withCurrent(MAX_BYTES_OPTIONS, s.maxItemBytes) as v (v)}<option value={v}>{formatSize(v)}</option>{/each}
        </select>
      </div>
      <h2>记录范围</h2>
      <div class="row col">
        <div class="rt"><b>记录的内容类型</b><small>关闭“富文本”后只保留文字，不保留格式</small></div>
        <div class="checks">
          <label><input type="checkbox" checked={s.captureText} onchange={(e) => save({ captureText: e.currentTarget.checked })} /> 文本</label>
          <label><input type="checkbox" checked={s.captureRich} onchange={(e) => save({ captureRich: e.currentTarget.checked })} /> 富文本</label>
          <label><input type="checkbox" checked={s.captureImage} onchange={(e) => save({ captureImage: e.currentTarget.checked })} /> 图片</label>
          <label><input type="checkbox" checked={s.captureFiles} onchange={(e) => save({ captureFiles: e.currentTarget.checked })} /> 文件</label>
        </div>
      </div>
      <div class="row col">
        <div class="rt"><b>忽略的应用</b><small>来自这些程序的复制不会被记录。密码管理器标记为敏感的内容始终忽略。</small></div>
        <AppList items={s.ignoredApps} onchange={(v) => save({ ignoredApps: v })} />
      </div>
      <div class="row">
        <div class="rt"><b>数据目录</b><small class="path">{info?.dataDir ?? ""}</small></div>
        <button class="btn" onclick={() => openDataDir().catch((e) => showToast(String(e), true))}>打开文件夹</button>
      </div>
      <div class="row">
        <div class="rt"><b>清空临时历史</b><small>永久条目不受影响</small></div>
        <button class="btn danger" disabled={!stats?.temp} onclick={() => (confirmClear = true)}>清空</button>
      </div>
    {:else}
      <h1>关于</h1>
      <div class="row">
        <div class="rt"><b>AnyPaste {info?.version ?? ""}</b><small>本地剪贴板历史</small></div>
      </div>
      <div class="row">
        <div class="rt"><b>数据目录</b><small class="path">{info?.dataDir ?? ""}</small></div>
      </div>
      <div class="row">
        <div class="rt"><b>日志目录</b><small class="path">{info?.logDir ?? ""}</small></div>
      </div>
    {/if}
  </main>

  {#if toast}
    <div class="toast" class:error={toast.error} role="status">{toast.text}</div>
  {/if}
  {#if confirmClear}
    <ConfirmDialog
      title="清空临时记录？"
      message={`将删除全部 ${stats?.temp ?? 0} 条临时条目，永久条目不受影响。此操作无法撤销。`}
      action="清空"
      oncancel={() => (confirmClear = false)}
      onconfirm={doClearTemp}
    />
  {/if}
</div>

<style>
  .window {
    position: relative;
    height: 100vh;
    display: grid;
    grid-template-columns: 180px 1fr;
  }
  .nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 8px;
  }
  .nav button {
    position: relative;
    height: 36px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    font-size: 13.5px;
    text-align: left;
  }
  .nav button:hover,
  .nav button.active {
    background: var(--card-hover);
  }
  .nav button.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 3px;
    background: var(--accent);
  }
  .pane {
    overflow-y: auto;
    padding: 16px 24px 24px 8px;
    scrollbar-width: thin;
  }
  h1 {
    margin: 4px 0 14px;
    font-size: 22px;
    font-weight: 600;
  }
  h2 {
    margin: 18px 0 6px;
    font-size: 13px;
    font-weight: 600;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 60px;
    padding: 12px 16px;
    margin-bottom: 3px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--card);
  }
  .row.col {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 16px;
  }
  .rt {
    flex: 1;
    min-width: 0;
  }
  .rt b {
    display: block;
    font-weight: 500;
    font-size: 13.5px;
  }
  .rt small {
    display: block;
    margin-top: 2px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-2);
  }
  .path {
    font-family: var(--mono);
    user-select: text;
    word-break: break-all;
  }
  select {
    min-width: 150px;
    height: 32px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    font-size: 13px;
  }
  .btn {
    height: 32px;
    padding: 0 14px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--card);
    font-size: 13px;
    white-space: nowrap;
  }
  .btn:hover:not(:disabled) {
    background: var(--card-hover);
  }
  .btn:disabled {
    opacity: 0.5;
  }
  .btn.danger {
    color: var(--danger);
  }
  .switch {
    position: relative;
    flex: none;
    width: 40px;
    height: 20px;
  }
  .switch input {
    position: absolute;
    inset: 0;
    z-index: 1;
    margin: 0;
    opacity: 0;
    cursor: pointer;
  }
  .switch span {
    position: absolute;
    inset: 0;
    border-radius: 10px;
    border: 1px solid var(--ink-2);
    transition: 0.15s;
  }
  .switch span::after {
    content: "";
    position: absolute;
    left: 4px;
    top: 4px;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--ink-2);
    transition: 0.15s;
  }
  .switch input:checked + span {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch input:checked + span::after {
    left: 23px;
    background: var(--accent-ink);
  }
  .switch input:focus-visible + span {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .note {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 6px;
    background: var(--perm-soft);
    font-size: 12.5px;
    line-height: 1.6;
  }
  .note span {
    flex: 1;
  }
  .status.ok {
    font-size: 12.5px;
    color: var(--ok);
  }
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    font-size: 13px;
  }
  .checks label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .stat {
    padding: 10px 12px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--card);
  }
  .stat b {
    display: block;
    font-size: 18px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .stat small {
    font-size: 12px;
    color: var(--ink-2);
  }
  .muted {
    color: var(--ink-2);
  }
  .error {
    color: var(--danger);
  }
  .toast {
    position: absolute;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    max-width: calc(100% - 48px);
    padding: 8px 14px;
    border-radius: 6px;
    background: var(--ink);
    color: var(--surface);
    font-size: 12.5px;
    z-index: 5;
  }
  .toast.error {
    background: var(--danger);
    color: #fff;
  }
  @media (max-width: 640px) {
    .window {
      grid-template-columns: 1fr;
      grid-template-rows: auto 1fr;
    }
    .nav {
      flex-direction: row;
      overflow-x: auto;
      padding: 8px;
    }
    .nav button.active::before {
      display: none;
    }
    .pane {
      padding: 8px 12px 16px;
    }
    .row {
      flex-wrap: wrap;
    }
  }
</style>
