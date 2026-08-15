export interface ProfileSummary {
  id: string;
  name: string;
  model: string | null;
  provider: string | null;
  reasoning_effort: string | null;
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
  updated_at: string;
}

export interface Settings {
  theme: "system" | "light" | "dark";
  codex_app_path: string | null;
  auto_restart: boolean;
  restart_timeout_ms: number;
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
