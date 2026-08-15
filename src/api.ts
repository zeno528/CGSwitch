import { invoke } from "@tauri-apps/api/core";
import type {
  AppState,
  AuthStatus,
  DeviceCodeResponse,
  ManagedAccount,
  ProfileDetail,
  ProfileSummary,
  RestartStage,
  Settings,
} from "./types";

export const isTauri = typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

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

interface WebDetail {
  base_url: string | null;
  api_key: string | null;
  model_values: Record<string, string>;
  config_fragment: string;
}

const webDetails: Record<string, WebDetail> = {
  "profile-zai-glm-high": {
    base_url: "https://open.bigmodel.cn/api/v1",
    api_key: "sk-demo",
    model_values: {
      model: '"glm-5.3"',
      model_reasoning_effort: '"high"',
      model_catalog_json: '"zai.json"',
    },
    config_fragment:
      'model = "glm-5.3"\nmodel_reasoning_effort = "high"\nmodel_catalog_json = "zai.json"\n\n[model_providers.ZAI]\nname = "ZAI"\nbase_url = "https://open.bigmodel.cn/api/v1"\nwire_api = "responses"\nexperimental_bearer_token = "••••••••"',
  },
  "profile-zai-glm-fast": {
    base_url: "https://open.bigmodel.cn/api/v1",
    api_key: null,
    model_values: {
      model: '"glm-5-turbo"',
      model_reasoning_effort: '"low"',
      model_catalog_json: '"zai.json"',
    },
    config_fragment:
      'model = "glm-5-turbo"\nmodel_reasoning_effort = "low"\nmodel_catalog_json = "zai.json"\n\n[model_providers.ZAI]\nname = "ZAI"\nbase_url = "https://open.bigmodel.cn/api/v1"\nwire_api = "responses"',
  },
  "profile-official": {
    base_url: null,
    api_key: null,
    model_values: {
      model: '"gpt-5.6"',
      model_reasoning_effort: '"medium"',
    },
    config_fragment: 'model = "gpt-5.6"\nmodel_reasoning_effort = "medium"',
  },
};

function webProfileDetail(id: string): ProfileDetail {
  const profile = webProfiles.find((item) => item.id === id);
  if (!profile) throw new Error("配置档案不存在");
  const detail = webDetails[id];
  return {
    id: profile.id,
    name: profile.name,
    icon: profile.icon,
    provider: profile.provider,
    base_url: detail?.base_url ?? null,
    api_key: detail?.api_key ?? null,
    model_values: detail?.model_values ?? {},
    config_fragment: detail?.config_fragment ?? "",
    updated_at: profile.updated_at,
  };
}

let webSettings: Settings = {
  theme: "system",
  codex_app_path: null,
  auto_restart: false,
  restart_timeout_ms: 5000,
  autostart_enabled: false,
  silent_start: false,
  minimize_to_tray: false,
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
    case "get_profile":
      return webProfileDetail(String(args?.id)) as T;
    case "update_profile": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("配置档案不存在");
      profile.name = String(args?.name ?? profile.name);
      const detail = webDetails[profile.id];
      if (detail) {
        if (typeof args?.baseUrl === "string") detail.base_url = args.baseUrl || null;
        if (typeof args?.apiKey === "string") detail.api_key = args.apiKey || null;
      }
      return { ...profile } as T;
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
    case "set_window_theme":
      return undefined as T;
    case "auth_get_status":
      return { authenticated: false, default_account_id: null, accounts: [] } as T;
    case "open_url":
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
  getProfile: (id: string) => call<ProfileDetail>("get_profile", { id }),
  updateProfile: (id: string, name: string, baseUrl?: string, apiKey?: string) =>
    call<ProfileSummary>("update_profile", { id, name, baseUrl, apiKey }),
  deleteProfile: (id: string) => call<void>("delete_profile", { id }),
  applyProfile: (id: string) => call<void>("apply_profile", { id }),
  restartCodex: () => call<void>("restart_codex"),
  setWindowTheme: (dark: boolean) => call<void>("set_window_theme", { dark }),
  authStartLogin: () => call<DeviceCodeResponse>("auth_start_login"),
  authPollForAccount: (deviceCode: string) =>
    call<ManagedAccount | null>("auth_poll_for_account", { deviceCode }),
  authGetStatus: () => call<AuthStatus>("auth_get_status"),
  authRemoveAccount: (accountId: string) =>
    call<void>("auth_remove_account", { accountId }),
  openUrl: (url: string) => call<void>("open_url", { url }),
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
