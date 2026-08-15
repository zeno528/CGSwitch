use std::{path::Path, sync::Mutex};

use tauri::{AppHandle, Emitter};

use crate::codex::{config as codex_config, process as codex_process};
use crate::database::{profile_summary, Database, StoredProfile};
use crate::error::{app_err, AppResult};
use crate::fsutil::{atomic_write, backup_file};
use crate::models::{AppState, CodexAppStatus, PathInfo, ProfileSummary, Settings};
use crate::paths::{now_ms, AppPaths};

#[derive(Debug)]
pub struct AppContext {
    database: Database,
    paths: AppPaths,
    operation: Mutex<()>,
}

impl AppContext {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        Ok(Self {
            database: Database::open(&paths)?,
            paths,
            operation: Mutex::new(()),
        })
    }

    pub fn get_state(&self) -> AppResult<AppState> {
        let settings = self.database.settings()?;
        let profiles = self.database.profiles()?;
        let active_profile_id = self.active_profile_id(&profiles)?;
        let process_ids = codex_process::find_process_ids(settings.codex_app_path.as_deref());
        let (display_path, source) =
            codex_process::codex_display_path(settings.codex_app_path.as_deref());

        Ok(AppState {
            profiles: profiles
                .iter()
                .map(profile_summary)
                .collect::<Vec<ProfileSummary>>(),
            active_profile_id,
            codex: CodexAppStatus {
                running: !process_ids.is_empty(),
                display_path,
                source,
            },
            settings,
            paths: self.path_info(),
        })
    }

    pub fn capture_profile(&self, name: &str) -> AppResult<ProfileSummary> {
        let name = validated_name(name)?;
        let profiles = self.database.profiles()?;
        if profiles
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(&name))
        {
            return Err(app_err!("已存在同名配置档案"));
        }
        let payload = codex_config::read_profile(&self.paths.codex_config())?;
        let timestamp = now_ms().to_string();
        let summary = self.database.insert_profile(&name, &payload, &timestamp)?;
        self.database.record_event(
            Some(&summary.id),
            "capture",
            "success",
            Some("captured live configuration"),
            &timestamp,
        )?;
        Ok(summary)
    }

    pub fn rename_profile(&self, id: &str, name: &str) -> AppResult<()> {
        let name = validated_name(name)?;
        let profiles = self.database.profiles()?;
        if profiles
            .iter()
            .any(|profile| profile.id != id && profile.name.eq_ignore_ascii_case(&name))
        {
            return Err(app_err!("已存在同名配置档案"));
        }
        self.database
            .rename_profile(id, &name, &now_ms().to_string())
    }

    pub fn delete_profile(&self, id: &str) -> AppResult<()> {
        self.database.delete_profile(id)
    }

    pub fn set_profile_icon(&self, id: &str, icon: Option<&str>) -> AppResult<()> {
        let icon = validated_icon(icon)?;
        self.database
            .set_profile_icon(id, icon.as_deref(), &now_ms().to_string())
    }

    pub fn apply_profile(&self, id: &str) -> AppResult<()> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        let payload = self.database.profile_payload(id)?;
        let config_path = self.paths.codex_config();
        let original = std::fs::read_to_string(&config_path)
            .map_err(|error| app_err!("无法读取 {}: {error}", config_path.display()))?;
        let mut document = codex_config::parse_document(&original)?;
        codex_config::apply_to_document(&mut document, &payload)?;
        let updated = document.to_string();

        backup_file(&config_path, &self.paths.config_backup, "config")?;
        atomic_write(&config_path, updated.as_bytes())?;
        self.database.record_event(
            Some(id),
            "apply",
            "success",
            Some("configuration applied"),
            &now_ms().to_string(),
        )?;
        Ok(())
    }

    pub fn restart_codex(&self, app: &AppHandle) -> AppResult<()> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        let settings = self.database.settings()?;
        emit(app, "stopping", None);

        let process_ids = codex_process::find_process_ids(settings.codex_app_path.as_deref());
        if !process_ids.is_empty() {
            codex_process::terminate_process_ids(&process_ids);
            emit(app, "waiting", None);
            let exited =
                codex_process::wait_for_exit(&process_ids, settings.restart_timeout_ms, 100);
            if !exited {
                let message = "Codex 未在超时时间内退出，已取消重新启动";
                self.database.record_event(
                    None,
                    "restart",
                    "timeout",
                    Some(message),
                    &now_ms().to_string(),
                )?;
                emit(app, "error", Some(message));
                return Err(app_err!("{message}"));
            }
        }

        emit(app, "launching", None);
        let result = codex_process::launch_codex(settings.codex_app_path.as_deref());
        let status = if result.is_ok() { "success" } else { "failed" };
        let message = result.as_ref().err().map(|error| error.0.clone());
        self.database.record_event(
            None,
            "restart",
            status,
            message.as_deref(),
            &now_ms().to_string(),
        )?;
        match result {
            Ok(()) => {
                emit(app, "success", None);
                Ok(())
            }
            Err(error) => {
                emit(app, "error", Some(&error.0));
                Err(error)
            }
        }
    }

    pub fn settings(&self) -> AppResult<Settings> {
        self.database.settings()
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<Settings> {
        let mut settings = settings.clone();
        settings.theme = settings.theme.trim().to_lowercase();
        if !["system", "light", "dark"].contains(&settings.theme.as_str()) {
            return Err(app_err!("不支持的主题设置"));
        }
        if !(1_000..=60_000).contains(&settings.restart_timeout_ms) {
            return Err(app_err!("重启等待时间必须在 1000 到 60000 毫秒之间"));
        }
        settings.codex_app_path = settings
            .codex_app_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.database.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn open_path(&self, path: &str) -> AppResult<()> {
        if !self.is_managed_path(path) {
            return Err(app_err!("不能打开未列出的本机路径"));
        }
        open_in_file_explorer(Path::new(path))
    }

    fn active_profile_id(&self, profiles: &[StoredProfile]) -> AppResult<Option<String>> {
        let config_path = self.paths.codex_config();
        let Ok(text) = std::fs::read_to_string(config_path) else {
            return Ok(None);
        };
        let document = match codex_config::parse_document(&text) {
            Ok(document) => document,
            Err(_) => return Ok(None),
        };
        for profile in profiles {
            if codex_config::matches_profile(&document, &profile.payload)? {
                return Ok(Some(profile.id.clone()));
            }
        }
        Ok(None)
    }

    fn path_info(&self) -> Vec<PathInfo> {
        vec![
            PathInfo {
                label: "数据库".into(),
                path: self.paths.database.display().to_string(),
            },
            PathInfo {
                label: "Codex 配置".into(),
                path: self.paths.codex_config().display().to_string(),
            },
            PathInfo {
                label: "配置备份".into(),
                path: self.paths.config_backup.display().to_string(),
            },
            PathInfo {
                label: "数据库备份".into(),
                path: self.paths.database_backup.display().to_string(),
            },
            PathInfo {
                label: "日志".into(),
                path: self.paths.logs.display().to_string(),
            },
        ]
    }

    fn is_managed_path(&self, path: &str) -> bool {
        self.path_info().iter().any(|item| item.path == path)
    }
}

fn validated_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 50 {
        return Err(app_err!("配置档案名称长度必须在 1 到 50 个字符之间"));
    }
    Ok(name.to_string())
}

/// 图标 id 对应 src/assets/providers/<id>.svg，仅校验格式；
/// 合法 id 列表由前端注册表维护，未匹配的 id 前端按无图标渲染。
fn validated_icon(icon: Option<&str>) -> AppResult<Option<String>> {
    icon.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.len() > 40
                || !value
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
            {
                return Err(app_err!("无效的图标标识"));
            }
            Ok(value.to_string())
        })
        .transpose()
}

fn emit(app: &AppHandle, stage: &str, message: Option<&str>) {
    let _ = app.emit(
        "restart-progress",
        serde_json::json!({ "stage": stage, "message": message }),
    );
}

fn open_in_file_explorer(path: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        use windows::{
            core::HSTRING,
            Win32::{
                System::Com::CoInitialize,
                UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems},
            },
        };

        let _ = unsafe { CoInitialize(None) };
        let folder = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path
        };
        let folder_text = HSTRING::from(folder);
        let folder_id = unsafe { ILCreateFromPathW(&folder_text) };
        if folder_id.is_null() {
            return Err(app_err!("无法定位资源管理器路径：{}", folder.display()));
        }
        let item_text = HSTRING::from(path);
        let item_id = path
            .is_file()
            .then(|| unsafe { ILCreateFromPathW(&item_text) });
        let selection = item_id
            .filter(|item| !item.is_null())
            .map(|item| [item.cast_const()]);
        let result = unsafe {
            SHOpenFolderAndSelectItems(
                folder_id.cast_const(),
                selection.as_ref().map(|items| &items[..]),
                0,
            )
        };
        unsafe {
            ILFree(Some(folder_id.cast_const()));
            if let Some(item) = item_id.filter(|item| !item.is_null()) {
                ILFree(Some(item.cast_const()));
            }
        }
        result.map_err(|error| app_err!("无法打开资源管理器：{error}"))
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| app_err!("无法打开文件管理器：{error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_and_apply_profile_round_trip() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            r#"
model = "glm-5.3"
model_provider = "ZAI"
model_reasoning_effort = "high"

[mcp_servers.keep]
command = "node"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
experimental_bearer_token = "secret"
"#,
        )
        .unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("GLM High").unwrap();
        std::fs::write(
            context.paths.codex_config(),
            r#"
model = "other-model"
model_provider = "ZAI"
model_reasoning_effort = "low"

[mcp_servers.keep]
command = "node"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://old.example"
experimental_bearer_token = "old"
"#,
        )
        .unwrap();

        context.apply_profile(&profile.id).unwrap();
        let state = context.get_state().unwrap();
        let text = std::fs::read_to_string(context.paths.codex_config()).unwrap();

        assert_eq!(
            state.active_profile_id.as_deref(),
            Some(profile.id.as_str())
        );
        assert!(text.contains("glm-5.3"));
        assert!(text.contains("https://api.example"));
        assert!(text.contains("[mcp_servers.keep]"));
        assert!(context.paths.config_backup.read_dir().unwrap().count() > 0);
    }

    #[test]
    fn only_exposed_paths_can_be_opened() {
        let home = tempfile::tempdir().unwrap();
        let context = AppContext::new(crate::paths::from_home(home.path()).unwrap()).unwrap();

        assert!(context.is_managed_path(&context.paths.database.display().to_string()));
        assert!(!context.is_managed_path("C:\\unmanaged-path"));
    }

    #[test]
    fn icon_ids_are_validated() {
        assert_eq!(validated_icon(None).unwrap(), None);
        assert_eq!(validated_icon(Some("  ")).unwrap(), None);
        assert_eq!(
            validated_icon(Some(" zhipu ")).unwrap().as_deref(),
            Some("zhipu")
        );
        assert!(validated_icon(Some("Zhipu")).is_err());
        assert!(validated_icon(Some("a!b")).is_err());
        assert!(validated_icon(Some(&"x".repeat(41))).is_err());
    }
}
