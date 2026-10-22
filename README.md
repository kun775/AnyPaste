# AnyPaste

本地剪贴板历史工具，目标是小巧、美观、可替代 Windows 自带的 `Win+V`。

- 需求与开发计划：[docs/PLAN.md](docs/PLAN.md)
- 交互原型：[prototype/index.html](prototype/index.html)（浏览器直接打开）

## 使用

- 复制任意内容即自动记录为**临时条目**，到期（默认 24 小时）自动删除；重复复制同一内容只会置顶。
- `Alt+V` 在光标附近打开剪贴板历史，选中条目回车即粘贴回原窗口。从托盘打开时没有目标窗口，选中条目只复制到剪贴板。

| 按键 | 作用 |
|---|---|
| `↑` `↓` / `PgUp` `PgDn` | 选择条目 |
| `Enter` / 双击 | 粘贴 |
| `Shift+Enter` | 以纯文本粘贴（去掉格式；文件列表粘贴为路径） |
| `Ctrl+1` … `Ctrl+9` | 直接粘贴第 1–9 条 |
| `Ctrl+P` | 设为永久 / 转为临时 |
| `Delete` | 删除（永久条目需确认） |
| `Tab` | 切换临时 / 永久 |
| `Esc` | 清空搜索；再按一次关闭 |
| 输入文字 | 搜索（3 个字符以上走全文索引） |

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

调试弹窗界面时，可以在普通浏览器中打开 `http://localhost:1420/popup.html?mock`，使用内置的模拟数据，无需启动 Tauri（仅开发模式生效，不会进入生产包）。

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
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --test-threads=1
```

## 打包

```bash
npm run tauri build -- --bundles nsis
```

安装包输出到 `src-tauri/target/release/bundle/nsis/`。

## 目录

| 路径 | 说明 |
|---|---|
| `popup.html`、`src/popup/` | 弹出列表窗口（常驻隐藏，热键唤出） |
| `settings.html`、`src/settings/` | 设置窗口（按需创建，关闭即销毁） |
| `src/lib/` | 前端共享：命令封装 `api.ts`、主题令牌 `theme.css`、开发用模拟后端 `devMock.ts` |
| `src-tauri/src/` | Rust 核心：托盘、窗口、命令、热键、事件、数据目录、运行时状态 |
| `src-tauri/src/clipboard/` | 剪贴板监听、格式读取与写回、隐私标记过滤、入库前处理 |
| `src-tauri/src/input.rs` | 记录前台窗口、模拟 Ctrl+V 粘贴回原窗口 |
| `src-tauri/src/store/` | SQLite 存储、迁移脚本、采集设置 |
| `src-tauri/src/janitor.rs` | 过期与超量临时条目清理、孤立图片清理 |

## 数据位置

| 内容 | 路径 |
|---|---|
| 数据目录 | `%APPDATA%\AnyPaste\` |
| 数据库 | `%APPDATA%\AnyPaste\anypaste.db`（SQLite，WAL 模式） |
| 图片 | `%APPDATA%\AnyPaste\images\` |
| 日志 | `%APPDATA%\AnyPaste\logs\anypaste.log`（单文件上限 1 MB，保留一份历史） |
