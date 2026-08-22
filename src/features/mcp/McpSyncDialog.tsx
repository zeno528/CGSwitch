import { useEffect, useMemo, useState } from "react";
import { AppDialog } from "../../components/AppDialog";
import type { McpSyncDiffEntry, McpSyncPreview } from "../../types";

type SyncDirection = "live-to-db" | "db-to-live";
interface McpSyncDialogProps {
  open: boolean;
  preview: McpSyncPreview | null;
  previewError: string;
  busy: boolean;
  onClose: () => void;
  onApply: (direction: SyncDirection) => void;
}

const fieldLabels: Record<string, string> = { enabled: "启用状态", startup_timeout_sec: "启动超时秒", tool_timeout_sec: "工具超时秒", command: "启动命令", args: "启动参数", env: "环境变量", url: "服务地址", bearer_token_env_var: "令牌环境变量", http_headers: "HTTP 头", env_http_headers: "环境变量 HTTP 头" };
const valueText = (value: unknown) => value === null ? "未设置" : typeof value === "string" ? value : JSON.stringify(value);
const kindText = (entry: McpSyncDiffEntry) => entry.kind === "live_only" ? "外部新增" : entry.kind === "db_only" ? "配置文件缺失" : entry.unmodeled_only ? "仅格式差异" : "内容被修改";

export default function McpSyncDialog({ open, preview, previewError, busy, onClose, onApply }: McpSyncDialogProps) {
  const [step, setStep] = useState<"diff" | "confirm">("diff");
  const [direction, setDirection] = useState<SyncDirection | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  useEffect(() => { if (!open) { setStep("diff"); setDirection(null); setExpanded(new Set()); } }, [open]);
  const entries = preview?.entries ?? [];
  const pendingLines = useMemo(() => {
    const names = (kind: McpSyncDiffEntry["kind"]) => entries.filter((entry) => entry.kind === kind).map((entry) => entry.name);
    const parts = (values: string[]) => values.flatMap((value, index) => [index ? "、" : "", <code key={value} className="mono code-tok">{value}</code>]);
    if (!direction) return [];
    if (!preview) return [<span key="rebuild">该文件当前无法解析，将用数据库中的 MCP 配置重建整个 MCP 段。</span>, <span key="backup">执行前会自动备份 <code className="mono code-tok">~/.codex/config.toml</code>。</span>];
    const added = names("live_only"); const missing = names("db_only"); const changed = names("changed");
    const lines: React.ReactNode[] = [];
    if (direction === "db-to-live") {
      if (added.length) lines.push(<span key="added" className="font-semibold text-red-600 dark:text-red-400">配置文件有新增，从配置文件中删除：{parts(added)}</span>);
      if (missing.length) lines.push(<span key="missing">配置文件缺失，写入配置文件：{parts(missing)}</span>);
      if (changed.length) lines.push(<span key="changed">配置文件已修改，恢复为数据库内容：{parts(changed)}</span>);
      lines.push(<span key="backup">执行前会自动备份 <code className="mono code-tok">~/.codex/config.toml</code>。</span>);
    } else {
      if (added.length) lines.push(<span key="added">数据库新增：{parts(added)}</span>);
      if (changed.length) lines.push(<span key="changed">数据库修改：{parts(changed)}</span>);
      if (missing.length) lines.push(<span key="missing">数据库删除：{parts(missing)}</span>);
    }
    return lines;
  }, [direction, entries, preview]);

  const requestDirection = (next: SyncDirection) => { setDirection(next); setStep("confirm"); };
  const dialogDescription = step === "diff" && previewError
    ? "config.toml 无法解析，只能用数据库中的 MCP 配置恢复。"
    : undefined;
  const directionChoice = (next: SyncDirection) => {
    const dbToLive = next === "db-to-live";
    return (
      <button
        type="button"
        className="mcp-sync-choice apple-group flex w-full items-start justify-between gap-3 border-0 p-3 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-50"
        disabled={busy}
        onClick={() => requestDirection(next)}
        aria-label={dbToLive ? "用数据库更新 config.toml" : "用 config.toml 更新数据库"}
      >
        <span className="min-w-0">
          <span className="block font-semibold">{dbToLive ? "用数据库更新 config.toml" : "用 config.toml 更新数据库"}</span>
          <span className="muted mt-1 block text-xs leading-relaxed">
            {dbToLive ? previewError ? "配置文件无法解析；写入前会自动备份原文件。" : "只替换 config.toml 的 MCP 配置段，其他配置保留。" : "读取当前 config.toml，把差异写入数据库。"}
          </span>
        </span>
        <span className="muted shrink-0 text-xs">{dbToLive ? "数据库 → 文件" : "文件 → 数据库"}</span>
      </button>
    );
  };

  return (
    <AppDialog
      open={open}
      onOpenChange={(next) => { if (!next && !busy) onClose(); }}
      title={step === "confirm" && direction ? direction === "db-to-live" ? "确认覆盖 config.toml" : "确认更新数据库" : previewError ? "恢复 MCP 配置" : "处理 MCP 配置差异"}
      description={dialogDescription}
      className="max-w-[560px]"
      footer={step === "confirm" ? (
        <div className="flex w-full items-center justify-end gap-2">
          <button type="button" className="apple-action-button" disabled={busy} onClick={() => setStep("diff")}>{previewError ? "返回" : "返回差异"}</button>
          <button type="button" className="apple-action-button app-button--primary" disabled={busy || !direction} onClick={() => direction && onApply(direction)}>{direction === "db-to-live" ? "确认覆盖 config.toml" : "确认更新数据库"}</button>
        </div>
      ) : (
        <button type="button" className="apple-action-button" disabled={busy} onClick={onClose}>取消</button>
      )}
    >
      {step === "confirm" ? (
        <div className="mcp-sync-confirm">
          <p className="mcp-sync-confirm__scope">
            <span className="field-subtitle">覆盖范围</span>
            <span>{direction === "db-to-live" ? previewError ? <>config.toml 无法解析，将用数据库中的 MCP 配置重建整个文件。</> : <>完整替换 <code className="mono code-tok">~/.codex/config.toml</code> 的 <code className="mono code-tok">[mcp_servers]</code> 段；其他配置不受影响。</> : <>完整替换数据库中的 MCP 配置；数据库中的其他数据不受影响。</>}</span>
          </p>
          <div className="mcp-sync-confirm__changes">
            <div className="field-subtitle">变更摘要</div>
            <ul className="mcp-sync-confirm__list">{pendingLines.map((line, index) => <li key={index}>{line}</li>)}</ul>
          </div>
        </div>
      ) : previewError ? (
        <div className="space-y-3">
          <p className="muted text-sm">{previewError}</p>
          {directionChoice("db-to-live")}
        </div>
      ) : preview ? (
        <div className="space-y-4">
          <div className="muted rounded-[var(--radius-control-sm)] bg-[color-mix(in_srgb,var(--sidebar-bg)_34%,var(--panel-bg))] px-3 py-2 text-sm">
            数据库 {preview.db_count} 台 · config.toml {preview.live_count} 台 · 差异 {preview.entries.length} 项
          </div>
          <div className="space-y-2">
            <div className="field-subtitle">差异详情</div>
            <div className="max-h-[50vh] space-y-2 overflow-y-auto pr-1">
              {entries.map((entry) => {
                const isExpanded = expanded.has(entry.name);
                return (
                  <div key={entry.name} className="apple-group">
                    <button type="button" className="flex w-full items-center gap-2 bg-black/4 px-3 py-2 text-left transition-colors dark:bg-white/6" aria-expanded={isExpanded} onClick={() => setExpanded((current) => { const next = new Set(current); if (next.has(entry.name)) next.delete(entry.name); else next.add(entry.name); return next; })}>
                      <span className="flex min-w-0 flex-1 items-center gap-2">
                        <span className={`apple-chip ${entry.kind === "live_only" || entry.unmodeled_only ? "chip-warn" : "chip-danger"}`}>{kindText(entry)}</span>
                        <span className="truncate font-semibold">{entry.name}</span>
                      </span>
                      <span className="muted shrink-0 text-xs">{isExpanded ? "收起" : "查看明细"}</span>
                    </button>
                    {isExpanded ? <div className="mono space-y-1 border-t border-[var(--panel-divider)] bg-black/4 p-3 meta-xs leading-relaxed break-all dark:bg-white/6">{entry.changed_fields.length ? entry.changed_fields.map((diff) => <div key={diff.field}>{fieldLabels[diff.field] ?? diff.field}：数据库 {valueText(diff.db)} → config.toml {valueText(diff.live)}</div>) : entry.unmodeled_only ? <p>建模字段全部相同，差异只在注释 / 格式 / 未建模键。</p> : <p className="whitespace-pre-wrap">{entry.live_toml ?? entry.db_toml}</p>}</div> : null}
                  </div>
                );
              })}
            </div>
          </div>
          <div className="space-y-2">
            <div className="field-subtitle">选择要保留的配置</div>
            <div className="grid gap-2">
              {directionChoice("live-to-db")}
              {directionChoice("db-to-live")}
            </div>
          </div>
        </div>
      ) : null}
    </AppDialog>
  );
}
