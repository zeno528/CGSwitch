<p align="center">
  <img src="public/logo.png" width="120" alt="CGSwitch" />
</p>

<h1 align="center">CGSwitch</h1>

<p align="center">一款管理 Codex / ChatGPT 桌面应用模型配置的 Windows/macOS 工具：一键切换模型，自动检测运行状态、一键重启应用，无需手动操作。</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="License" /></a>
  <img src="https://img.shields.io/badge/Vue_3-4FC08D?style=flat-square&logo=vuedotjs&logoColor=white" alt="Vue 3" />
  <img src="https://img.shields.io/badge/TypeScript-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Vite-646CFF?style=flat-square&logo=vite&logoColor=white" alt="Vite" />
  <img src="https://img.shields.io/badge/Tauri_2-24C8DB?style=flat-square&logo=tauri&logoColor=white" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Tailwind_CSS-06B6D4?style=flat-square&logo=tailwindcss&logoColor=white" alt="Tailwind CSS" />
  <img src="https://img.shields.io/badge/CodeMirror_6-D30707?style=flat-square&logo=codemirror&logoColor=white" alt="CodeMirror 6" />
</p>

## 功能

- 配置档案管理：捕获 `~/.codex/config.toml` 当前状态、一键添加内置官方模板（DeepSeek / 智谱 / MiniMax / ChatGPT）、切换应用
- 应用内全量编辑：CodeMirror 高亮编辑档案的 `config.toml` 与 `models.json`，未生效档案也可直接编辑，保存时做 TOML / JSON 校验，应用时才写入 `~/.codex`
- 档案操作：重命名、复制（自动追加“副本”后缀）、官网地址跳转、连通性测试、图标自定义
- 写入安全：使用 `toml_edit` 原子修改，保留 MCP、插件、注释等无关内容；每次写入前自动备份 `config.toml` 与关联文件
- 切换后自动重启官方 Codex / ChatGPT 桌面应用（可配置开关与超时）
- ChatGPT 账号 OAuth 登录管理
- 数据备份：数据库导入 / 导出、备份列表恢复与删除
- 桌面体验：深色 / 浅色主题、开机自启、关闭时最小化到托盘、静默启动

## 数据位置

```text
~/.cgswitch/
├── cgswitch.db
├── backups/
│   ├── config/
│   ├── database/
│   └── codex-files/
└── logs/
```

- Windows：`C:\Users\<user>\.cgswitch`
- macOS：`/Users/<user>/.cgswitch`

数据库保存档案、设置和切换事件。provider token 只保留在本机，不要把 `.cgswitch/`、日志或数据库提交到 Git。

## 开发

```bash
pnpm install
pnpm dev:tauri
```

`pnpm dev:tauri` 遵循 Tauri 2 官方开发流程：Tauri CLI 先执行 `beforeDevCommand: pnpm dev` 启动 Vite，再连接 `http://localhost:5173` 并启动 Rust debug 版应用。Vue/CSS 修改由 Vite HMR 生效；Rust 源码变更后让 Tauri dev 重新编译。停止开发会话使用 `Ctrl+C`。

### 调试流程

1. 安装依赖并启动开发应用（见上）。
2. 确认窗口、配置档案页、设置页、明暗主题和错误提示可用。
3. 调试 UI 时保持 Tauri dev 进程运行；浏览器控制台 / WebView console 输出在 debug 版可用。
4. 需要可独立运行的调试包时执行：

   ```bash
   pnpm build:debug
   ```

   产物在 `src-tauri/target/debug/bundle/`。

VS Code 调试配置使用 Tauri 官方推荐的 Cargo + CodeLLDB + Vite dev server 流程，见 `.vscode/tasks.json` 与 `.vscode/launch.json`。

### 发布前检查

```bash
pnpm typecheck
pnpm build
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo test
```

以上全部通过，且人工确认当前版本可以发布后，才执行 `pnpm tauri build`。不要跳过 `pnpm dev:tauri` 的人工调试直接做发布构建。

## 构建

```bash
pnpm tauri build
```

发布目标：

- Windows x64 NSIS 安装包
- macOS x64 DMG
- macOS Apple Silicon DMG

V1 安装包未签名。macOS Gatekeeper 拦截时，请确认安装包来源，再按系统说明放行。

## 致谢与许可证

本项目采用 `MIT` 协议。供应商图标来自 [thesvg.org](https://thesvg.org)，各 SVG 文件头已保留来源声明。


## 感谢

项目目前处于测试阶段，欢迎[反馈问题与建议](https://github.com/zeno528/CGSwitch/issues)。
