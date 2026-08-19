export interface ProfileSummary {
  id: string;
  name: string;
  /** 官方档案绑定的订阅账号 id；第三方为 null。 */
  account_id: string | null;
  model: string | null;
  provider: string | null;
  reasoning_effort: string | null;
  has_key: boolean;
  admin_url: string | null;
  /** 供应商级开关：是否在卡片显示并自动刷新余额/用量。 */
  show_balance: boolean;
  icon: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProfileDetail {
  id: string;
  name: string;
  /** 官方档案绑定的订阅账号 id；第三方为 null。 */
  account_id: string | null;
  icon: string | null;
  provider: string | null;
  base_url: string | null;
  api_key: string | null;
  model_values: Record<string, string>;
  config_fragment: string;
  /** 供应商自己保存的完整 config 原文（内置供应商可全量编辑；普通供应商为片段）。 */
  raw_config: string | null;
  auth_content: string | null;
  catalog_content: string | null;
  /** 供应商自己保存的 models.json 原文。 */
  raw_catalog: string | null;
  /** 供应商自己保存的 auth.json 原文。 */
  raw_auth: string | null;
  admin_url: string | null;
  show_balance: boolean;
  updated_at: string;
}

export interface ProfileConnectionResult {
  ok: boolean;
  latency_ms: number | null;
  status: number | null;
  error: string | null;
}

/** ~/.codex/config.toml [mcp_servers.*] 的一条服务器配置（建模字段子集；未建模键由后端原样保留）。 */
export interface McpServerSpec {
  name: string;
  /** null = 未写入该键（Codex 默认启用）；false 显式停用。 */
  enabled: boolean | null;
  startup_timeout_sec: number | null;
  tool_timeout_sec: number | null;
  /** STDIO 传输：有 command 无 url。 */
  command: string | null;
  args: string[];
  env: Record<string, string>;
  /** Streamable HTTP 传输：有 url 无 command。 */
  url: string | null;
  bearer_token_env_var: string | null;
  http_headers: Record<string, string>;
  env_http_headers: Record<string, string>;
}

export interface TomlDiagnostic {
  from: number;
  to: number;
  message: string;
}

export interface EditorDiagnosticSummary {
  count: number;
  firstLine: number | null;
}

export interface ProfileBalanceInfo {
  currency: string;
  total_balance: string;
  granted_balance: string;
  topped_up_balance: string;
  /** 用量型供应商（如 MiniMax Token Plan）的剩余百分比；余额型供应商为 null。 */
  usage_percent: number | null;
  /** 5 小时窗口重置倒计时（如 "2h23m"）。 */
  usage_reset: string | null;
  /** 7 天窗口已用百分比；仅用量型供应商返回。 */
  weekly_usage_percent: number | null;
  /** 7 天窗口重置倒计时（如 "5d21h"）。 */
  weekly_reset: string | null;
}

export interface ProfileBalance {
  is_available: boolean;
  balance_infos: ProfileBalanceInfo[];
  latency_ms: number | null;
}

export interface DatabaseBackupInfo {
  name: string;
  size_bytes: number;
  created_at: number;
}

export interface DeviceCodeResponse {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface ManagedAccount {
  id: string;
  login: string;
  authenticated_at: number;
  is_default: boolean;
}

export interface AuthStatus {
  authenticated: boolean;
  default_account_id: string | null;
  accounts: ManagedAccount[];
  external: ManagedAccount | null;
}

export interface Settings {
  theme: "system" | "light" | "dark";
  auto_restart: boolean;
  autostart_enabled: boolean;
  silent_start: boolean;
  minimize_to_tray: boolean;
  auto_backup_interval_hours: number;
  database_backup_keep_count: number;
}

export interface CodexAppStatus {
  running: boolean;
  display_path: string;
  source: string;
}

export interface PathInfo {
  label: string;
  path: string;
}

export interface AppState {
  profiles: ProfileSummary[];
  active_profile_id: string | null;
  codex: CodexAppStatus;
  settings: Settings;
  paths: PathInfo[];
  /** 供应商级余额/用量缓存（上次成功查询结果），保证卡片静默显示不闪烁。 */
  balance_cache: Record<string, ProfileBalanceInfo>;
}

export type RestartStage = "idle" | "stopping" | "waiting" | "launching" | "success" | "error";
