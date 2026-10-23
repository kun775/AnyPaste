# AnyPaste 需求分析与开发计划

> 本地剪贴板历史工具，目标是小巧、美观、上手即用，可替代 Windows 自带的 `Win+V`。
> 交互原型见 [prototype/index.html](../prototype/index.html)。

## 1. 需求理解

| # | 原始需求 | 理解与落地方式 |
|---|---|---|
| R1 | 复制快捷键自动保存 | 监听系统剪贴板变化（`AddClipboardFormatListener`），不挂钩 `Ctrl+C` 按键本身。这样右键菜单复制、程序内复制也会被记录，行为与 `Win+V` 一致。 |
| R2 | 24 小时自动过期 | 普通复制产生**临时条目**，`expires_at = 最近一次复制时间 + TTL`（默认 24h，可配置）。重复复制同一内容会刷新过期时间并置顶。后台每分钟清理一次，每次新增条目后也会清理。 |
| R3 | 指定快捷键复制可永久保存 | 全局热键（默认 `Ctrl+Shift+C`，可改）：程序模拟一次复制，捕获到的新内容直接写为**永久条目**；如果 600ms 内剪贴板没有变化（如未选中内容），则把最近一条临时条目转为永久。结果以系统通知提示。 |
| R4 | 指定快捷键弹出列表 | 全局热键（默认 `Alt+V`，可选接管 `Win+V`）在光标附近弹出无边框列表窗口；回车/双击把条目粘贴回原窗口。 |
| R5 | 取代 `Win+V` | `Win+V` 由系统 Explorer 占用，普通 `RegisterHotKey` 注册不上。设置里提供开关：写入 `HKCU\...\Explorer\Advanced\DisabledHotkeys` 释放 `V`，需重启 Explorer 生效；关闭开关时恢复原值。 |
| R6 | 列表中管理条目 | 删除临时/永久条目、临时转永久、永久转临时（重新开始计时）、搜索、预览、纯文本粘贴、清空临时区。 |
| R7 | 多 Tab | `临时` / `永久` 两个 Tab，各自显示数量；搜索在当前 Tab 内生效。 |
| R8 | 开机启动 | 设置开关，写 `HKCU\...\Run`（`tauri-plugin-autostart`）。 |
| R9 | 关闭最小化到托盘 | 主窗口关闭按钮只隐藏窗口；托盘图标常驻。 |
| R10 | 托盘右键菜单 | 打开历史、暂停记录、立即同步、设置、开机启动（勾选）、退出。左键单击打开历史。 |
| R11 | SQLite 保存 | `%APPDATA%\AnyPaste\anypaste.db`，WAL 模式；图片存为文件，库中只存路径与缩略图。 |
| R12 | 可选 WebDAV / S3 云存储 | 理解为**两种可选后端**：WebDAV（坚果云、Nextcloud 等）或 S3 兼容存储桶（AWS S3、Cloudflare R2、MinIO、阿里云 OSS 等）。默认只同步永久条目，支持客户端加密。 |

### 1.1 已做的假设（可调整）

1. **平台**：首版只做 Windows 10/11（x64），架构上保留跨平台可能。
2. **内容类型**：首版支持纯文本、富文本（同时保存 HTML/RTF，默认按原格式粘贴）、图片、文件列表。
3. **云同步范围**：只同步永久条目；临时条目只在本机，属于隐私与流量的考虑。
4. **永久复制默认热键 `Ctrl+Shift+C`**：在浏览器开发者工具、终端中有冲突，因此可配置，且对终端类进程做特殊处理（见风险 4）。
5. **隐私**：遵循 Windows 约定，跳过带有 `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory=0` 标记的内容（KeePass、1Password 等密码管理器会设置）；另提供"忽略的应用"列表。

### 1.2 待你确认的问题

1. 技术栈是否接受 **Tauri 2 + Rust + Svelte**（见第 2 节取舍）？
2. 云同步是"多设备双向同步"还是"单向备份"？本计划按**双向同步**设计，单向备份是其子集。
3. 临时条目是否需要数量上限（如最多 500 条，超出删最旧）？本计划默认 500，可配置。
4. 是否需要多语言？本计划只做简体中文，文案集中管理便于以后扩展。

## 2. 技术选型

| 方案 | 安装包 | 内存 | UI 美观度 | 开发效率 | 结论 |
|---|---|---|---|---|---|
| **Tauri 2 + Rust + Svelte** | 约 4–6 MB | 约 40–70 MB（WebView2） | 高，CSS 自由度大，可配 Mica | 高 | **推荐** |
| C# + WinUI 3 / WPF | 10–80 MB（取决于是否自包含） | 约 50–90 MB | 原生 Fluent | 中 | 备选 |
| Rust + egui / Slint | 约 3–5 MB | 约 15–30 MB | 中，定制成本高 | 中低 | 极致小巧时考虑 |
| Electron | 80 MB+ | 150 MB+ | 高 | 高 | 不满足"小巧" |

选 Tauri 2 的理由：安装包小；WebView2 已随 Win10/11 预装；系统托盘、全局热键、开机启动、单实例都有官方插件；底层剪贴板、`SendInput`、注册表操作可以直接用 `windows` crate 写。

### 2.1 依赖清单

| 用途 | 依赖 |
|---|---|
| 桌面框架 | `tauri 2`、`tauri-plugin-global-shortcut`、`tauri-plugin-autostart`、`tauri-plugin-single-instance` |
| Win32 能力 | `windows`（剪贴板监听、读写格式、`SendInput`、前台窗口、注册表） |
| 数据库 | `rusqlite`（`bundled` + FTS5）、`refinery` 或手写迁移 |
| 窗口效果 | `window-vibrancy`（Mica / Acrylic） |
| 云存储 | `opendal`（同一套接口同时支持 WebDAV 与 S3） |
| 加密 | `aes-gcm` + `argon2`（由同步口令派生密钥） |
| 其他 | `tokio`、`serde`、`blake3`（内容哈希）、`image`（缩略图）、`tracing` |
| 前端 | Svelte 5 + TypeScript + Vite，不引入 UI 组件库 |

## 3. 架构设计

```
┌──────────────────────────── Rust 核心 (src-tauri) ────────────────────────────┐
│ clipboard/watcher   消息窗口 + AddClipboardFormatListener → 读取格式 → ClipEvent │
│ clipboard/writer    写回剪贴板（带 AnyPaste 私有标记，避免自己再被记录）       │
│ input/paste         恢复前台窗口 → SendInput Ctrl+V / 模拟复制                 │
│ hotkey              全局热键注册、冲突检测、Win+V 接管（注册表）              │
│ store               SQLite：clips / settings / sync_state，FTS5 搜索           │
│ janitor             定时清理过期临时条目、孤立图片文件                        │
│ sync                OpenDAL(WebDAV|S3) 增量上传/拉取、墓碑、加密               │
│ tray / windows      托盘菜单、弹出窗、设置窗生命周期                          │
└──────────────── Tauri commands / events ─────────────────────────────────────┘
┌──────────────────────────── 前端 (src) ──────────────────────────────────────┐
│ popup.html   弹出列表：Tab、搜索、键盘导航、右键菜单、预览                    │
│ settings.html 设置：常规 / 快捷键 / 存储 / 云同步 / 关于                       │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 关键流程

**普通复制（临时）**
1. 收到 `WM_CLIPBOARDUPDATE` → 等待 80ms 合并后续通知（OLE 程序一次复制会先写入再 flush，触发两次）→ 剪贴板序列号未变则跳过。
2. 如有 AnyPaste 私有标记或隐私标记，跳过。
3. 按优先级读取格式：文件列表 > 文本（附带 HTML/RTF 时为富文本）> 图片（`PNG` 格式优先，其次 `CF_DIB`）。文本优先于图片，因为 Word、Excel 复制文字时也会附带位图。
4. 计算 `blake3` 哈希（文本类按文字内容，纯文本与富文本视为同一条；图片按像素）→ 已存在则刷新 `last_copied_at`、`expires_at` 并置顶，格式以本次复制为准（已是永久的保持永久）；不存在则插入临时条目。
5. 推送 `clip-added` 事件给前端（M2）。

**永久复制**
1. 热键触发 → 记录当前剪贴板序列号 `GetClipboardSequenceNumber`，设置"待永久"标记（流程进行中再次按热键忽略）。
2. 等待修饰键释放 → `SendInput` 模拟复制（终端类进程见风险 4）。
3. 600ms 内序列号变化 → 监听线程采集到内容时消费标记，直接写为永久（暂停记录时也照常保存）；没变化 → 把最近一条临时条目转为永久。
4. 系统通知提示结果：已永久保存 / 已将最近一条设为永久 / 未保存及原因。

**弹出与粘贴**
1. 热键触发 → 记录 `GetForegroundWindow()` → 在光标附近显示预先创建好的隐藏窗口（目标 < 100ms）。
2. 选中条目 → 隐藏弹窗 → `SetForegroundWindow(原窗口)` → 写剪贴板（带私有标记）→ `SendInput Ctrl+V`。
3. 写入的私有标记让监听器只刷新该条目时间，不产生新条目。

### 3.2 数据模型

```sql
CREATE TABLE clips (
  id             INTEGER PRIMARY KEY,
  uuid           TEXT NOT NULL UNIQUE,          -- 同步用全局 ID
  kind           TEXT NOT NULL,                 -- text | rich | image | files
  content_hash   TEXT NOT NULL UNIQUE,
  text           TEXT,                          -- 纯文本 / 文件路径（换行分隔）/ 图片 OCR 预留
  html           TEXT,
  rtf            TEXT,
  image_file     TEXT,                          -- 图片文件名，位于数据目录 images/ 下
  thumb          BLOB,                          -- 小缩略图，列表直接显示
  size_bytes     INTEGER NOT NULL,
  source_app     TEXT,                          -- 进程名，如 chrome.exe
  is_permanent   INTEGER NOT NULL DEFAULT 0,
  created_at     INTEGER NOT NULL,              -- 毫秒时间戳
  last_copied_at INTEGER NOT NULL,
  expires_at     INTEGER,                       -- 永久条目为 NULL
  updated_at     INTEGER NOT NULL,              -- 同步冲突判定
  deleted_at     INTEGER                        -- 墓碑；仅已同步的永久条目使用
);
CREATE INDEX idx_clips_list ON clips(is_permanent, deleted_at, last_copied_at DESC);
CREATE INDEX idx_clips_expire ON clips(expires_at) WHERE is_permanent = 0;
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- M2 迁移：全文搜索
CREATE VIRTUAL TABLE clips_fts USING fts5(text, content='clips', content_rowid='id', tokenize='trigram');
-- M5 迁移：同步状态
CREATE TABLE sync_state (key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- 上次同步时间、远端游标
```

- 迁移脚本位于 `src-tauri/src/store/migrations/`，版本号记录在 `PRAGMA user_version`。
- 中文搜索用 FTS5 `trigram` 分词，少于 3 个字符时退化为 `LIKE`。
- 单条内容上限默认 10 MB（可配置），超出则不记录并写日志。
- 采集相关设置项：`temp_ttl_secs`（默认 86400）、`temp_max_items`（默认 500，0 为不限）、`max_item_bytes`（默认 10 MB）。

### 3.3 云同步设计

- **存储布局**（WebDAV 与 S3 共用，经 OpenDAL 抽象；`<root>` 为用户填写的远端目录或路径前缀）：
  ```
  <root>/
    meta.json                        # 格式版本、加密参数（盐、口令校验串）
    clips/<id>.<version>.rec         # 条目内容（JSON，加密时为密文）
    clips/<id>.<version>.del         # 删除标记
    blobs/<id>                       # 图片 PNG，按内容只上传一次
  ```
  `id` 为内容哈希（加密时为带密钥的哈希，避免泄露"是否包含某段已知内容"），`version` 为更新时间。以内容而不是 uuid 标识条目，两台设备各自复制了相同内容时自然合并为一条，不会出现两个 uuid 来回覆盖。
- **合并**：每次修改写入新对象而不覆盖旧对象，同一条目取版本最大者（后写入者胜出），旧版本在同步时删除；多台设备同时写入不会互相覆盖，也不需要共享索引文件。
- **删除**：同步开启后删除永久条目留下本地墓碑并上传删除标记；永久转临时也视为从同步中删除。墓碑与删除标记 30 天后清理。
- **加密**：可选。口令经 Argon2id 派生 64 字节：前 32 字节作 AES-256-GCM 密钥（每个对象随机 nonce），后 32 字节作对象名的哈希密钥。口令错误、云端加密状态与本机设置不一致时直接报错，不修改云端。
- **触发**：启动后同步一次；按设置间隔（默认 15 分钟）定时同步；永久条目变化后 30 秒防抖同步（本地指纹未变化时跳过）；托盘与设置窗口"立即同步"。单次同步 10 分钟超时。
- **凭据**：WebDAV 密码、S3 Secret Key、同步口令存 Windows 凭据管理器，不写入数据库，也不回传到界面。
- **已知限制**：被删除条目引用的云端图片暂不回收；依赖各设备时钟大致准确。

## 4. 功能清单与优先级

| 优先级 | 功能 |
|---|---|
| P0（MVP） | 剪贴板监听；文本/图片/文件记录；去重置顶；24h 过期清理；弹出列表；临时/永久 Tab；搜索；键盘导航；回车粘贴；删除；临时转永久；永久复制热键；托盘与菜单；关闭到托盘；开机启动；单实例 |
| P1 | 设置窗口全部项；热键自定义与冲突检测；接管 `Win+V`；富文本粘贴/纯文本粘贴；忽略应用列表；暂停记录；明暗主题跟随系统；Mica 背景 |
| P2 | WebDAV / S3 同步；客户端加密；导入导出；数据目录迁移 |
| P3 | 永久条目分组/标签；片段模板；OCR；自动更新 |

## 5. 开发里程碑

| 阶段 | 内容 | 预估 | 验收标准 |
|---|---|---|---|
| M0 脚手架 | Tauri 2 + Svelte + TS 工程、单实例、托盘、日志、目录约定、CI 构建 | 0.5 天 | `npm run tauri dev` 启动后只有托盘图标；重复启动只唤起已有实例 |
| M1 采集与存储 | 剪贴板监听、格式读取、哈希去重、SQLite 与迁移、过期清理、隐私标记过滤 | 2 天 | 复制文本/图片/文件后库中出现对应条目；重复复制只刷新时间；把 TTL 设为 1 分钟可观察到自动清理；KeePass 复制的密码不被记录 |
| M2 弹窗与粘贴 | 弹出窗口、Tab、搜索、键盘导航、右键菜单、粘贴回原窗口、自身写入不重复记录 | 3 天 | 在记事本、浏览器、VS Code、Word 中 `Alt+V` → 回车可正确粘贴；弹窗出现 < 100ms；列表 1 万条时滚动流畅 |
| M3 永久复制 | 热键模拟复制、序列号检测、回退为"转永久最近一条"、终端特殊处理 | 1 天 | 选中文本按热键后条目出现在永久 Tab；Windows Terminal 中不会触发 `Ctrl+C` 中断 |
| M4 设置与系统集成 | 设置窗口、热键录制与冲突提示、开机启动、`Win+V` 接管与恢复、忽略应用、暂停记录、主题 | 2 天 | 所有设置重启后保持；修改保留时长后已有临时条目按新时长重算过期时间；关闭 `Win+V` 接管后系统剪贴板历史恢复 |
| M5 云同步 | OpenDAL 接入 WebDAV/S3、连接测试、增量同步、墓碑、加密、状态展示 | 3 天 | 两台机器指向同一存储，永久条目新增/删除/修改 1 分钟内一致；口令错误时给出明确提示且不覆盖远端 |
| M6 打磨与发布 | 空态/错误态、动画、性能、NSIS 安装包、便携版、README | 1.5 天 | 安装包 < 8 MB；空闲内存 < 80 MB；冷启动 < 1s |

合计约 13 个工作日。M0–M3 完成即可日常使用（MVP）。

## 6. 主要风险与对策

| # | 风险 | 对策 |
|---|---|---|
| 1 | `Win+V` 被系统占用 | 注册表 `DisabledHotkeys` 方案，需重启 Explorer；设置里明确提示并提供一键恢复。备选：低级键盘钩子拦截，但可能被安全软件拦截，仅作兜底。 |
| 2 | 粘贴回原窗口失败（焦点被系统阻止） | 弹窗显示前记录前台窗口；用 `AttachThreadInput` + `SetForegroundWindow`；以管理员运行的目标窗口无法注入时提示"已复制到剪贴板，请手动粘贴"。 |
| 3 | 剪贴板被其他程序占用（`OpenClipboard` 失败） | 指数退避重试 5 次；延迟渲染格式按需读取。 |
| 4 | 永久复制模拟 `Ctrl+C` 在终端里变成中断信号 | 维护进程列表：`WindowsTerminal.exe`、`conhost.exe` 等改发 `Ctrl+Insert`（终端、编辑器、浏览器通用的复制键，不会中断命令；`Ctrl+Shift+C` 会被自身热键拦截，不可用）；列表可在设置中编辑。 |
| 5 | 大图片 / 大文件撑大数据库 | 图片落盘为文件，库只存缩略图；单条大小上限；文件列表只存路径不复制文件。 |
| 6 | 云端泄露 | 默认只同步永久条目；推荐开启客户端加密；凭据存凭据管理器。 |
| 7 | WebView2 内存偏高（一个常驻弹窗约 180 MB 私有内存，主进程仅约 5 MB） | 弹窗按需创建，隐藏超过设定时间（默认 1 分钟）即销毁，WebView2 进程随之退出，空闲内存约 4–8 MB；代价是释放后再打开约 0.7 s。设置中可改为常驻（启动即预创建，打开约 90 ms）。设置窗关闭即销毁。 |

## 7. 目录规划

```
AnyPaste/
├─ docs/PLAN.md
├─ prototype/index.html          # 交互原型（纯静态，可直接用浏览器打开）
├─ src/                          # 前端 Svelte
│  ├─ popup/   settings/   lib/
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs  app.rs  tray.rs  windows.rs
│  │  ├─ clipboard/{watcher.rs, reader.rs, writer.rs}
│  │  ├─ input/{paste.rs, copy.rs}
│  │  ├─ hotkey/{mod.rs, winv.rs}
│  │  ├─ store/{mod.rs, migrations/, clips.rs, settings.rs}
│  │  ├─ janitor.rs
│  │  └─ sync/{mod.rs, backend.rs, crypto.rs, merge.rs}
│  └─ tauri.conf.json
└─ README.md
```

## 8. 测试策略

- **Rust 单元测试**：去重与置顶、过期计算、临时/永久转换、FTS 搜索、同步合并（冲突、墓碑）、加解密往返。数据库用内存 SQLite。
- **同步集成测试**：本地起 MinIO 与 WebDAV 容器（docker compose），只用测试桶与临时数据。
- **手工验收清单**：热键在常见应用（记事本、Chrome、VS Code、Word、Windows Terminal、管理员权限窗口）中的复制/粘贴表现；多显示器、高 DPI 下弹窗位置；明暗主题。
