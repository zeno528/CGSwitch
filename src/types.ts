export interface ProfileSummary {
  id: string;
  name: string;
  model: string | null;
  provider: string | null;
  reasoning_effort: string | null;
  has_key: boolean;
  icon: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProfileDetail {
  id: string;
  name: string;
  icon: string | null;
  provider: string | null;
  base_url: string | null;
  api_key: string | null;
  model_values: Record<string, string>;
  config_fragment: string;
  auth_content: string | null;
  catalog_content: string | null;
  updated_at: string;
}

export interface ProfileConnectionResult {
  ok: boolean;
  latency_ms: number | null;
  status: number | null;
  error: string | null;
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
}

export interface Settings {
  theme: "system" | "light" | "dark";
  codex_app_path: string | null;
  auto_restart: boolean;
  restart_timeout_ms: number;
  autostart_enabled: boolean;
  silent_start: boolean;
  minimize_to_tray: boolean;
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
}

export type RestartStage = "idle" | "stopping" | "waiting" | "launching" | "success" | "error";
