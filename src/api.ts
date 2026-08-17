import { invoke } from "@tauri-apps/api/core";
import { builtinPresetByKind } from "./presets";
import type {
  AppState,
  AuthStatus,
  CodexAppStatus,
  DatabaseBackupInfo,
  DeepSeekBalance,
  DeepSeekBalanceInfo,
  DeviceCodeResponse,
  ManagedAccount,
  ProfileDetail,
  ProfileConnectionResult,
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
    account_id: null,
    model: "glm-5.3",
    provider: "ZAI",
    reasoning_effort: "high",
    has_key: true,
    admin_url: "https://open.bigmodel.cn/console",
    show_balance: false,
    icon: "zhipu",
    created_at: "2026-08-15 10:00:00",
    updated_at: "2026-08-15 10:00:00",
  },
  {
    id: "profile-zai-glm-fast",
    name: "ZAI GLM 快速",
    account_id: null,
    model: "glm-5-turbo",
    provider: "ZAI",
    reasoning_effort: "low",
    has_key: false,
    admin_url: null,
    show_balance: false,
    icon: null,
    created_at: "2026-08-15 10:01:00",
    updated_at: "2026-08-15 10:01:00",
  },
  {
    id: "profile-official",
    name: "官方默认",
    account_id: null,
    model: "gpt-5.6",
    provider: null,
    reasoning_effort: "medium",
    has_key: false,
    admin_url: null,
    show_balance: false,
    icon: "openai-chatgpt",
    created_at: "2026-08-15 10:02:00",
    updated_at: "2026-08-15 10:02:00",
  },
];

const webPaths = [
  { label: "数据库", path: "C:\\Users\\<user>\\.cgswitch\\cgswitch.db" },
  { label: "设置文件", path: "C:\\Users\\<user>\\.cgswitch\\settings.json" },
  { label: "Codex 配置", path: "C:\\Users\\<user>\\.codex\\config.toml" },
  { label: "配置备份", path: "C:\\Users\\<user>\\.cgswitch\\backups\\config" },
  { label: "数据库备份", path: "C:\\Users\\<user>\\.cgswitch\\backups\\database" },
];

interface WebDetail {
  base_url: string | null;
  api_key: string | null;
  model_values: Record<string, string>;
  config_fragment: string;
  raw_config?: string | null;
  raw_catalog?: string | null;
  raw_auth?: string | null;
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
  if (!profile) throw new Error("供应商配置不存在");
  const detail = webDetails[id];
  return {
    id: profile.id,
    name: profile.name,
    account_id: profile.account_id,
    icon: profile.icon,
    provider: profile.provider,
    base_url: detail?.base_url ?? null,
    api_key: detail?.api_key ?? null,
    model_values: detail?.model_values ?? {},
    config_fragment: detail?.config_fragment ?? "",
    raw_config: detail?.raw_config ?? null,
    auth_content: detail?.api_key
      ? '{\n  "OPENAI_API_KEY": "sk-demo-real-value"\n}'
      : null,
    catalog_content: detail?.model_values.model_catalog_json
      ? '{\n  "models": [\n    { "id": "glm-5.3", "name": "GLM 5.3" }\n  ]\n}'
      : null,
    raw_catalog: detail?.raw_catalog ?? null,
    raw_auth: detail?.raw_auth ?? null,
    admin_url: profile.admin_url,
    show_balance: profile.show_balance,
    updated_at: profile.updated_at,
  };
}

let webSettings: Settings = {
  theme: "system",
  auto_restart: false,
  autostart_enabled: false,
  silent_start: false,
  minimize_to_tray: false,
};

let webBackups: DatabaseBackupInfo[] = [];
// 与后端一致：激活状态只由“应用/捕获”显式建立，添加供应商不激活
let webActiveProfileId: string | null = null;
const webBalanceCache: Record<string, DeepSeekBalanceInfo> = {};

function webState(): AppState {
  return {
    profiles: [...webProfiles],
    active_profile_id: webActiveProfileId,
    codex: {
      running: true,
      display_path: "OpenAI.Codex_2p2nqsd0c76g0!App",
      source: "packaged-app",
    },
    settings: { ...webSettings },
    paths: webPaths,
    balance_cache: { ...webBalanceCache },
  };
}

async function webInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await new Promise((resolve) => setTimeout(resolve, 120));
  switch (command) {
    case "get_state":
    case "get_settings":
      return webState() as T;
    case "get_codex_status":
      return webState().codex as T;
    case "capture_profile": {
      const now = new Date().toISOString();
      const profile: ProfileSummary = {
        id: `profile-${Date.now()}`,
        name: String(args?.name ?? "新供应商"),
        account_id: null,
        model: "glm-5.3",
        provider: "ZAI",
        reasoning_effort: "high",
        has_key: true,
        admin_url: null,
        show_balance: false,
        icon: null,
        created_at: now,
        updated_at: now,
      };
      webProfiles.unshift(profile);
      // 捕获即建立“当前 live = 该供应商”的显式关联
      webActiveProfileId = profile.id;
      return profile as T;
    }
    case "add_builtin_profile": {
      const preset = builtinPresetByKind(String(args?.kind ?? ""));
      if (!preset) throw new Error("未知的内置供应商类型");
      const rawKey = String(args?.apiKey ?? "");
      const apiKey = preset.provider ? rawKey : null;
      const rawBaseUrl = String(args?.baseUrl ?? "");
      const baseUrl = preset.provider ? rawBaseUrl || preset.base_url : null;
      const rawAdminUrl = String(args?.adminUrl ?? "");
      const adminUrl = rawAdminUrl || preset.admin_url;
      const now = new Date().toISOString();
      const profile: ProfileSummary = {
        id: `profile-${Date.now()}`,
        name: preset.name,
        account_id: preset.provider ? null : (typeof args?.accountId === "string" ? args.accountId : null),
        model: preset.model,
        provider: preset.provider,
        reasoning_effort: preset.model_values.model_reasoning_effort?.replace(/^"|"$/g, "") ?? null,
        has_key: preset.provider ? Boolean(rawKey.trim()) : false,
        admin_url: adminUrl,
        show_balance: false,
        icon: preset.icon,
        created_at: now,
        updated_at: now,
      };
      webProfiles.unshift(profile);
      webDetails[profile.id] = {
        base_url: baseUrl,
        api_key: apiKey,
        model_values: preset.model_values,
        config_fragment: preset.fragment,
      };
      return profile as T;
    }
    case "add_custom_profile": {
      const now = new Date().toISOString();
      const profile: ProfileSummary = {
        id: `profile-${Date.now()}`,
        name: String(args?.name ?? "自定义供应商"),
        account_id: null,
        model: null,
        provider: null,
        reasoning_effort: null,
        has_key: Boolean(args?.apiKey),
        admin_url:
          typeof args?.adminUrl === "string" && args.adminUrl ? args.adminUrl : null,
        show_balance: false,
        icon: "custom",
        created_at: now,
        updated_at: now,
      };
      webProfiles.unshift(profile);
      webDetails[profile.id] = {
        base_url:
          typeof args?.baseUrl === "string" && args.baseUrl ? args.baseUrl : null,
        api_key:
          typeof args?.apiKey === "string" && args.apiKey ? args.apiKey : null,
        model_values: {},
        config_fragment: String(args?.configText ?? ""),
        raw_config: String(args?.configText ?? ""),
        raw_catalog:
          typeof args?.catalogText === "string" && args.catalogText ? args.catalogText : null,
        raw_auth: typeof args?.authText === "string" && args.authText ? args.authText : null,
      };
      return profile as T;
    }
    case "get_builtin_catalog": {
      const preset = builtinPresetByKind(String(args?.kind ?? ""));
      if (!preset?.model_values.model_catalog_json) return null as T;
      return '{\n  "models": [\n    { "id": "preview", "name": "模型目录预览" }\n  ]\n}' as T;
    }
    case "test_profile_connection": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      if (!profile.provider) throw new Error("该供应商缺少配置，无法测试连通性");
      const rawKey = args?.apiKey !== undefined ? String(args.apiKey) : "saved-key";
      if (!rawKey.trim()) throw new Error("请填写 API 密钥");
      const rawBase = args?.baseUrl !== undefined ? String(args.baseUrl) : "https://api.example.com";
      if (!rawBase.trim()) throw new Error("请填写调用地址");
      await new Promise((resolve) => setTimeout(resolve, 300));
      return { ok: true, latency_ms: 87, status: 200, error: null } as T;
    }
    case "get_deepseek_balance": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      if (profile.provider !== "deepseek") {
        throw new Error("该供应商不是 DeepSeek，无法查询余额");
      }
      await new Promise((resolve) => setTimeout(resolve, 400));
      return {
        is_available: true,
        balance_infos: [
          {
            currency: "CNY",
            total_balance: "110.00",
            granted_balance: "10.00",
            topped_up_balance: "100.00",
          },
        ],
        latency_ms: 210,
      } as T;
    }
    case "export_database": {
      const name = `cgswitch-export-${Date.now()}.db`;
      webBackups.unshift({ name, size_bytes: 20480, created_at: Math.floor(Date.now() / 1000) });
      return `C:\\Users\\<user>\\.cgswitch\\backups\\database\\${name}` as T;
    }
    case "import_database":
      return undefined as T;
    case "list_database_backups":
      return [...webBackups] as T;
    case "restore_database":
      return undefined as T;
    case "delete_database_backup": {
      webBackups = webBackups.filter((backup) => backup.name !== args?.name);
      return undefined as T;
    }
    case "rename_database_backup": {
      const backup = webBackups.find((item) => item.name === args?.oldName);
      if (backup) {
        let stem = String(args?.title ?? "").trim();
        if (stem.startsWith("cgswitch-export-")) stem = stem.slice("cgswitch-export-".length);
        if (stem.endsWith(".db")) stem = stem.slice(0, -3);
        stem = stem.replace(/[<>:"/\\|?*]/g, "").trim();
        if (stem) backup.name = `cgswitch-export-${stem}.db`;
      }
      return undefined as T;
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
    case "set_profile_show_balance": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (profile) profile.show_balance = Boolean(args?.enabled);
      return undefined as T;
    }
    case "set_profile_balance": {
      if (typeof args?.id === "string" && args?.info) {
        webBalanceCache[args.id] = args.info as DeepSeekBalanceInfo;
      }
      return undefined as T;
    }
    case "duplicate_profile": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      const now = new Date().toISOString();
      const copy: ProfileSummary = {
        ...profile,
        id: `profile-${Date.now()}`,
        name: `${profile.name} 副本`,
        created_at: now,
        updated_at: now,
      };
      webProfiles.unshift(copy);
      if (webDetails[profile.id]) webDetails[copy.id] = { ...webDetails[profile.id] };
      return copy as T;
    }
    case "get_profile":
      return webProfileDetail(String(args?.id)) as T;
    case "update_profile": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      profile.name = String(args?.name ?? profile.name);
      const detail = webDetails[profile.id];
      if (detail) {
        if (typeof args?.baseUrl === "string") detail.base_url = args.baseUrl || null;
        if (typeof args?.apiKey === "string") detail.api_key = args.apiKey || null;
      }
      if (typeof args?.adminUrl === "string") profile.admin_url = args.adminUrl || null;
      return { ...profile } as T;
    }
    case "update_profile_config": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      const detail = webDetails[profile.id];
      if (detail) {
        if (typeof args?.configText === "string") detail.raw_config = args.configText;
        if (typeof args?.catalogText === "string") detail.raw_catalog = args.catalogText;
        if (typeof args?.authText === "string") detail.raw_auth = args.authText;
      }
      return webProfileDetail(profile.id) as T;
    }
    case "delete_profile": {
      const index = webProfiles.findIndex((item) => item.id === args?.id);
      if (index >= 0) webProfiles.splice(index, 1);
      return undefined as T;
    }
    case "apply_profile":
      webActiveProfileId = typeof args?.id === "string" ? args.id : null;
      await new Promise((resolve) => setTimeout(resolve, 500));
      return undefined as T;
    case "restart_codex":
      await new Promise((resolve) => setTimeout(resolve, 500));
      return undefined as T;
    case "set_window_theme":
      return undefined as T;
    case "auth_get_status":
      return { authenticated: false, default_account_id: null, accounts: [] } as T;
    case "auth_apply_to_codex":
      return undefined as T;
    case "auth_set_default_account":
      return undefined as T;
    case "set_profile_account": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (profile) {
        profile.account_id = typeof args?.accountId === "string" ? args.accountId : null;
      }
      return undefined as T;
    }
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
  getCodexStatus: () => call<CodexAppStatus>("get_codex_status"),
  captureProfile: (name: string) => call<ProfileSummary>("capture_profile", { name }),
  addBuiltinProfile: (
    kind: string,
    baseUrl?: string,
    apiKey?: string,
    adminUrl?: string,
    accountId?: string,
  ) => call<ProfileSummary>("add_builtin_profile", { kind, baseUrl, apiKey, adminUrl, accountId }),
  addCustomProfile: (
    name: string,
    configText: string,
    baseUrl?: string,
    apiKey?: string,
    adminUrl?: string,
    catalogText?: string | null,
    authText?: string | null,
  ) =>
    call<ProfileSummary>("add_custom_profile", {
      name,
      configText,
      baseUrl,
      apiKey,
      adminUrl,
      catalogText,
      authText,
    }),
  getBuiltinCatalog: (kind: string) => call<string | null>("get_builtin_catalog", { kind }),
  testProfileConnection: (id: string, baseUrl?: string, apiKey?: string) =>
    call<ProfileConnectionResult>("test_profile_connection", { id, baseUrl, apiKey }),
  getDeepseekBalance: (id: string) =>
    call<DeepSeekBalance>("get_deepseek_balance", { id }),
  exportDatabase: () => call<string>("export_database"),
  importDatabase: (path: string) => call<void>("import_database", { path }),
  listDatabaseBackups: () => call<DatabaseBackupInfo[]>("list_database_backups"),
  restoreDatabase: (name: string) => call<void>("restore_database", { name }),
  deleteDatabaseBackup: (name: string) => call<void>("delete_database_backup", { name }),
  renameDatabaseBackup: (oldName: string, title: string) =>
    call<void>("rename_database_backup", { oldName, title }),
  renameProfile: (id: string, name: string) => call<void>("rename_profile", { id, name }),
  setProfileIcon: (id: string, icon: string | null) => call<void>("set_profile_icon", { id, icon }),
  setProfileShowBalance: (id: string, enabled: boolean) =>
    call<void>("set_profile_show_balance", { id, enabled }),
  setProfileBalance: (id: string, info: DeepSeekBalanceInfo) =>
    call<void>("set_profile_balance", { id, info }),
  setProfileAccount: (id: string, accountId: string | null) =>
    call<void>("set_profile_account", { id, accountId }),
  duplicateProfile: (id: string) => call<ProfileSummary>("duplicate_profile", { id }),
  getProfile: (id: string) => call<ProfileDetail>("get_profile", { id }),
  updateProfile: (id: string, name: string, baseUrl?: string, apiKey?: string, adminUrl?: string) =>
    call<ProfileSummary>("update_profile", { id, name, baseUrl, apiKey, adminUrl }),
  updateProfileConfig: (
    id: string,
    configText: string,
    catalogText: string | null,
    authText: string | null,
  ) => call<ProfileDetail>("update_profile_config", { id, configText, catalogText, authText }),
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
  authSetDefaultAccount: (accountId: string) =>
    call<void>("auth_set_default_account", { accountId }),
  authApplyToCodex: (accountId: string) =>
    call<void>("auth_apply_to_codex", { accountId }),
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
