# SwitchGPT

SwitchGPT 是一个轻量的 Windows / macOS 桌面工具，用于捕获、切换 OpenAI Codex 的模型配置档案，并在切换后重启官方 Codex / ChatGPT 桌面应用。

## 功能

- 捕获 `~/.codex/config.toml` 中的模型、推理强度与 provider 配置
- 使用 `toml_edit` 原子修改配置，保留 MCP、插件、注释等无关内容
- 为每次写入自动保留 `config.toml` 备份
- 识别并重启官方 Codex / ChatGPT 桌面应用
- 深色 / 浅色界面与独立设置页

## 数据位置

```text
~/.switchgpt/
├── switchgpt.db
├── backups/
│   ├── config/
│   └── database/
└── logs/
```

- Windows：`C:\Users\<user>\.switchgpt`
- macOS：`/Users/<user>/.switchgpt`

数据库保存档案、设置和切换事件。provider token 只保留在本机，不要把 `.switchgpt/`、日志或数据库提交到 Git。

## 开发

```bash
pnpm install
pnpm dev:tauri
```

`pnpm dev:tauri` 遵循 Tauri 2 官方开发流程：Tauri CLI 先执行 `beforeDevCommand: pnpm dev` 启动 Vite，再连接 `http://localhost:5173` 并启动 Rust debug 版应用。Vue/CSS 修改由 Vite HMR 生效；Rust 源码变更后让 Tauri dev 重新编译。停止开发会话使用 `Ctrl+C`。

### 调试流程

1. 安装依赖并启动开发应用：

   ```bash
   pnpm install
   pnpm dev:tauri
   ```

2. 确认窗口、配置档案页、设置页、明暗主题和错误提示可用。
3. 调试 UI 时保持 Tauri dev 进程运行；浏览器控制台/WebView console 输出在 debug 版可用。
4. 需要可独立运行的调试包时执行：

   ```bash
   pnpm build:debug
   ```

   产物在 `src-tauri/target/debug/bundle/`。
5. 发布构建前必须先通过人工调试与自动检查：

   ```bash
   pnpm typecheck
   pnpm build
   cargo fmt --all -- --check
   cargo clippy -- -D warnings
   cargo test
   ```

6. 以上全部通过，且人工确认当前版本可以发布后，才执行 `pnpm tauri build`。不要跳过 `pnpm dev:tauri` 的人工调试直接做发布构建。

VS Code 调试配置使用 Tauri 官方推荐的 Cargo + CodeLLDB + Vite dev server 流程，见 `.vscode/tasks.json` 与 `.vscode/launch.json`。

Rust 检查：

```bash
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo test
```

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

感谢 [Codex++](https://github.com/BigPizzaV3/CodexPlusPlus)（AGPL-3.0）提供桌面应用进程重启与配置原子写入的实现思路参考，感谢 [LeagueAkari](https://github.com/LeagueAkari/LeagueAkari) 的界面主题思路启发。

本项目采用 `MIT` 协议。LeagueAkari 为 MIT；如复制其主题代码，需保留其版权声明。供应商图标来自 [thesvg.org](https://thesvg.org)（[glincker/thesvg](https://github.com/glincker/thesvg)，MIT），各 SVG 文件头已保留来源声明。

## 维护供应商图标

新增供应商：把 `<id>.svg` 放入 `src/assets/providers/`，图标会自动出现在档案编辑页的候选列表中；中文名称在 `src/icons.ts` 的 `LABELS` 里配置（缺省显示文件名）。
