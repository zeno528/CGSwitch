---
name: local-windows-build
description: 快速本地编译 SwitchGPT 的 Windows 安装包（NSIS exe / MSI），构建完成后打开产物目录并列出安装包路径，供用户直接安装测试，不用等 GitHub Actions 发行工作流。当用户说"本地编译"、"本地构建"、"打个本地包"、"本地出个安装包"、"构建本地测试版"时使用。
---

# 本地快速编译 Windows 版

在用户本机直接编译当前代码的 Windows 安装包，跳过 GitHub Actions 发行工作流（冷编译约 20 分钟，本地增量通常 1 分钟内）。构建的是**当前工作区状态**——包括未提交的改动，适合快速验证。

## 执行步骤

### Step 1: 快速预检

运行 `pnpm typecheck`（约 10 秒）。失败就直接停下报告错误，不要进入几分钟的编译；用户修完类型错误再重来。

Rust 侧不用预检——`pnpm tauri build` 本身会编译，有错会当场报。

### Step 2: 构建

在项目根目录运行：

```powershell
pnpm tauri build
```

- 用 PowerShell 工具执行，timeout 设 600000（10 分钟）
- 成功标志：输出 `Finished 2 bundles at:` 并列出两个路径
- 该命令自动完成：sync-version（VERSION → 三处版本号同步）→ vite build → cargo release 编译 → NSIS + MSI 打包

### Step 3: 确认产物并打开目录

```powershell
Get-ChildItem "src-tauri/target/release/bundle/nsis/*.exe", "src-tauri/target/release/bundle/msi/*.msi" | Select-Object Name, @{n='SizeMB';e={[math]::Round($_.Length/1MB,1)}}, LastWriteTime
Invoke-Item "src-tauri/target/release/bundle/nsis"
```

### Step 4: 按此格式报告

| 产物 | 路径 | 说明 |
|:---|:---|:---|
| NSIS 安装包（推荐） | `src-tauri\target\release\bundle\nsis\SwitchGPT_<版本>_x64-setup.exe` | 标准安装体验 |
| MSI 安装包 | `src-tauri\target\release\bundle\msi\SwitchGPT_<版本>_x64_en-US.msi` | 备选 |
| 绿色版 | `src-tauri\target\release\switchgpt.exe` | 免安装直接运行 |

版本号取自根目录 `VERSION` 文件。提醒用户：要测新版本号就先改 `VERSION` 再跑本 skill。

## 注意事项

- 构建产物未签名，Windows SmartScreen 可能提示——点"仍要运行"
- 本地只出 Windows 包；macOS 包仍走 GitHub Actions 的 Release 工作流
- 产物文件名固定带版本号，重复构建会覆盖同名旧文件
- 安装新版本会直接覆盖安装旧版本，无需先卸载

## 常见问题

**构建失败，报 Rust 编译错误**
看错误第一行的文件和行号，多为当前代码问题；修完重新执行 Step 2 即可，增量编译很快。

**`pnpm` / `cargo` 命令找不到**
确认在项目根目录、Node/Rust 工具链已装；必要时先 `pnpm install`。

**第一次构建很慢（10-20 分钟）**
正常——冷编译约 600 个 crate。之后 `src-tauri/target` 是热的，增量构建通常 1 分钟内。

**typecheck 过了但 tauri build 在 vite 阶段失败**
看 vite 报错的具体文件；`pnpm build` 可以单独复现该阶段。
