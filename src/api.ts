import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { balanceQueryProviders, builtinPresetByKind } from "./presets";
import type {
  AppState,
  AuthStatus,
  CodexAppStatus,
  DatabaseBackupInfo,
  DeviceCodeResponse,
  ManagedAccount,
  McpServerSpec,
  McpSyncPreview,
  ProfileBalance,
  ProfileBalanceInfo,
  ProfileDetail,
  ProfileConnectionResult,
  ProfileSummary,
  RestartStage,
  Settings,
  TomlDiagnostic,
} from "./types";
import { stripTomlQuotes } from "./utils";

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
  { label: "应用数据目录", path: "C:\\Users\\<user>\\.cgswitch" },
  { label: "Codex 配置", path: "C:\\Users\\<user>\\.codex\\config.toml" },
  { label: "备份目录", path: "C:\\Users\\<user>\\.cgswitch\\backups" },
];

function patchContextOverrideForWeb(text: string, enabled: boolean): string {
  const newline = text.includes("\r\n") ? "\r\n" : "\n";
  const lines = text
    .split(/\r?\n/)
    .filter(
      (line) =>
        !/^\s*model_context_window\s*=/.test(line) &&
        !/^\s*model_auto_compact_token_limit\s*=/.test(line),
    );
  if (enabled) {
    const sectionIndex = lines.findIndex((line) => /^\s*\[/.test(line));
    lines.splice(sectionIndex < 0 ? lines.length : sectionIndex, 0, "model_context_window = 1000000", "model_auto_compact_token_limit = 900000");
  }
  return lines.join(newline);
}

// 从 2xx 的 JSON 响应体里识别供应商级错误（OpenAI 风格 error 或智谱风格 code/success）。
function connectionErrorFromBody(value: unknown): string | null {
  if (!value || typeof value !== "object") return null;
  const json = value as Record<string, unknown>;
  if (json.error !== undefined) {
    const error = json.error;
    const message =
      error && typeof error === "object"
        ? (error as Record<string, unknown>).message
        : error;
    return typeof message === "string" && message ? message : "接口返回错误";
  }
  if (json.success === false) {
    const message = typeof json.msg === "string" ? json.msg : json.message;
    return typeof message === "string" && message ? message : "接口返回错误";
  }
  const code = json.code;
  const codeNumber =
    typeof code === "number" ? code : typeof code === "string" ? Number(code) : NaN;
  if (Number.isFinite(codeNumber) && codeNumber >= 400) {
    const message = typeof json.msg === "string" ? json.msg : json.message;
    return typeof message === "string" && message ? message : "接口返回错误";
  }
  return null;
}

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
  auto_backup_interval_hours: 0,
  database_backup_keep_count: 5,
};

let webBackups: DatabaseBackupInfo[] = [];
// MCP 页 web 调试样例：stdio + http 各一条，字段对齐 Codex 官方配置格式
let webMcpServers: McpServerSpec[] = [
  {
    name: "github",
    enabled: null,
    startup_timeout_sec: null,
    tool_timeout_sec: null,
    command: "github-mcp-server",
    args: ["stdio"],
    env: { GITHUB_PERSONAL_ACCESS_TOKEN: "ghp_demo" },
    url: null,
    bearer_token_env_var: null,
    http_headers: {},
    env_http_headers: {},
  },
  {
    name: "tavily",
    enabled: null,
    startup_timeout_sec: null,
    tool_timeout_sec: null,
    command: null,
    args: [],
    env: {},
    url: "https://mcp.tavily.com/mcp",
    bearer_token_env_var: "TAVILY_API_KEY",
    http_headers: {},
    env_http_headers: {},
  },
];
// 与后端一致：激活状态只由“应用/捕获”显式建立，添加供应商不激活
let webActiveProfileId: string | null = null;
const webBalanceCache: Record<string, ProfileBalanceInfo> = {};

function databaseBackupName(date = new Date()): string {
  const pad = (value: number, length = 2) => String(value).padStart(length, "0");
  return `cg-backup-${date.getFullYear()}${pad(date.getMonth() + 1)}${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}${pad(date.getSeconds())}-${pad(date.getMilliseconds(), 3)}.db`;
}

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
      webProfiles.push(profile);
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
        reasoning_effort: stripTomlQuotes(preset.model_values.model_reasoning_effort) || null,
        has_key: preset.provider ? Boolean(rawKey.trim()) : false,
        admin_url: adminUrl,
        show_balance: false,
        icon: preset.icon,
        created_at: now,
        updated_at: now,
      };
      webProfiles.push(profile);
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
      webProfiles.push(profile);
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
    case "test_provider_connection": {
      const apiKey = String(args?.apiKey ?? "");
      const baseUrl = String(args?.baseUrl ?? "");
      if (!apiKey.trim()) throw new Error("请填写 API 密钥");
      if (!baseUrl.trim()) throw new Error("请填写调用地址");
      const url = `${baseUrl.replace(/\/+$/, "")}/models`;
      const start = Date.now();
      try {
        const res = await fetch(url, {
          headers: { Authorization: `Bearer ${apiKey.trim()}` },
        });
        return {
          ok: res.ok,
          latency_ms: Date.now() - start,
          status: res.status,
          error: res.ok ? null : `接口返回 HTTP ${res.status}`,
        } as T;
      } catch {
        throw new Error("网络请求被浏览器拦截，请在桌面版验证连通性");
      }
    }
    case "test_profile_connection": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      if (!profile.provider) {
        // 官方订阅：网页调试模式直接模拟认证连通正常
        return { ok: true, latency_ms: 12, status: 200, error: null } as T;
      }
      const apiKey = args?.apiKey !== undefined ? String(args.apiKey) : "saved-key";
      if (!apiKey.trim()) throw new Error("请填写 API 密钥");
      const baseUrl = args?.baseUrl !== undefined ? String(args.baseUrl) : "https://api.example.com";
      if (!baseUrl.trim()) throw new Error("请填写调用地址");
      // 网页调试模式做真实请求，避免“随便填都能成功”的假象；
      // 跨域被浏览器拦截时明确提示用桌面版验证
      const url = `${baseUrl.replace(/\/+$/, "")}/models`;
      const start = Date.now();
      try {
        const res = await fetch(url, {
          headers: { Authorization: `Bearer ${apiKey.trim()}` },
        });
        const latency_ms = Date.now() - start;
        if (res.ok) {
          const text = await res.text();
          let json: unknown = null;
          try {
            json = JSON.parse(text);
          } catch {
            return {
              ok: false,
              latency_ms,
              status: res.status,
              error: `接口返回 HTTP ${res.status}，但响应不是有效的 JSON（请检查调用地址）`,
            } as T;
          }
          const error = connectionErrorFromBody(json);
          if (error) {
            return { ok: false, latency_ms, status: res.status, error } as T;
          }
          return { ok: true, latency_ms, status: res.status, error: null } as T;
        }
        if (res.status === 401 || res.status === 403) {
          return { ok: false, latency_ms, status: res.status, error: "API 密钥无效" } as T;
        }
        return { ok: false, latency_ms, status: res.status, error: `接口返回 HTTP ${res.status}` } as T;
      } catch {
        return {
          ok: false,
          latency_ms: null,
          status: null,
          error: "连接失败：浏览器跨域限制无法真实请求，请用桌面版验证",
        } as T;
      }
    }
    case "get_profile_balance": {
      const profile = webProfiles.find((item) => item.id === args?.id);
      if (!profile) throw new Error("供应商配置不存在");
      if (!balanceQueryProviders.has(profile.provider ?? "")) {
        throw new Error("该供应商不支持余额查询");
      }
      await new Promise((resolve) => setTimeout(resolve, 400));
      if (profile.provider === "minimax") {
        return {
          is_available: true,
          balance_infos: [
            {
              currency: "",
              total_balance: "",
              granted_balance: "",
              topped_up_balance: "",
              usage_percent: 15,
              usage_reset: "2h23m",
              weekly_usage_percent: 4,
              weekly_reset: "5d21h",
            },
          ],
          latency_ms: 210,
        } as T;
      }
      return {
        is_available: true,
        balance_infos: [
          {
            currency: "CNY",
            total_balance: "110.00",
            granted_balance: "10.00",
            topped_up_balance: "100.00",
            usage_percent: null,
            usage_reset: null,
            weekly_usage_percent: null,
            weekly_reset: null,
          },
        ],
        latency_ms: 210,
      } as T;
    }
    case "export_database": {
      const name = databaseBackupName();
      webBackups.unshift({ name, size_bytes: 20480, created_at: Math.floor(Date.now() / 1000) });
      return `C:\\Users\\<user>\\.cgswitch\\backups\\database\\${name}` as T;
    }
    case "export_database_to": {
      const name = databaseBackupName();
      const directory = String(args?.directory ?? "C:\\Users\\<user>\\Downloads");
      return `${directory}\\${name}` as T;
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
        if (stem.startsWith("cg-backup-")) stem = stem.slice("cg-backup-".length);
        if (stem.startsWith("cgswitch-export-")) stem = stem.slice("cgswitch-export-".length);
        if (stem.endsWith(".db")) stem = stem.slice(0, -3);
        stem = stem.replace(/[<>:"/\\|?*]/g, "").trim();
        if (stem) backup.name = `cg-backup-${stem}.db`;
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
        webBalanceCache[args.id] = args.info as ProfileBalanceInfo;
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
      webProfiles.push(copy);
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
    case "patch_chatgpt_context_config":
      return patchContextOverrideForWeb(
        String(args?.configText ?? ""),
        Boolean(args?.enabled),
      ) as T;
    case "delete_profile": {
      const index = webProfiles.findIndex((item) => item.id === args?.id);
      if (index >= 0) webProfiles.splice(index, 1);
      return undefined as T;
    }
    case "reorder_profiles": {
      const ids = Array.isArray(args?.ids) ? (args.ids as string[]) : [];
      webProfiles.sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id));
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
      return { authenticated: false, default_account_id: null, accounts: [], external: null } as T;
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
    // Web 调试模式无 Rust 侧解析器，TOML 校验一律视为通过（与既有 mock 桩风格一致）
    case "validate_toml":
      return [] as T;
    case "list_mcp_servers":
      return [...webMcpServers] as T;
    case "get_mcp_section_toml": {
      // 创建表单预填用：把 mock 列表渲染成 config.toml 片段
      const lines: string[] = [];
      for (const server of webMcpServers) {
        lines.push(`[mcp_servers.${server.name}]`);
        if (server.command) lines.push(`command = "${server.command}"`);
        if (server.args.length) {
          lines.push(`args = [${server.args.map((arg) => `"${arg}"`).join(", ")}]`);
        }
        if (server.url) lines.push(`url = "${server.url}"`);
        if (server.bearer_token_env_var) {
          lines.push(`bearer_token_env_var = "${server.bearer_token_env_var}"`);
        }
        const env = Object.entries(server.env);
        if (env.length) {
          lines.push(`[mcp_servers.${server.name}.env]`);
          for (const [key, value] of env) lines.push(`${key} = "${value}"`);
        }
        lines.push("");
      }
      return lines.join("\n") as T;
    }
    case "restore_mcp_from_database":
      return webMcpServers.length as T;
    case "import_mcp_from_live":
      return webMcpServers.length as T;
    case "mcp_sync_preview": {
      // web 调试探样例：一条“内容不同”+ 一条“仅配置文件”，便于在 pnpm dev 里走查差异弹窗
      const first = webMcpServers[0];
      const changed = first
        ? {
            name: first.name,
            kind: "changed" as const,
            unmodeled_only: false,
            live_spec: first,
            db_spec: { ...first, url: first.url ? first.url + "-old" : first.url },
            live_toml: null,
            db_toml: null,
            changed_fields: [
              { field: "url", live: first.url, db: first.url ? first.url + "-old" : null },
            ],
          }
        : null;
      const liveOnly = {
        name: "web-demo-live-only",
        kind: "live_only" as const,
        unmodeled_only: false,
        live_spec: {
          name: "web-demo-live-only",
          enabled: null,
          startup_timeout_sec: null,
          tool_timeout_sec: null,
          command: "npx",
          args: ["-y", "web-demo"],
          env: {},
          url: null,
          bearer_token_env_var: null,
          http_headers: {},
          env_http_headers: {},
        },
        db_spec: null,
        live_toml: null,
        db_toml: null,
        changed_fields: [],
      };
      const entries = [changed, liveOnly].filter((entry) => entry !== null);
      return {
        entries,
        live_count: webMcpServers.length + 1,
        db_count: webMcpServers.length,
      } as T;
    }
    case "save_mcp_server": {
      const spec = args?.spec as McpServerSpec;
      const original = typeof args?.originalName === "string" ? args.originalName : null;
      webMcpServers = webMcpServers.filter((server) => server.name !== (original ?? spec.name));
      webMcpServers.push(spec);
      return undefined as T;
    }
    case "delete_mcp_server":
      webMcpServers = webMcpServers.filter((server) => server.name !== args?.name);
      return undefined as T;
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
  // 创建态表单测试：供应商尚未保存，直接用表单里的地址/密钥
  testProviderConnection: (baseUrl: string, apiKey: string) =>
    call<ProfileConnectionResult>("test_provider_connection", { baseUrl, apiKey }),
  getProfileBalance: (id: string) =>
    call<ProfileBalance>("get_profile_balance", { id }),
  exportDatabase: () => call<string>("export_database"),
  exportDatabaseTo: (directory: string) => call<string>("export_database_to", { directory }),
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
  setProfileBalance: (id: string, info: ProfileBalanceInfo) =>
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
  patchChatgptContextConfig: (configText: string, enabled: boolean) =>
    call<string>("patch_chatgpt_context_config", { configText, enabled }),
  validateToml: (text: string) => call<TomlDiagnostic[]>("validate_toml", { text }),
  formatToml: (text: string) => call<string>("format_toml", { text }),
  deleteProfile: (id: string) => call<void>("delete_profile", { id }),
  reorderProfiles: (ids: string[]) => call<void>("reorder_profiles", { ids }),
  applyProfile: (id: string) => call<void>("apply_profile", { id }),
  listMcpServers: () => call<McpServerSpec[]>("list_mcp_servers"),
  // 创建表单预填用：当前全局 MCP 段的 TOML 文本（无 MCP 返回空串）
  getMcpSectionToml: () => call<string>("get_mcp_section_toml"),
  // 显式恢复：数据库镜像写回 live config.toml，返回恢复数量
  restoreMcpFromDatabase: () => call<number>("restore_mcp_from_database"),
  // 显式导入：live 当前 MCP 段强制镜像进数据库，返回导入数量
  importMcpFromLive: () => call<number>("import_mcp_from_live"),
  // 同步预览：对比 live 与数据库镜像的 MCP 差异（只读），供同步前人工裁决
  mcpSyncPreview: () => call<McpSyncPreview>("mcp_sync_preview"),
  saveMcpServer: (originalName: string | null, spec: McpServerSpec) =>
    call<void>("save_mcp_server", { originalName, spec }),
  deleteMcpServer: (name: string) => call<void>("delete_mcp_server", { name }),
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
    return listen("restart-progress", (event) =>
      handler(event.payload as { stage: RestartStage; message: string | null }),
    );
  },
};
