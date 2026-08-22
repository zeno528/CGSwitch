import { useCallback, useEffect, useRef, useState } from "react";
import { Layers2, Minus, Settings as SettingsIcon, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, isTauri } from "../api";
import type { AppState, Settings } from "../types";
import { McpIcon } from "../components/McpIcon";
import { FeedbackProvider } from "./Feedback";
import ProfilesView from "../features/profiles/ProfilesView";
import McpView from "../features/mcp/McpView";
import SettingsView from "../features/settings/SettingsView";

type AppView = "profiles" | "mcp" | "settings";

const appWindow = isTauri ? getCurrentWindow() : null;

export default function AppShell() {
  const [view, setView] = useState<AppView>("profiles");
  const [profilesReset, setProfilesReset] = useState(0);
  const [mcpReset, setMcpReset] = useState(0);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(
    () => localStorage.getItem("cgswitch.sidebar-collapsed") !== "0",
  );
  const [sidebarFlyoutArmed, setSidebarFlyoutArmed] = useState(true);
  const [state, setState] = useState<AppState | null>(null);
  const [loadError, setLoadError] = useState("");
  const [systemDark, setSystemDark] = useState(() => window.matchMedia("(prefers-color-scheme: dark)").matches);
  const [indicator, setIndicator] = useState({ top: 8, left: 0, instant: false });
  const [activationEpoch, setActivationEpoch] = useState(0);
  const stateRef = useRef<AppState | null>(null);
  const codexPollTimer = useRef<number | undefined>(undefined);
  const codexPolling = useRef(false);
  const profileNavRef = useRef<HTMLButtonElement>(null);
  const mcpNavRef = useRef<HTMLButtonElement>(null);
  const settingsNavRef = useRef<HTMLButtonElement>(null);
  const sidebarNavRef = useRef<HTMLElement>(null);
  const previousViewRef = useRef<AppView>(view);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const refresh = useCallback(async () => {
    try {
      const nextState = await api.getState();
      const merged = stateRef.current ? { ...nextState, auth_status: stateRef.current.auth_status } : nextState;
      stateRef.current = merged;
      setState(merged);
      setLoadError("");
    } catch (error) {
      setLoadError(String(error));
    }
  }, []);

  const refreshAuthStatus = useCallback(async () => {
    try {
      const auth_status = await api.authGetStatus();
      const previous = stateRef.current;
      if (previous) {
        const next = { ...previous, auth_status };
        stateRef.current = next;
        setState(next);
      }
    } catch {
      // 首屏已经显示时，后台认证刷新失败保留旧快照。
    }
  }, []);

  const pollCodexStatus = useCallback(async () => {
    if (codexPolling.current || !stateRef.current) return;
    codexPolling.current = true;
    try {
      const codex = await api.getCodexStatus();
      const previous = stateRef.current;
      if (previous) {
        const next = { ...previous, codex };
        stateRef.current = next;
        setState(next);
      }
    } catch {
      // 轮询失败保留上次状态。
    } finally {
      codexPolling.current = false;
    }
  }, []);

  const stopCodexPolling = useCallback(() => {
    if (codexPollTimer.current !== undefined) {
      window.clearInterval(codexPollTimer.current);
      codexPollTimer.current = undefined;
    }
  }, []);

  const syncCodexPolling = useCallback(() => {
    if (document.hidden || !stateRef.current) {
      stopCodexPolling();
      return;
    }
    if (codexPollTimer.current === undefined) {
      codexPollTimer.current = window.setInterval(() => void pollCodexStatus(), 3000);
    }
  }, [pollCodexStatus, stopCodexPolling]);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  const isDark = state?.settings.theme === "dark" || ((state?.settings.theme ?? "system") === "system" && systemDark);

  useEffect(() => {
    const root = document.documentElement;
    root.classList.add("theme-switching");
    root.classList.toggle("dark", isDark);
    root.style.colorScheme = isDark ? "dark" : "light";
    const frame = requestAnimationFrame(() => root.classList.remove("theme-switching"));
    if (isTauri) void api.setWindowTheme(isDark).catch(() => undefined);
    return () => cancelAnimationFrame(frame);
  }, [isDark]);

  useEffect(() => {
    let cancelled = false;
    let delayedAuth: number | undefined;
    void (async () => {
      await refresh();
      if (cancelled) return;
      if (isTauri && !stateRef.current?.settings.silent_start) {
        try {
          await appWindow?.show();
        } catch {
          // 内容初始化不依赖窗口显示成功。
        }
      }
      delayedAuth = window.setTimeout(() => {
        if (!cancelled) void refreshAuthStatus();
      }, 0);
      syncCodexPolling();
    })();
    return () => {
      cancelled = true;
      if (delayedAuth !== undefined) window.clearTimeout(delayedAuth);
      stopCodexPolling();
    };
  }, [refresh, refreshAuthStatus, stopCodexPolling, syncCodexPolling]);

  useEffect(() => {
    const onActive = () => {
      setActivationEpoch((value) => value + 1);
      void refresh();
      void refreshAuthStatus();
      syncCodexPolling();
    };
    const onInactive = () => stopCodexPolling();
    const onVisibility = () => (document.hidden ? onInactive() : onActive());
    window.addEventListener("focus", onActive);
    window.addEventListener("blur", onInactive);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("focus", onActive);
      window.removeEventListener("blur", onInactive);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [refresh, refreshAuthStatus, stopCodexPolling, syncCodexPolling]);

  const updateIndicator = useCallback(() => {
    const target = view === "profiles" ? profileNavRef.current : view === "mcp" ? mcpNavRef.current : settingsNavRef.current;
    const nav = sidebarNavRef.current;
    if (!target || !nav) return;
    setIndicator((current) => ({
      ...current,
      top: target.getBoundingClientRect().top - nav.getBoundingClientRect().top + 8,
      left: target.offsetLeft,
    }));
  }, [view]);

  useEffect(() => {
    const frame = requestAnimationFrame(updateIndicator);
    const previousView = previousViewRef.current;
    previousViewRef.current = view;
    if (previousView === view) return () => cancelAnimationFrame(frame);

    setIndicator((current) => ({
      ...current,
      instant: view === "settings" || previousView === "settings",
    }));
    const reset = requestAnimationFrame(() => {
      setIndicator((current) => ({ ...current, instant: false }));
    });
    return () => {
      cancelAnimationFrame(frame);
      cancelAnimationFrame(reset);
    };
  }, [updateIndicator, sidebarCollapsed]);

  useEffect(() => {
    const main = document.querySelector("main");
    if (main) document.documentElement.style.setProperty("--scrollbar-size", `${main.offsetWidth - main.clientWidth}px`);
  }, [state, view]);

  const toggleSidebar = () => {
    setSidebarCollapsed((collapsed) => {
      const next = !collapsed;
      localStorage.setItem("cgswitch.sidebar-collapsed", next ? "1" : "0");
      if (next) setSidebarFlyoutArmed(false);
      window.setTimeout(updateIndicator, 360);
      return next;
    });
  };

  const goProfiles = () => {
    setProfilesReset((value) => value + 1);
    setView("profiles");
  };

  const goMcp = () => {
    setMcpReset((value) => value + 1);
    setView("mcp");
  };

  const updateSettings = (settings: Settings) => {
    const previous = stateRef.current;
    if (!previous) return;
    const next = { ...previous, settings };
    stateRef.current = next;
    setState(next);
  };

  const previewTheme = (theme: Settings["theme"]) => {
    const previous = stateRef.current;
    if (!previous) return;
    const next = { ...previous, settings: { ...previous.settings, theme } };
    stateRef.current = next;
    setState(next);
  };

  const navClass = (active: boolean) =>
    `apple-sidebar-nav-button ${active ? "bg-[var(--selection-bg)] font-semibold text-accent" : "font-normal hover:bg-black/5 dark:hover:bg-white/8"}`;

  return (
    <FeedbackProvider>
      <div className="flex h-full min-h-0 flex-col">
        <div className="apple-window-chrome">
          <div
            data-tauri-drag-region
            className={`apple-sidebar-shell ${sidebarCollapsed ? "apple-sidebar--collapsed" : ""}`}
          >
            <div
              className="apple-sidebar-brand flex h-full w-fit cursor-pointer items-center"
              role="button"
              tabIndex={0}
              aria-label="CGswitch"
              onClick={toggleSidebar}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") toggleSidebar();
              }}
              onMouseEnter={() => setSidebarFlyoutArmed(true)}
              onMouseLeave={() => setSidebarFlyoutArmed(false)}
            >
              <img src="/logo.svg" alt="CGswitch" className="dark:invert" draggable="false" />
              <span className="apple-sidebar-label apple-wordmark whitespace-nowrap">CGswitch</span>
            </div>
            {sidebarFlyoutArmed ? (
              <span className="apple-sidebar-flyout" aria-hidden="true">{sidebarCollapsed ? "展开侧边栏" : "收缩侧边栏"}</span>
            ) : null}
          </div>
          <div data-tauri-drag-region className="min-w-0 flex-1 self-stretch" />
          <div className="flex h-full items-center">
            <button type="button" className="window-control-button" aria-label="最小化" onClick={() => void appWindow?.minimize()}><Minus strokeWidth={2} aria-hidden="true" /></button>
            <button type="button" className="window-control-button" aria-label="最大化" onClick={() => void appWindow?.toggleMaximize()}><Square strokeWidth={2} aria-hidden="true" /></button>
            <button type="button" className="window-control-button window-control-button--close" aria-label="关闭" onClick={() => void appWindow?.close()}><X strokeWidth={2} aria-hidden="true" /></button>
          </div>
        </div>

        <div className="flex min-h-0 flex-1">
          <aside className={`apple-sidebar relative h-full shrink-0 ${sidebarCollapsed ? "apple-sidebar--collapsed" : ""}`}>
            <nav ref={sidebarNavRef} className="relative mx-1.5 mt-3 space-y-1">
              <span className={`apple-sidebar-indicator ${indicator.instant ? "apple-sidebar-indicator--instant" : ""}`} style={{ top: `${indicator.top}px`, left: `${indicator.left}px` }} aria-hidden="true" />
              <button ref={profileNavRef} type="button" className={navClass(view === "profiles")} aria-label="供应商配置" onClick={goProfiles} onMouseEnter={() => setSidebarFlyoutArmed(true)}>
                <Layers2 strokeWidth={2} aria-hidden="true" />
                <span className="apple-sidebar-label" aria-hidden={sidebarCollapsed}>供应商配置</span>
                {sidebarCollapsed && sidebarFlyoutArmed ? <span className="apple-sidebar-flyout" aria-hidden="true">供应商配置</span> : null}
              </button>
              <button ref={mcpNavRef} type="button" className={navClass(view === "mcp")} aria-label="MCP 管理" onClick={goMcp} onMouseEnter={() => setSidebarFlyoutArmed(true)}>
                <McpIcon className="h-[18px] w-[18px]" />
                <span className="apple-sidebar-label" aria-hidden={sidebarCollapsed}>MCP 管理</span>
                {sidebarCollapsed && sidebarFlyoutArmed ? <span className="apple-sidebar-flyout" aria-hidden="true">MCP 管理</span> : null}
              </button>
            </nav>
            <div className="absolute inset-x-1.5 bottom-4">
              <button ref={settingsNavRef} type="button" className={navClass(view === "settings")} aria-label="设置" onClick={() => setView("settings")} onMouseEnter={() => setSidebarFlyoutArmed(true)}>
                <SettingsIcon strokeWidth={2} aria-hidden="true" />
                <span className="apple-sidebar-label" aria-hidden={sidebarCollapsed}>设置</span>
                {sidebarCollapsed && sidebarFlyoutArmed ? <span className="apple-sidebar-flyout" aria-hidden="true">设置</span> : null}
              </button>
            </div>
          </aside>

          <main className="min-w-0 flex-1 overflow-y-auto overflow-x-hidden bg-[var(--app-bg)] pt-4">
            {!state ? (
              <div className="startup-skeleton" aria-busy="true">
                <div className="startup-skeleton__title" />
                <div className="startup-skeleton__subtitle" />
                <div className="startup-skeleton__panel" />
                <div className="startup-skeleton__heading" />
                <div className="startup-skeleton__list" />
                {loadError ? <p className="muted mt-4 text-sm">{loadError}</p> : null}
              </div>
            ) : view === "profiles" ? (
              <ProfilesView key={profilesReset} state={state} activationEpoch={activationEpoch} onRefresh={refresh} />
            ) : view === "mcp" ? (
              <McpView key={mcpReset} />
            ) : (
              <SettingsView state={state} onPreviewTheme={previewTheme} onRefresh={refresh} onSaved={updateSettings} onHome={goProfiles} />
            )}
          </main>
        </div>
      </div>
    </FeedbackProvider>
  );
}
