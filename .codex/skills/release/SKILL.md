---
name: release
description: CGswitch 发版流水线（本地部分）：确定版本号、撰写发行日志、打 tag 推送触发 Release 工作流构建三平台资产、盯构建进度、展示资产与日志供确认后发布。当用户说"发版"、"发行"、"release"、"发个新版本"、"发布新版本"时使用。
---

# CGswitch 发版

分工：本 skill 做本地部分（版本号、发行日志、tag、盯构建、最终发布）；`.github/workflows/release.yml` 由 tag 推送触发，只做构建、建草稿和上传资产。草稿不会通知关注者；执行发布那一刻 GitHub 才给关注者发通知邮件。

前置条件：当前分支必须是 main 且与远端同步（发行 tag 必须打在 main 上）。不满足时停下，提醒用户先合并/推送，不要自行切换分支。

## Instructions

### Step 1: 确定版本

1. 取最新 tag：`git tag -l 'v*' | sort -V | tail -1`
2. 读 `VERSION` 文件。
3. 若 `VERSION` 已大于最新 tag（用户或工具已提前 bump），直接使用，不要重复 bump。
4. 否则执行 `node scripts/bump-version.mjs patch`（用户明说 minor/major 时用对应级别；拿不准时默认 patch 并在汇报里说明）。
5. 刷新锁文件里的包版本：`cargo update -p cgswitch --manifest-path src-tauri/Cargo.toml`

### Step 2: 撰写发行日志

1. 查看自上一 tag 以来的提交：`git log <上一tag>..HEAD --oneline --no-merges`（首个版本用全部历史）。
2. 写 `docs/release-notes/v<版本>.md`，模板：

```markdown
# CGswitch v<版本>

### 新增
- …

### 修复
- …

### 界面与样式
- …
```

3. 写作规则：
   - 用用户视角描述变更（"新增 xxx 功能"），不要照抄 commit 标题。
   - 空分区整节省略；可用分区：新增 / 修复 / 界面与样式 / 性能优化 / 重构 / 移除 / 安全。
   - 版本号 bump、纯 CI/工作流、纯文档类提交不进日志。
   - 已有 changelog 风格时（查看 `docs/release-notes/` 旧文件），沿用旧格式。
4. 提交所有发版文件：`git add VERSION package.json src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json docs/release-notes/`
   提交信息：`chore(release): v<版本>`

### Step 3: 打 tag 并推送

推送前先跑 `pnpm check`（与 Release 工作流 verify job 同一条链），失败就地修复并补充提交；全绿再继续——避免 verify 阶段失败浪费整轮三平台构建。

```
git tag v<版本>
git push origin main v<版本>
```

推送 tag 被拒绝（tag 已存在）→ 停止，报告版本冲突，从 Step 1 重新确定版本。

### Step 4: 盯 Release 工作流

1. 等 10 秒后取 run：`gh run list --workflow=Release --limit 1 --json databaseId,status,headSha`
2. `gh run watch <run-id> --exit-status --interval 30` 放后台执行（约 30-40 分钟），完成时会收到通知。
3. 构建失败：`gh run view <run-id> --log-failed` 提取报错摘要，报告用户并停止（草稿若已创建则留在草稿态，不影响关注者）。

### Step 5: 确认与发布

1. 展示给用户（这一步必须等用户明确确认，不得自动发布）：
   - `gh release view v<版本> --json name,isDraft,assets` 的资产清单（文件名 + 大小）
   - 发行日志全文预览
2. 用户确认后执行：`gh release edit v<版本> --draft=false --latest`
3. 变体处理：
   - 用户说"预发布"：加 `--prerelease`，去掉 `--latest`
   - 用户要改日志：改 `docs/release-notes/v<版本>.md`，执行 `gh release edit v<版本> --notes-file docs/release-notes/v<版本>.md` 更新草稿后再发布
4. 发布后告知用户：关注者通知已发出，附 release 页面链接 `https://github.com/zeno528/CGswitch/releases/tag/v<版本>`

## 示例

**场景**：用户说"发版"

1. 最新 tag `v0.4.3`，VERSION 为 0.4.4（已提前 bump）→ 直接用 0.4.4
2. `git log v0.4.3..HEAD --oneline --no-merges` 起草 `docs/release-notes/v0.4.4.md`
3. 提交 `chore(release): v0.4.4`，`git tag v0.4.4`，`git push origin main v0.4.4`
4. 后台 `gh run watch` 盯 Release 工作流至全绿
5. 展示 4 个资产（Windows setup/msi、macOS x64/arm64 dmg）+ 日志全文，等确认
6. 用户回复"发布" → `gh release edit v0.4.4 --draft=false --latest`，报告链接

## Troubleshooting

**tag 推送被拒（already exists）**：版本号与已有 tag 撞车。回到 Step 1 递增版本重来；已提交的发版 commit 需 `git reset --soft HEAD~1` 后重新 bump。

**工作流未触发**：确认推送的是 tag（不是只推了 main）；确认 `.github/workflows/release.yml` 的 tag 触发器已合入 main；`gh run list --workflow=Release` 查看队列。

**verify 第 0 步就失败（tag 与 VERSION 不一致 / 缺发行日志）**：说明 tag 打出时仓库状态不对。删掉远端 tag（`git push origin :refs/tags/v<版本>`）和本地 tag，修正后从 Step 3 重来。
