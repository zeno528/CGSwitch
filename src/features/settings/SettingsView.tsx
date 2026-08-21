import { ArrowLeft, ArrowClockwise, CircleNotch, Database, DownloadSimple, FloppyDisk, FolderOpen, Info, Moon, MoonStars, PencilSimple, Monitor, Power, SlidersHorizontal, Sun, TerminalWindow, TrayArrowDown, UploadSimple, UserCircle } from "@phosphor-icons/react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useRef, useState } from "react";
import { api, isTauri } from "../../api";
import { useFeedback } from "../../app/Feedback";
import { AppDialog } from "../../components/AppDialog";
import { AppSelect } from "../../components/AppSelect";
import { AppSwitch } from "../../components/AppSwitch";
import { ProfileIconTile } from "../../components/ProfileIconTile";
import { TrashIcon } from "../../components/TrashIcon";
import type { AppState, DatabaseBackupInfo, PathInfo, Settings } from "../../types";
import ChatGPTAccount from "./ChatGPTAccount";
import version from "../../../VERSION?raw";

interface SettingsViewProps { state: AppState; onPreviewTheme: (theme: Settings["theme"]) => void; onRefresh: () => Promise<void>; onSaved: (settings: Settings) => void; onHome: () => void; }
type Section = "general" | "codex" | "account" | "advanced" | "about";

const themeOptions = [{ label: "跟随系统", value: "system" as const }, { label: "浅色", value: "light" as const }, { label: "深色", value: "dark" as const }];
const autoBackupOptions = [{ label: "关闭", value: 0 }, { label: "6 小时", value: 6 }, { label: "12 小时", value: 12 }, { label: "24 小时", value: 24 }, { label: "48 小时", value: 48 }, { label: "7 天", value: 168 }];
const keepOptions = [3, 5, 10, 15, 20, 30].map((value) => ({ label: `${value} 个`, value }));

export default function SettingsView({ state, onPreviewTheme, onRefresh, onSaved, onHome }: SettingsViewProps) {
  const feedback = useFeedback();
  const [form, setForm] = useState<Settings>(state.settings);
  const [section, setSection] = useState<Section>("general");
  const [saving, setSaving] = useState(false);
  const [backups, setBackups] = useState<DatabaseBackupInfo[]>([]);
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [openingPath, setOpeningPath] = useState<string | null>(null);
  const [renameTarget, setRenameTarget] = useState<DatabaseBackupInfo | null>(null);
  const [renameText, setRenameText] = useState("");
  const [renaming, setRenaming] = useState(false);
  const tabBar = useRef<HTMLDivElement>(null);
  const [indicator, setIndicator] = useState({ left: 0, width: 0 });

  useEffect(() => setForm(state.settings), [state.settings]);
  const loadBackups = async () => { try { setBackups(await api.listDatabaseBackups()); } catch { setBackups([]); } };
  useEffect(() => { void api.getSettings().then((settings) => { setForm(settings); onSaved(settings); }).catch((error) => feedback.error(String(error))); void loadBackups(); }, []);
  useEffect(() => { const button = tabBar.current?.querySelector<HTMLElement>(`[data-section="${section}"]`); if (button) setIndicator({ left: button.offsetLeft, width: button.offsetWidth }); }, [section]);

  const saveGeneral = async (patch: Partial<Settings>) => {
    if (saving) return;
    const previous = form;
    const next = { ...form, ...patch };
    setForm(next);
    if (patch.theme) onPreviewTheme(patch.theme);
    setSaving(true);
    try { const saved = await api.saveSettings(next); onSaved(saved); if (saved.database_backup_keep_count !== previous.database_backup_keep_count) await loadBackups(); }
    catch (error) { setForm(previous); onPreviewTheme(previous.theme); feedback.error(String(error)); }
    finally { setSaving(false); }
  };

  const exportBackupToFile = async () => {
    if (exporting) return;
    setExporting(true);
    try { let directory: string | null = null; if (isTauri) { const result = await openDialog({ title: "选择导出目录", directory: true, multiple: false }); directory = typeof result === "string" ? result : null; if (!directory) return; } const path = await api.exportDatabaseTo(directory ?? "mock-export"); feedback.success(`数据文件已导出：${path}`); }
    catch (error) { feedback.error(String(error)); }
    finally { setExporting(false); }
  };
  const importBackupFromFile = async () => {
    if (importing) return;
    try { let picked: string | null = null; if (isTauri) { const result = await openDialog({ title: "选择数据库文件", multiple: false, filters: [{ name: "SQLite 数据库", extensions: ["db"] }] }); picked = typeof result === "string" ? result : null; if (!picked) return; } setImporting(true); await api.importDatabase(picked ?? "mock-backup.db"); feedback.success("数据库已导入并恢复"); await onRefresh(); await loadBackups(); }
    catch (error) { feedback.error(String(error)); }
    finally { setImporting(false); }
  };
  const createImmediateBackup = async () => { if (exporting) return; setExporting(true); try { await api.exportDatabase(); feedback.success("已创建内部备份"); await loadBackups(); } catch (error) { feedback.error(String(error)); } finally { setExporting(false); } };
  const backupTitle = (name: string) => name.replace(/^(?:cg-backup-|cgswitch-export-)/, "").replace(/\.db$/, "");
  const formatSize = (bytes: number) => bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(2)} MB`;
  const formatTimestamp = (seconds: number) => { const date = new Date(seconds * 1000); const pad = (value: number) => String(value).padStart(2, "0"); return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`; };
  const openRename = (backup: DatabaseBackupInfo) => { setRenameTarget(backup); setRenameText(backupTitle(backup.name)); };
  const submitRename = async () => { const target = renameTarget; const text = renameText.trim(); if (!target || renaming || !text || text === backupTitle(target.name)) { setRenameTarget(null); return; } setRenaming(true); try { await api.renameDatabaseBackup(target.name, text); feedback.success("备份已重命名"); await loadBackups(); setRenameTarget(null); } catch (error) { feedback.error(String(error)); } finally { setRenaming(false); } };
  const restoreBackup = async (backup: DatabaseBackupInfo) => { if (!await feedback.confirm({ title: "恢复数据库备份", description: `确定用「${backup.name}」覆盖当前所有供应商数据吗？恢复后无法撤销。备份中的 MCP 配置会一并写回 ~/.codex/config.toml（覆盖当前 MCP 段）。`, confirmText: "恢复", destructive: true })) return; try { await api.restoreDatabase(backup.name); feedback.success("数据库已恢复"); await onRefresh(); await loadBackups(); } catch (error) { feedback.error(String(error)); } };
  const deleteBackup = async (backup: DatabaseBackupInfo) => { if (!await feedback.confirm({ title: "删除数据库备份", description: <>确定删除「<strong>{backup.name}</strong>」吗？删除后不可恢复。</>, confirmText: "删除", destructive: true })) return; try { await api.deleteDatabaseBackup(backup.name); feedback.success("备份已删除"); await loadBackups(); } catch (error) { feedback.error(String(error)); } };
  const openPath = async (item: PathInfo) => { if (openingPath) return; setOpeningPath(item.path); try { await api.openPath(item.path); } catch (error) { feedback.error(String(error)); } finally { setOpeningPath(null); } };

  const tab = (id: Section, label: string, Icon: typeof SlidersHorizontal) => <button type="button" data-section={id} className={`settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors ${section === id ? "text-accent" : "text-[var(--text-secondary)] hover:text-accent"}`} aria-current={section === id ? "page" : undefined} onClick={() => setSection(id)}><Icon className="h-4 w-4 shrink-0" weight="bold" />{label}</button>;

  return <section className="settings-page mx-auto flex w-full max-w-none flex-col"><div className="apple-page-bar apple-page-bar--sticky"><button type="button" className="apple-page-header apple-back-button" aria-label="返回首页" onClick={onHome}><ArrowLeft className="h-4 w-4 shrink-0 text-accent" weight="bold" /><span className="apple-title">设置</span></button></div><div ref={tabBar} className="relative mt-2 flex items-center gap-1 border-b border-[var(--panel-border)]" aria-label="设置分区"><span className="settings-tab-indicator absolute -bottom-px h-0.5 rounded-full bg-accent" style={{ left: indicator.left, width: indicator.width }} aria-hidden="true" />{tab("general", "通用", SlidersHorizontal)}{tab("codex", "应用", TerminalWindow)}{tab("account", "账号", UserCircle)}{tab("advanced", "高级", Database)}{tab("about", "关于", Info)}</div><div className="apple-edit-content">
    {section === "general" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="setting-title mb-2">主题</div><div className="apple-group inline-flex gap-1 p-1">{themeOptions.map((option) => <button key={option.value} type="button" className={`inline-flex h-9 w-28 items-center justify-center gap-1.5 rounded-xl text-sm transition-colors ${form.theme === option.value ? "bg-[var(--selection-bg)] font-semibold text-accent" : "font-medium hover:bg-black/5 dark:hover:bg-white/8"}`} aria-pressed={form.theme === option.value} onClick={() => void saveGeneral({ theme: option.value })}>{option.value === "system" ? <Monitor className="h-4 w-4" weight="bold" /> : option.value === "light" ? <Sun className="h-4 w-4" weight="bold" /> : <Moon className="h-4 w-4" weight="bold" />}{option.label}</button>)}</div><hr className="my-4 border-0 border-t border-[var(--panel-divider)]" /><div className="flex flex-col gap-5">{[["autostart_enabled", "开机自启", "登录系统后自动启动 CGswitch", Power, "text-accent"], ["silent_start", "静默启动", "启动时不显示主窗口，驻留系统托盘", MoonStars, "text-[#af52de]"], ["minimize_to_tray", "关闭时最小化到托盘", "点击关闭按钮时隐藏到托盘而不是退出", TrayArrowDown, "text-[var(--warning)]"]].map(([key, label, description, Icon, color]) => <div key={String(key)} className="flex items-center justify-between gap-4"><div className="flex items-start gap-3"><span className={`settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-xl ${String(color)}`}><Icon className="h-[18px] w-[18px]" weight="bold" /></span><div><div className="setting-title">{String(label)}</div><div className="setting-description mt-0.5">{String(description)}</div></div></div><AppSwitch checked={Boolean(form[key as keyof Settings])} onCheckedChange={(value) => void saveGeneral({ [String(key)]: value })} /></div>)}</div></div> : null}
    {section === "codex" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="flex items-center justify-between gap-4"><div><div className="setting-title">应用配置后自动重启 Codex</div><div className="setting-description mt-0.5">开启后应用配置会自动重启 Codex 生效；关闭则只保存配置，稍后可手动重启。</div></div><AppSwitch checked={form.auto_restart} onCheckedChange={(value) => void saveGeneral({ auto_restart: value })} /></div></div> : null}
    {section === "account" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="flex items-center gap-3"><ProfileIconTile name="ChatGPT" icon="openai-chatgpt" size="sm" /><h2 className="text-[15px] font-semibold tracking-tight">ChatGPT 账号</h2></div><div className="mt-4"><ChatGPTAccount initialStatus={state.auth_status} /></div></div> : null}
    {section === "advanced" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="flex items-center gap-3"><span className="settings-icon-tile grid h-9 w-9 shrink-0 place-items-center rounded-xl text-accent"><Database className="h-[18px] w-[18px]" weight="bold" /></span><div><div className="setting-title">数据备份</div><div className="setting-description mt-0.5">管理本地数据库备份，支持导入、导出和自动备份</div></div></div><div className="mt-4 grid grid-cols-2 gap-2"><button type="button" className="apple-action-button" disabled={importing} onClick={() => void importBackupFromFile()}><DownloadSimple className="h-4 w-4 text-accent" weight="bold" />导入数据库</button><button type="button" className="apple-action-button" disabled={exporting} onClick={() => void exportBackupToFile()}><UploadSimple className="h-4 w-4 text-success" weight="bold" />导出数据库</button></div><div className="mt-[var(--gap-section)] rounded-[var(--radius-card)] bg-[color-mix(in_srgb,var(--sidebar-bg)_30%,var(--panel-bg))] p-3.5 shadow-[0_0_0_1px_var(--panel-ring)]"><div className="text-[15px] font-semibold tracking-tight">自动备份</div><div className="mt-3 grid gap-3 sm:grid-cols-2"><div><div className="field-label muted mb-1.5">备份间隔</div><AppSelect value={form.auto_backup_interval_hours} options={autoBackupOptions} onChange={(value) => void saveGeneral({ auto_backup_interval_hours: value })} /></div><div><div className="field-label muted mb-1.5">最多保留</div><AppSelect value={form.database_backup_keep_count} options={keepOptions} onChange={(value) => void saveGeneral({ database_backup_keep_count: value })} /></div></div><div className="mt-3 grid grid-cols-2 gap-2"><button type="button" className="apple-action-button" disabled={exporting} onClick={() => void createImmediateBackup()}><FloppyDisk className="h-4 w-4 text-accent" weight="bold" />立即备份</button><button type="button" className="apple-action-button" onClick={() => { const item = state.paths.find((path) => path.label === "备份目录"); if (item) void openPath(item); else feedback.warning("找不到备份目录"); }}><FolderOpen className="h-4 w-4 text-[var(--warning)]" weight="bold" />备份文件夹</button></div></div><hr className="my-4 border-0 border-t border-[var(--panel-divider)]" /><div className="setting-title mb-2">备份记录</div>{backups.length ? <div className="space-y-2">{backups.map((backup) => <div key={backup.name} className="apple-list-row"><div className="flex min-w-0 items-center gap-2.5"><span className="settings-icon-tile grid h-8 w-8 shrink-0 place-items-center rounded-lg text-accent"><Database className="h-4 w-4" weight="bold" /></span><div className="min-w-0"><div className="mono truncate text-xs font-medium">{backup.name}</div><div className="muted text-[11px]">{formatTimestamp(backup.created_at)} · {formatSize(backup.size_bytes)}</div></div></div><div className="flex shrink-0 gap-1.5"><button type="button" className="apple-icon-button text-zinc-500 hover:text-accent" title="编辑备份名称" onClick={() => openRename(backup)}><PencilSimple className="h-4 w-4" weight="bold" /></button><button type="button" className="apple-icon-button text-accent" title="恢复数据库" onClick={() => void restoreBackup(backup)}><ArrowClockwise className="h-4 w-4" weight="bold" /></button><button type="button" className="apple-icon-button text-[var(--danger)]/70" title="删除备份" onClick={() => void deleteBackup(backup)}><TrashIcon /></button></div></div>)}</div> : <div className="setting-description flex items-center gap-2"><Database className="h-4 w-4" />还没有导出过备份。</div>}</div> : null}
    {section === "about" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="flex items-center gap-3"><img src="/logo.svg" alt="CGswitch" className="h-12 w-12 shrink-0 dark:invert" /><div><div className="apple-wordmark">CGswitch</div><div className="app-version mt-1.5">版本 {version.trim()}</div></div></div><hr className="my-4 border-0 border-t border-[var(--panel-divider)]" /><h2 className="setting-title">数据与路径</h2><p className="setting-description mt-1.5">常用数据位置，点击文件夹图标即可打开。</p><div className="mt-4 grid gap-2 sm:grid-cols-3">{state.paths.map((item) => <div key={item.label} className="flex min-w-0 items-center justify-between gap-3 rounded-[var(--radius-control-sm)] border border-[var(--panel-ring)] px-3 py-2.5"><div className="min-w-0"><div className="text-sm font-medium">{item.label}</div><div className="mono muted mt-0.5 truncate text-[11px]" title={item.path}>{item.path}</div></div><button type="button" className="apple-icon-button shrink-0 text-zinc-500 hover:text-accent disabled:opacity-40" disabled={Boolean(openingPath)} title={`打开${item.label}`} onClick={() => void openPath(item)}>{openingPath === item.path ? <CircleNotch className="h-4 w-4 animate-spin" weight="bold" /> : <FolderOpen className="h-4 w-4" weight="bold" />}</button></div>)}</div></div> : null}
  </div><AppDialog open={renameTarget !== null} onOpenChange={(open) => { if (!open) setRenameTarget(null); }} title="重命名备份" footer={<><button type="button" className="apple-action-button" onClick={() => setRenameTarget(null)}>取消</button><button type="button" className="apple-action-button app-button--primary" disabled={renaming || !renameText.trim()} onClick={() => void submitRename()}>保存</button></>}><input className="app-input" maxLength={80} placeholder="输入新的备份标题" value={renameText} onChange={(event) => setRenameText(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.nativeEvent.isComposing) void submitRename(); }} /></AppDialog></section>;
}
