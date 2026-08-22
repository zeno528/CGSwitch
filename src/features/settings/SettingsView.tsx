import { AppWindow, ArrowLeft, CircleUserRound, Database, Info, SlidersHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "../../api";
import { useFeedback } from "../../app/Feedback";
import { AppSwitch } from "../../components/AppSwitch";
import { ProfileIconTile } from "../../components/ProfileIconTile";
import type { AppState, PathInfo, Settings } from "../../types";
import ChatGPTAccount from "./ChatGPTAccount";
import { SettingsAbout, SettingsAdvanced, SettingsGeneral } from "./SettingsSections";

interface SettingsViewProps { state: AppState; onPreviewTheme: (theme: Settings["theme"]) => void; onRefresh: () => Promise<void>; onSaved: (settings: Settings) => void; onHome: () => void; }
type Section = "general" | "codex" | "account" | "advanced" | "about";

export default function SettingsView({ state, onPreviewTheme, onRefresh, onSaved, onHome }: SettingsViewProps) {
  const feedback = useFeedback();
  const [form, setForm] = useState<Settings>(state.settings);
  const [section, setSection] = useState<Section>("general");
  const [saving, setSaving] = useState(false);
  const [backupsEpoch, setBackupsEpoch] = useState(0);
  const [openingPath, setOpeningPath] = useState<string | null>(null);
  const tabBar = useRef<HTMLDivElement>(null);
  const [indicator, setIndicator] = useState({ left: 0, width: 0 });

  useEffect(() => setForm(state.settings), [state.settings]);
  useEffect(() => { void api.getSettings().then((settings) => { setForm(settings); onSaved(settings); }).catch((error) => feedback.error(String(error))); }, []);
  useEffect(() => { const button = tabBar.current?.querySelector<HTMLElement>(`[data-section="${section}"]`); if (button) setIndicator({ left: button.offsetLeft, width: button.offsetWidth }); }, [section]);

  const saveGeneral = async (patch: Partial<Settings>) => {
    if (saving) return;
    const previous = form;
    const next = { ...form, ...patch };
    setForm(next);
    if (patch.theme) onPreviewTheme(patch.theme);
    setSaving(true);
    try { const saved = await api.saveSettings(next); onSaved(saved); if (saved.database_backup_keep_count !== previous.database_backup_keep_count) setBackupsEpoch((epoch) => epoch + 1); }
    catch (error) { setForm(previous); onPreviewTheme(previous.theme); feedback.error(String(error)); }
    finally { setSaving(false); }
  };

  const openPath = async (item: PathInfo) => { if (openingPath) return; setOpeningPath(item.path); try { await api.openPath(item.path); } catch (error) { feedback.error(String(error)); } finally { setOpeningPath(null); } };
  const tab = (id: Section, label: string, Icon: typeof SlidersHorizontal) => <button type="button" data-section={id} className={`settings-tab relative flex h-10 items-center gap-1.5 rounded-md px-3 transition-colors ${section === id ? "text-accent" : "text-[var(--text-secondary)] hover:text-accent"}`} aria-current={section === id ? "page" : undefined} onClick={() => setSection(id)}><Icon className="h-4 w-4 shrink-0" strokeWidth={2} />{label}</button>;

  return <section className="settings-page mx-auto flex w-full max-w-none flex-col"><div className="apple-page-bar apple-page-bar--sticky"><button type="button" className="apple-page-header apple-back-button" aria-label="返回首页" onClick={onHome}><ArrowLeft className="h-4 w-4 shrink-0 text-accent" strokeWidth={2} /><span className="apple-title">设置</span></button></div><div ref={tabBar} className="relative mt-2 flex items-center gap-1 border-b border-[var(--panel-border)]" aria-label="设置分区"><span className="settings-tab-indicator absolute -bottom-px h-0.5 rounded-full bg-accent" style={{ left: indicator.left, width: indicator.width }} aria-hidden="true" />{tab("general", "通用", SlidersHorizontal)}{tab("codex", "应用", AppWindow)}{tab("account", "账号", CircleUserRound)}{tab("advanced", "高级", Database)}{tab("about", "关于", Info)}</div><div className="apple-edit-content">
    {section === "general" ? <SettingsGeneral form={form} onPatch={(patch) => void saveGeneral(patch)} /> : null}
    {section === "codex" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="flex items-center justify-between gap-4"><div><div className="setting-title">应用配置后自动重启 Codex</div><div className="setting-description mt-0.5">开启后应用配置会自动重启 Codex 生效；关闭则只保存配置，稍后可手动重启。</div></div><AppSwitch checked={form.auto_restart} onCheckedChange={(value) => void saveGeneral({ auto_restart: value })} /></div></div> : null}
    {section === "account" ? <div className="apple-group mt-[var(--gap-section)] p-[var(--gap-card)]"><div className="flex items-center gap-3"><ProfileIconTile name="ChatGPT" icon="openai-chatgpt" size="sm" /><h2 className="title-sm">ChatGPT 账号</h2></div><div className="mt-4"><ChatGPTAccount initialStatus={state.auth_status} /></div></div> : null}
    {section === "advanced" ? <SettingsAdvanced form={form} onPatch={(patch) => void saveGeneral(patch)} paths={state.paths} backupsEpoch={backupsEpoch} onOpenPath={openPath} onRefresh={onRefresh} /> : null}
    {section === "about" ? <SettingsAbout paths={state.paths} onOpenPath={openPath} openingPath={openingPath} /> : null}
  </div></section>;
}
