# AnyPaste

本地剪贴板历史工具，目标是小巧、美观、可替代 Windows 自带的 `Win+V`。

- 需求与开发计划：[docs/PLAN.md](docs/PLAN.md)
- 交互原型：[prototype/index.html](prototype/index.html)（浏览器直接打开）

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

## 检查与测试

```bash
npm run check
npm run build
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Rust 编译会嵌入前端产物 `dist/`，因此 clippy 和测试之前需要先执行 `npm run build`。

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
| `src/lib/` | 前端共享：命令封装 `api.ts`、主题令牌 `theme.css` |
| `src-tauri/src/` | Rust 核心：托盘、窗口、命令、数据目录、运行时状态 |
| `src-tauri/src/clipboard/` | 剪贴板监听、格式读取、隐私标记过滤、入库前处理 |
| `src-tauri/src/store/` | SQLite 存储、迁移脚本、采集设置 |
| `src-tauri/src/janitor.rs` | 过期与超量临时条目清理、孤立图片清理 |

## 数据位置

| 内容 | 路径 |
|---|---|
| 数据目录 | `%APPDATA%\AnyPaste\` |
| 数据库 | `%APPDATA%\AnyPaste\anypaste.db`（SQLite，WAL 模式） |
| 图片 | `%APPDATA%\AnyPaste\images\` |
| 日志 | `%APPDATA%\AnyPaste\logs\anypaste.log`（单文件上限 1 MB，保留一份历史） |
