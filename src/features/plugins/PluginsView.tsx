import { Download, Blocks, Search } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "../../api";
import { useFeedback } from "../../app/Feedback";
import { AppDialog } from "../../components/AppDialog";
import { AppSwitch } from "../../components/AppSwitch";
import { LoadingSpinner } from "../../components/LoadingSpinner";
import { TrashIcon } from "../../components/TrashIcon";
import type { PluginCandidate, PluginPreview, PluginSummary } from "../../types";

const containsLabels: Record<string, string> = {
  skills: "Skills",
  mcp: "MCP",
  app: "App",
  hooks: "Hook",
  agents: "Agent",
  commands: "命令",
};

const originLabels: Partial<Record<PluginSummary["origin"], string>> = {
  personal: "本地登记",
  claude: "Claude",
  cursor: "Cursor",
  official: "官方目录",
  skill: "Skill 安装",
  codex: "Codex 安装",
};

/** 可启停的条目类来源（摘/回 marketplace 条目） */
const entryOrigins: readonly PluginSummary["origin"][] = ["personal", "claude", "cursor"];
/** 可卸载的来源（官方市场与 Skill 注册表除外） */
const removableOrigins: readonly PluginSummary["origin"][] = [
  "cgswitch",
  "codex",
  "personal",
  "claude",
  "cursor",
];

const originHints: Partial<Record<PluginSummary["origin"], string>> = {
  official: "官方市场，由 Codex 管理",
  skill: "记录在 Codex Skill 注册表",
  codex: "启停请在 Codex 内操作",
  cgswitch: "启停请在 Codex 内操作",
};

function ContainsChips({ items }: { items: string[] }) {
  if (!items.length) return null;
  return (
    <span className="flex shrink-0 flex-wrap gap-1">
      {items.map((item) => (
        <span key={item} className="rounded-md bg-black/5 px-1.5 py-px font-medium tracking-wide muted meta-xs dark:bg-white/10">
          {containsLabels[item] ?? item}
        </span>
      ))}
    </span>
  );
}

export default function PluginsView() {
  const feedback = useFeedback();
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [url, setUrl] = useState("");
  const [previewing, setPreviewing] = useState(false);
  const [preview, setPreview] = useState<PluginPreview | null>(null);
  const [installing, setInstalling] = useState("");
  const [toggling, setToggling] = useState("");

  const refresh = async () => {
    try {
      setPlugins(await api.listPlugins());
      setLoadError("");
    } catch (error) {
      setLoadError(String(error));
    } finally {
      setLoaded(true);
    }
  };
  useEffect(() => { void refresh(); }, []);

  const openPreview = async () => {
    if (previewing) return;
    if (!url.trim()) {
      feedback.warning("请先填写 GitHub 插件地址");
      return;
    }
    setPreviewing(true);
    try {
      setPreview(await api.previewPlugin(url.trim()));
    } catch (error) {
      feedback.error(String(error));
      setPreview(null);
    } finally {
      setPreviewing(false);
    }
  };

  const install = async (candidate: PluginCandidate) => {
    if (installing) return;
    setInstalling(candidate.name);
    try {
      const summary = await api.installPlugin(url.trim(), candidate.sub_path || null);
      feedback.success(`已安装 ${summary.display_name ?? summary.name}，重启 Codex 后生效`);
      setPreview(null);
      await refresh();
    } catch (error) {
      feedback.error(String(error));
    } finally {
      setInstalling("");
    }
  };

  const toggle = async (plugin: PluginSummary, value: boolean) => {
    if (toggling) return;
    setToggling(plugin.name);
    try {
      await api.setPluginEnabled(plugin.name, value);
      await refresh();
    } catch (error) {
      feedback.error(String(error));
    } finally {
      setToggling("");
    }
  };

  const remove = async (plugin: PluginSummary) => {
    const entryLike = entryOrigins.includes(plugin.origin);
    const confirmed = await feedback.confirm({
      title: plugin.origin === "cgswitch" ? "卸载插件" : entryLike ? "移除插件登记" : "卸载插件",
      description: entryLike
        ? `确定移除「${plugin.display_name ?? plugin.name}」的登记条目吗？该插件不是 CGswitch 安装的，文件会保留在原处，重新启用即可恢复。`
        : `确定卸载「${plugin.display_name ?? plugin.name}」吗？将通过 codex CLI 卸载（${plugin.marketplace ? `市场源 ${plugin.marketplace} 保留` : "市场源保留"}），之后可在 Codex 或这里重新安装。`,
      confirmText: entryLike ? "移除" : "卸载",
      destructive: true,
    });
    if (!confirmed) return;
    try {
      await api.uninstallPlugin(plugin.name);
      feedback.success(entryLike ? "登记已移除，文件保留" : "插件已卸载");
      await refresh();
    } catch (error) {
      feedback.error(String(error));
    }
  };

  return (
    <section className="apple-scroll-page mx-auto w-full max-w-none">
      <header className="apple-page-bar flex-wrap justify-between gap-4">
        <div className="flex min-w-0 items-center gap-2.5">
          <span className="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-[10px] text-accent">
            <Blocks className="h-[18px] w-[18px]" strokeWidth={2} />
          </span>
          <div className="flex items-center gap-2">
            <div className="apple-title">插件市场</div>
            {loaded ? (
              <span className="apple-chip" aria-label={`${plugins.length} 个插件`}>{plugins.length}</span>
            ) : null}
          </div>
        </div>
        <div className="flex w-full max-w-md items-center gap-2">
          <input
            className="app-input min-w-0 flex-1"
            placeholder="https://github.com/<owner>/<repo>（可带 /tree/<分支>/<子目录>）"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.nativeEvent.isComposing) void openPreview();
            }}
          />
          <button type="button" className="apple-action-button app-button--primary" disabled={previewing} onClick={() => void openPreview()}>
            {previewing ? <LoadingSpinner /> : <Search className="h-4 w-4" strokeWidth={2} />}
            获取预览
          </button>
        </div>
      </header>
      <div className="apple-edit-content">
        {loadError ? <p className="muted mt-4 text-sm">{loadError}</p> : null}
        {loaded && plugins.length === 0 ? (
          <div className="apple-group py-14 text-center">
            <p className="muted">还没有安装插件。粘贴 GitHub 插件仓库地址并点「获取预览」，确认内容后安装。</p>
          </div>
        ) : plugins.length ? (
          <div className="space-y-2">
            {plugins.map((plugin) => (
              <div key={plugin.name} className="apple-list-row">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="min-w-0 truncate font-semibold">{plugin.display_name ?? plugin.name}</span>
                    {plugin.version ? (
                      <span className="shrink-0 rounded-md bg-black/5 px-1.5 py-px font-medium tracking-wide muted meta-xs dark:bg-white/10">v{plugin.version}</span>
                    ) : null}
                    {originLabels[plugin.origin] ? (
                      <span className="shrink-0 rounded-md bg-black/5 px-1.5 py-px font-medium tracking-wide muted meta-xs dark:bg-white/10">{originLabels[plugin.origin]}</span>
                    ) : null}
                    {plugin.enabled ? null : <span className="apple-chip chip-warn shrink-0">已禁用</span>}
                  </div>
                  <div className="muted meta-xs truncate">
                    {plugin.description ?? plugin.name}
                  </div>
                  <div className="mt-1 flex items-center gap-2">
                    <ContainsChips items={plugin.contains} />
                    {plugin.source_url ? <span className="muted meta-xs truncate">{plugin.source_url}</span> : null}
                  </div>
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {entryOrigins.includes(plugin.origin) ? (
                    <>
                      <AppSwitch
                        size="sm"
                        checked={plugin.enabled}
                        label={`启用 ${plugin.name}`}
                        disabled={toggling === plugin.name}
                        onCheckedChange={(value) => void toggle(plugin, value)}
                      />
                      <button
                        type="button"
                        className="apple-icon-button text-[var(--danger)]/70 hover:bg-[var(--danger)]/10 hover:text-[var(--danger)]"
                        title="移除登记"
                        aria-label={`移除登记 ${plugin.name}`}
                        onClick={() => void remove(plugin)}
                      >
                        <TrashIcon />
                      </button>
                    </>
                  ) : (
                    <>
                      {originHints[plugin.origin] ? (
                        <span className="muted meta-xs">{originHints[plugin.origin]}</span>
                      ) : null}
                      {removableOrigins.includes(plugin.origin) ? (
                        <button
                          type="button"
                          className="apple-icon-button text-[var(--danger)]/70 hover:bg-[var(--danger)]/10 hover:text-[var(--danger)]"
                          title="卸载"
                          aria-label={`卸载 ${plugin.name}`}
                          onClick={() => void remove(plugin)}
                        >
                          <TrashIcon />
                        </button>
                      ) : null}
                    </>
                  )}
                </div>
              </div>
            ))}
          </div>
        ) : null}
      </div>
      <AppDialog
        open={preview !== null}
        onOpenChange={(open) => { if (!open) setPreview(null); }}
        title="安装插件"
        footer={
          <button type="button" className="apple-action-button" onClick={() => setPreview(null)}>
            关闭
          </button>
        }
      >
        {preview ? (
          <div className="flex flex-col gap-3">
            <p className="muted text-sm">
              仓库 <span className="mono">{preview.repo}</span> · 分支 <span className="mono">{preview.reference}</span>
              {preview.reference !== preview.default_branch ? `（默认分支为 ${preview.default_branch}）` : ""}
            </p>
            {preview.candidates.map((candidate) => (
              <div key={candidate.sub_path || candidate.name} className="rounded-[var(--radius-card)] bg-black/3 p-3.5 shadow-[0_0_0_1px_var(--panel-ring)] dark:bg-white/4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="truncate font-semibold">{candidate.display_name ?? candidate.name}</span>
                      {candidate.version ? (
                        <span className="shrink-0 rounded-md bg-black/5 px-1.5 py-px font-medium tracking-wide muted meta-xs dark:bg-white/10">v{candidate.version}</span>
                      ) : null}
                    </div>
                    <div className="muted meta-xs truncate">{candidate.description ?? (candidate.sub_path || "仓库根目录")}</div>
                  </div>
                  <button
                    type="button"
                    className="apple-action-button app-button--primary"
                    disabled={installing !== ""}
                    onClick={() => void install(candidate)}
                  >
                    {installing === candidate.name ? <LoadingSpinner /> : <Download className="h-4 w-4" strokeWidth={2} />}
                    安装
                  </button>
                </div>
                <div className="mt-2"><ContainsChips items={candidate.contains} /></div>
                <details className="mt-2">
                  <summary className="muted meta-xs cursor-pointer select-none">文件清单（{candidate.files.length} 项）</summary>
                  <ul className="mono muted mt-1.5 flex flex-col gap-0.5">
                    {candidate.files.slice(0, 40).map((file) => (
                      <li key={file} className="truncate">{file}</li>
                    ))}
                    {candidate.files.length > 40 ? <li>…其余 {candidate.files.length - 40} 个文件</li> : null}
                  </ul>
                </details>
              </div>
            ))}
          </div>
        ) : null}
      </AppDialog>
    </section>
  );
}
