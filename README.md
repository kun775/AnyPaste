# AnyPaste

本地剪贴板历史工具，目标是小巧、美观、可替代 Windows 自带的 `Win+V`。

- 需求与开发计划：[docs/PLAN.md](docs/PLAN.md)
- 交互原型：[prototype/index.html](prototype/index.html)（浏览器直接打开）

## 使用

- 复制任意内容即自动记录为**临时条目**，到期（默认 24 小时）自动删除；重复复制同一内容只会置顶。
- `Ctrl+Shift+C` 复制选中内容并直接存为**永久条目**（不会过期）；没有选中内容时，把最近一条临时条目设为永久。结果以系统通知提示。终端类程序会改用 `Ctrl+Insert` 复制，避免中断正在运行的命令。
- `Alt+V` 在光标附近打开剪贴板历史，选中条目回车即粘贴回原窗口。从托盘打开时没有目标窗口，选中条目只复制到剪贴板。也可以用命令行 `AnyPaste.exe --popup` 打开，便于配合其他启动器或快捷方式。
- 为节省内存，弹出列表隐藏 1 分钟后会被释放（空闲时整个程序约 5 MB 内存），释放后再次打开需约 0.7 秒；可在设置中改为常驻以始终秒开（多占用约 150 MB）。

| 按键 | 作用 |
|---|---|
| `↑` `↓` / `PgUp` `PgDn` | 选择条目 |
| 单击条目 / `Enter` | 粘贴 |
| `Shift+Enter` | 以纯文本粘贴（去掉格式；文件列表粘贴为路径） |
| `Ctrl+1` … `Ctrl+9` | 直接粘贴第 1–9 条 |
| `Ctrl+P` | 设为永久 / 转为临时 |
| `Ctrl+T` | 置顶 / 取消置顶（置顶条目排在列表最前，只影响排序，不参与云同步） |
| `Delete` | 删除（永久条目需确认） |
| `Tab` | 切换临时 / 永久 |
| `Esc` | 清空搜索；再按一次关闭 |
| 输入文字 | 搜索（3 个字符以上走全文索引） |

图片条目显示缩略图，悬停时点「预览」按钮（或右键「预览原图」）可在弹窗内查看原图。弹窗顶部的拖动条和标签栏空白处可以拖动窗口。

### 设置

托盘菜单「设置…」打开设置窗口，修改即时生效并保存在数据库中：

- **常规**：开机启动、关闭窗口时最小化到托盘、暂停记录、永久复制通知、弹出位置、空闲后释放弹出列表、默认粘贴格式、主题。
- **快捷键**：自定义两个热键（冲突时自动恢复原热键）；接管 `Win+V`（修改注册表 `DisabledHotkeys` 并重启资源管理器后生效，关闭即恢复系统热键）；永久复制改用 `Ctrl+Insert` 的终端程序列表。
- **存储**：临时条目保留时长（修改后对已有条目生效）、数量上限、单条大小上限、记录的内容类型、忽略的应用、清空临时历史。
- **云同步**：把永久条目同步到 WebDAV（坚果云、Nextcloud 等）或 S3 兼容存储（AWS S3、Cloudflare R2、MinIO、阿里云 OSS 等）。可开启端到端加密，所有设备需使用相同的同步口令。密码与口令保存在 Windows 凭据管理器中。同步设计见 [docs/PLAN.md](docs/PLAN.md#33-云同步设计)。
- **关于**：版本信息、检查更新与一键安装、自动检查更新（每天一次，发现新版本时通知）、自动安装更新（默认关闭；便携版不支持），以及项目 GitHub 地址。更新包下载后会用内置公钥校验签名，校验失败不会安装。

## 环境要求

- Windows 10/11 x64（WebView2 运行时，系统一般已预装）
- Node.js 22+
- Rust stable（MSVC 工具链）

## 开发

```bash
npm install
npm run tauri dev
```

启动后只有托盘图标：左键单击打开剪贴板历史，右键打开菜单。重复启动不会产生第二个实例，而是打开已运行实例的设置窗口。

调试界面时，可以在普通浏览器中打开 `http://localhost:1420/popup.html?mock` 或 `settings.html?mock`，使用内置的模拟数据，无需启动 Tauri（仅开发模式生效，不会进入生产包）。

## 检查与测试

```bash
npm run check
npm run build
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Rust 编译会嵌入前端产物 `dist/`，因此 clippy 和测试之前需要先执行 `npm run build`。

读写真实系统剪贴板的测试默认跳过（会临时覆盖剪贴板内容，结束后恢复文本），需要时单独运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --test-threads=1 clipboard
```

同步引擎的单元测试使用内存存储模拟多台设备。针对真实 WebDAV 服务器的测试默认跳过，设置环境变量 `ANYPASTE_TEST_WEBDAV`（以及可选的 `ANYPASTE_TEST_WEBDAV_USER`、`ANYPASTE_TEST_WEBDAV_PASS`）后运行，测试数据写在随机目录中并在结束时删除：

```bash
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored webdav
```

## 打包

```bash
npm run tauri build -- --bundles nsis
```

安装包输出到 `src-tauri/target/release/bundle/nsis/`（按当前用户安装，无需管理员权限）。

**便携版**：把 `src-tauri/target/release/anypaste.exe` 放进任意目录，并在同一目录新建一个名为 `portable` 的空文件，数据就会保存在该目录的 `data` 子目录中。CI 会同时产出 `AnyPaste-portable-x64.zip`。

## 发布

发布由 [release 工作流](.github/workflows/release.yml) 完成：构建并签名安装包、创建 GitHub Release（附安装包、签名、便携版与 `latest.json`），并把 `latest.json` 上传到固定标签 `updater` 的发布中，供程序检查更新。

1. 把 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 中的版本号改为新版本并提交。
2. 创建附注标签，标签内容即发布说明，然后推送：

   ```bash
   git tag -a v0.3.0 --cleanup=verbatim -F notes.md
   git push origin master v0.3.0
   ```

   标签名含 `-`（如 `v0.3.0-beta.1`）时发布为预发布版。`--cleanup=verbatim` 不能省略，否则 Markdown 标题（以 `#` 开头的行）会被当作注释删掉。

**签名密钥**：更新包用 Tauri updater 的密钥签名，公钥写在 `tauri.conf.json` 中，私钥保存在仓库 Secret `TAURI_SIGNING_PRIVATE_KEY`（无口令）。私钥一旦丢失，已安装的版本将无法再自动更新，请妥善备份。本地签名构建：

```bash
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/anypaste-updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" npm run tauri build -- --bundles nsis --config src-tauri/tauri.release.conf.json
```

## 目录

| 路径 | 说明 |
|---|---|
| `popup.html`、`src/popup/` | 弹出列表窗口（常驻隐藏，热键唤出） |
| `settings.html`、`src/settings/` | 设置窗口（按需创建，关闭即销毁） |
| `src/lib/` | 前端共享：命令封装 `api.ts`、主题令牌 `theme.css`、开发用模拟后端 `devMock.ts` |
| `src-tauri/src/` | Rust 核心：托盘、窗口、命令、热键、事件、数据目录、运行时状态 |
| `src-tauri/src/clipboard/` | 剪贴板监听、格式读取与写回、隐私标记过滤、入库前处理 |
| `src-tauri/src/input.rs` | 记录前台窗口、模拟 Ctrl+V 粘贴与复制 |
| `src-tauri/src/settings.rs`、`prefs.rs` | 设置模型与校验；设置变更的统一入口（热键、托盘、过期时间同步） |
| `src-tauri/src/hotkey.rs`、`winv.rs`、`permanent.rs` | 全局热键、接管 Win+V、永久复制流程 |
| `src-tauri/src/updater.rs` | 检查与安装更新、定时检查 |
| `src-tauri/src/sync/` | 云同步：配置与凭据、后台调度（`mod.rs`）、同步引擎（`engine.rs`）、加密（`crypto.rs`） |
| `src-tauri/src/store/` | SQLite 存储、迁移脚本、采集设置 |
| `src-tauri/src/janitor.rs` | 过期与超量临时条目清理、孤立图片清理 |

## 数据位置

| 内容 | 路径 |
|---|---|
| 数据目录 | `%APPDATA%\AnyPaste\` |
| 数据库 | `%APPDATA%\AnyPaste\anypaste.db`（SQLite，WAL 模式） |
| 图片 | `%APPDATA%\AnyPaste\images\` |
| 日志 | `%APPDATA%\AnyPaste\logs\anypaste.log`（单文件上限 1 MB，保留一份历史） |
| 凭据 | Windows 凭据管理器中名为 `AnyPaste` 的条目（云同步密码与口令） |

便携模式下以上目录都位于程序目录的 `data` 子目录中。
