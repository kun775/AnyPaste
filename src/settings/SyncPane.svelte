<script lang="ts">
  import { onMount } from "svelte";
  import {
    getSync,
    onSyncStatus,
    saveSync,
    syncNow,
    testSync,
    type SecretsInput,
    type SyncConfig,
    type SyncStatus,
    type SyncView,
  } from "../lib/api";
  import { ago } from "../lib/format";

  interface Props {
    toast: (text: string, error?: boolean) => void;
  }

  let { toast }: Props = $props();

  const INTERVALS = [5, 15, 30, 60, 180];

  let saved = $state<SyncView | null>(null);
  let draft = $state<SyncConfig | null>(null);
  let status = $state<SyncStatus | null>(null);
  let password = $state("");
  let secretKey = $state("");
  let passphrase = $state("");
  let busy = $state<"" | "test" | "save">("");
  let testResult = $state<{ ok: boolean; text: string } | null>(null);
  let loadError = $state("");
  let now = $state(Date.now());

  const dirty = $derived(
    !!saved && !!draft && (JSON.stringify(saved.config) !== JSON.stringify(draft) || !!password || !!secretKey || !!passphrase),
  );

  async function load() {
    try {
      const v = await getSync();
      saved = v;
      draft = { ...v.config };
      status = v.status;
      loadError = "";
    } catch (e) {
      loadError = String(e);
    }
  }

  onMount(() => {
    load();
    const clock = setInterval(() => (now = Date.now()), 30_000);
    const off = onSyncStatus((s) => {
      status = s;
      now = Date.now();
    });
    return () => {
      clearInterval(clock);
      off.then((f) => f());
    };
  });

  /** 只提交填写过的凭据；留空表示沿用已保存的值。 */
  function secrets(): SecretsInput {
    const s: SecretsInput = {};
    if (password) s.password = password;
    if (secretKey) s.secretKey = secretKey;
    if (passphrase) s.passphrase = passphrase;
    return s;
  }

  async function test() {
    if (!draft) return;
    busy = "test";
    testResult = null;
    try {
      testResult = { ok: true, text: await testSync(draft, secrets()) };
    } catch (e) {
      testResult = { ok: false, text: String(e) };
    } finally {
      busy = "";
    }
  }

  async function save() {
    if (!draft) return;
    busy = "save";
    try {
      const v = await saveSync(draft, secrets());
      saved = v;
      draft = { ...v.config };
      status = v.status;
      password = secretKey = passphrase = "";
      toast(v.config.enabled ? "已保存，正在同步" : "已保存");
    } catch (e) {
      toast(String(e), true);
    } finally {
      busy = "";
    }
  }

  async function runNow() {
    try {
      await syncNow();
    } catch (e) {
      toast(String(e), true);
    }
  }

  function statusText(s: SyncStatus): string {
    switch (s.state) {
      case "disabled":
        return "未启用";
      case "syncing":
        return "正在同步…";
      case "error":
        return `同步失败：${s.lastError ?? "未知错误"}`;
      default: {
        if (!s.lastSyncAt) return "等待首次同步";
        const r = s.lastReport;
        const detail = r && (r.uploaded || r.downloaded || r.deleted) ? `，上传 ${r.uploaded}、下载 ${r.downloaded}、删除 ${r.deleted}` : "";
        return `上次同步：${ago(s.lastSyncAt, now)}${detail}`;
      }
    }
  }
</script>

{#if loadError}
  <p class="error">读取同步设置失败：{loadError}</p>
{:else if !draft || !saved}
  <p class="muted">正在加载…</p>
{:else}
  <div class="row">
    <div class="rt"><b>启用云同步</b><small>只同步永久条目；临时条目始终只保存在本机</small></div>
    <label class="switch"><input type="checkbox" bind:checked={draft.enabled} /><span></span></label>
  </div>

  <div class="row col">
    <div class="line">
      <div class="rt"><b>存储类型</b></div>
      <div class="seg" role="radiogroup" aria-label="存储类型">
        <button role="radio" aria-checked={draft.backend === "webdav"} class:on={draft.backend === "webdav"} onclick={() => draft && (draft.backend = "webdav")}>WebDAV</button>
        <button role="radio" aria-checked={draft.backend === "s3"} class:on={draft.backend === "s3"} onclick={() => draft && (draft.backend = "s3")}>S3 兼容</button>
      </div>
    </div>
    {#if draft.backend === "webdav"}
      <div class="grid">
        <label class="field wide">服务器地址<input bind:value={draft.webdavEndpoint} placeholder="https://dav.jianguoyun.com/dav/" spellcheck="false" /></label>
        <label class="field">用户名<input bind:value={draft.webdavUsername} autocomplete="off" spellcheck="false" /></label>
        <label class="field">密码 / 应用密码<input type="password" bind:value={password} placeholder={saved.hasPassword ? "已保存，留空则不修改" : ""} autocomplete="new-password" /></label>
        <label class="field wide">远端目录<input bind:value={draft.root} placeholder="/AnyPaste" spellcheck="false" /></label>
      </div>
    {:else}
      <div class="grid">
        <label class="field wide">Endpoint<input bind:value={draft.s3Endpoint} placeholder="https://<account>.r2.cloudflarestorage.com" spellcheck="false" /></label>
        <label class="field">Bucket<input bind:value={draft.s3Bucket} spellcheck="false" /></label>
        <label class="field">Region<input bind:value={draft.s3Region} placeholder="auto" spellcheck="false" /></label>
        <label class="field">Access Key ID<input bind:value={draft.s3AccessKeyId} autocomplete="off" spellcheck="false" /></label>
        <label class="field">Secret Access Key<input type="password" bind:value={secretKey} placeholder={saved.hasSecretKey ? "已保存，留空则不修改" : ""} autocomplete="new-password" /></label>
        <label class="field wide">路径前缀<input bind:value={draft.root} placeholder="/AnyPaste" spellcheck="false" /></label>
      </div>
    {/if}
  </div>

  <div class="row col">
    <div class="line">
      <div class="rt"><b>端到端加密</b><small>上传前在本机加密，服务器只能看到密文。忘记口令将无法恢复云端数据；所有设备需使用相同口令。</small></div>
      <label class="switch"><input type="checkbox" bind:checked={draft.encrypt} /><span></span></label>
    </div>
    {#if draft.encrypt}
      <label class="field">同步口令<input type="password" bind:value={passphrase} placeholder={saved.hasPassphrase ? "已保存，留空则不修改" : "设置一个足够长的口令"} autocomplete="new-password" /></label>
    {/if}
  </div>

  <div class="row">
    <div class="rt"><b>自动同步间隔</b><small>永久条目变化后也会在约 30 秒内同步</small></div>
    <select bind:value={draft.intervalMins}>
      {#each INTERVALS.includes(draft.intervalMins) ? INTERVALS : [...INTERVALS, draft.intervalMins] as m (m)}
        <option value={m}>{m < 60 ? `${m} 分钟` : `${m / 60} 小时`}</option>
      {/each}
    </select>
  </div>

  {#if testResult}
    <div class="result" class:fail={!testResult.ok} role="status">{testResult.text}</div>
  {/if}

  <div class="row actions">
    <div class="rt">
      {#if status}<span class="status {status.state}"><i></i>{statusText(status)}</span>{/if}
    </div>
    <button class="btn" disabled={busy !== ""} onclick={test}>{busy === "test" ? "正在测试…" : "测试连接"}</button>
    <button class="btn" disabled={!saved.config.enabled || dirty || status?.state === "syncing"} onclick={runNow}>立即同步</button>
    <button class="btn primary" disabled={!dirty || busy !== ""} onclick={save}>{busy === "save" ? "正在保存…" : "保存"}</button>
  </div>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--ink-2);
  }
  .field.wide {
    grid-column: 1 / -1;
  }
  .field input {
    height: 32px;
    padding: 0 10px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: var(--surface);
    color: var(--ink);
    font-size: 13px;
    user-select: text;
  }
  .seg {
    display: inline-flex;
    padding: 2px;
    border-radius: 6px;
    background: var(--line);
  }
  .seg button {
    height: 28px;
    padding: 0 14px;
    border: 0;
    border-radius: 4px;
    background: transparent;
    font-size: 12.5px;
    color: var(--ink-2);
  }
  .seg button.on {
    background: var(--card);
    color: var(--ink);
    font-weight: 500;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .result {
    margin: 6px 0;
    padding: 10px 12px;
    border-radius: 6px;
    font-size: 12.5px;
    background: var(--accent-soft);
  }
  .result.fail {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .actions {
    flex-wrap: wrap;
  }
  .status {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--ink-2);
    word-break: break-all;
  }
  .status i {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ink-3);
  }
  .status.idle i {
    background: var(--ok);
  }
  .status.syncing i {
    background: var(--accent);
  }
  .status.error {
    color: var(--danger);
  }
  .status.error i {
    background: var(--danger);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: transparent;
  }
  @media (max-width: 640px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
