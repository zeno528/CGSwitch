import { invoke } from "@tauri-apps/api/core";
import type { AppState, ProfileSummary, RestartStage, Settings } from "./types";

const isTauri = typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

type RestartProgressHandler = (payload: { stage: RestartStage; message: string | null }) => void;

const webProfiles: ProfileSummary[] = [
  {
    id: "profile-zai-glm-high",
    name: "ZAI GLM 高推理",
    model: "glm-5.3",
    provider: "ZAI",
    reasoning_effort: "high",
    icon: "zhipu",
    created_at: "2026-08-15 10:00:00",
    updated_at: "2026-08-15 10:00:00",
  },
  {
    id: "profile-zai-glm-fast",
    name: "ZAI GLM 快速",
    model: "glm-5-turbo",
    provider: "ZAI",
    reasoning_effort: "low",
    icon: null,
    created_at: "2026-08-15 10:01:00",
    updated_at: "2026-08-15 10:01:00",
  },
  {
    id: "profile-official",
    name: "官方默认",
    model: "gpt-5.6",
    provider: null,
    reasoning_effort: "medium",
    icon: "openai-chatgpt",
    created_at: "2026-08-15 10:02:00",
    updated_at: "2026-08-15 10:02:00",
  },
];

const webPaths = [
  { label: "数据库", path: "C:\\Users\\<user>\\.switchgpt\\switchgpt.db" },
  { label: "Codex 配置", path: "C:\\Users\\<user>\\.codex\\config.toml" },
  { label: "配置备份", path: "C:\\Users\\<user>\\.switchgpt\\backups\\config" },
  { label: "数据库备份", path: "C:\\Users\\<user>\\.switchgpt\\backups\\database" },
  { label: "日志", path: "C:\\Users\\<user>\\.switchgpt\\logs" },
];

let webSettings: Settings = {
  theme: "system",
  codex_app_path: null,
  auto_restart: false,
  restart_timeout_ms: 5000,
};

function webState(): AppState {
  return {
    profiles: [...webProfiles],
    active_profile_id: webProfiles[0]?.id ?? null,
    codex: {
      running: true,
      display_path: "OpenAI.Codex_2p2nqsd0c76g0!App",
      source: "packaged-app",
    },
    settings: { ...webSettings },
    paths: webPaths,
  };
}

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, 120));
  switch (command) {
    case "get_state":
    case "get_settings":
      return webState() as T;
    case "capture_profile": {
      const now = new Date().toISOString();
      const profile: ProfileSummary = {
        id: `profile-${Date.now()}`,
        name: String(args?.name ?? "新档案"),
        model: "glm-5.3",
        provider: "ZAI",
        reasoning_effort: "high",
        icon: null,
        created_at: now,
        updated_at: now,
      };
      webProfiles.unshift(profile);
      return profile as T;
    }
    case "rename_profile": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (profile) profile.name = String(args?.name ?? profile.name);
      return undefined as T;
    }
    case "set_profile_icon": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (profile) profile.icon = (args?.icon as string | null) ?? null;
      return undefined as T;
    }
    case "delete_profile": {
      const index = webProfiles.findIndex((item) => item.id === args?.id);
      if (index >= 0) webProfiles.splice(index, 1);
      return undefined as T;
    }
    case "apply_profile":
    case "restart_codex":
      await new Promise((resolve) => setTimeout(resolve, 500));
      return undefined as T;
    case "save_settings":
      webSettings = { ...(args?.settings as Settings) };
      return { ...webSettings } as T;
    default:
      throw new Error(`Web 调试模式不支持命令：${command}`);
  }
}

function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return isTauri ? invoke<T>(command, args) : webInvoke<T>(command, args);
}

export const api = {
  getState: () => call<AppState>("get_state"),
  captureProfile: (name: string) => call<ProfileSummary>("capture_profile", { name }),
  renameProfile: (id: string, name: string) => call<void>("rename_profile", { id, name }),
  setProfileIcon: (id: string, icon: string | null) => call<void>("set_profile_icon", { id, icon }),
  deleteProfile: (id: string) => call<void>("delete_profile", { id }),
  applyProfile: (id: string) => call<void>("apply_profile", { id }),
  restartCodex: () => call<void>("restart_codex"),
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (settings: Settings) => call<Settings>("save_settings", { settings }),
  openPath: (path: string) => call<void>("open_path", { path }),
  onRestartProgress: async (handler: RestartProgressHandler) => {
    if (!isTauri) return () => undefined;
    const { listen } = await import("@tauri-apps/api/event");
    return listen("restart-progress", (event) =>
      handler(event.payload as { stage: RestartStage; message: string | null }),
    );
  },
};
