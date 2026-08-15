use std::{path::Path, sync::Mutex};

use tauri::{AppHandle, Emitter};

use crate::builtin;
use crate::codex::{config as codex_config, process as codex_process};
use crate::database::{profile_summary, Database, StoredProfile};
use crate::error::{app_err, AppResult};
use crate::fsutil::{atomic_write, backup_file};
use crate::models::{
    AppState, CodexAppStatus, PathInfo, ProfileDetail, ProfilePayload, ProfileSummary, Settings,
};
use crate::paths::{now_ms, AppPaths};

struct ProviderDetail {
    base_url: Option<String>,
    api_key: Option<String>,
    fragment: String,
}

fn parse_provider_detail(body: &str) -> AppResult<ProviderDetail> {
    let document = codex_config::parse_document(body)?;
    let table = document.as_table();
    let value = |key: &str| {
        table
            .get(key)
            .and_then(toml_edit::Item::as_str)
            .map(str::to_string)
    };
    let mut fragment = String::new();
    for (key, item) in table.iter() {
        fragment.push_str(&format!("{key} = {item}\n"));
    }
    Ok(ProviderDetail {
        base_url: value("base_url"),
        api_key: value("experimental_bearer_token"),
        fragment,
    })
}

fn provider_api_key(body: &str) -> Option<String> {
    let document = codex_config::parse_document(body).ok()?;
    document
        .as_table()
        .get("experimental_bearer_token")
        .and_then(toml_edit::Item::as_str)
        .map(str::to_string)
}

fn is_builtin_placeholder(payload: &ProfilePayload, key: &str) -> bool {
    payload
        .builtin
        .as_deref()
        .and_then(|kind| builtin::template(kind).ok())
        .is_some_and(|template| {
            template
                .placeholder
                .is_some_and(|placeholder| placeholder == key.as_bytes())
        })
}

fn profile_config_fragment(payload: &ProfilePayload) -> String {
    let mut fragment = String::new();
    for (key, raw) in &payload.model_values {
        fragment.push_str(&format!("{key} = {raw}\n"));
    }
    if let (Some(provider_id), Some(body)) = (&payload.provider_id, &payload.provider_body) {
        if let Ok(detail) = parse_provider_detail(body) {
            fragment.push_str(&format!("\n[model_providers.{provider_id}]\n"));
            fragment.push_str(&detail.fragment);
        }
    }
    fragment
}

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

    pub fn add_builtin_profile(
        &self,
        kind: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> AppResult<ProfileSummary> {
        let template = builtin::template(kind)?;
        let profiles = self.database.profiles()?;
        if profiles
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(template.name))
        {
            return Err(app_err!("已存在同名配置档案"));
        }
        let base_url = base_url.map(str::trim).filter(|value| !value.is_empty());
        let api_key = api_key.map(str::trim).filter(|key| !key.is_empty());
        if template.placeholder.is_some() && api_key.is_none() {
            return Err(app_err!("请先填写 API 密钥"));
        }
        // 只创建快照，不写生产环境；快照内容与最终应用时渲染的 config 一致
        let rendered = template.render_config(None)?;
        let text =
            std::str::from_utf8(&rendered).map_err(|_| app_err!("内置模板不是有效 UTF-8"))?;
        let mut payload =
            codex_config::capture_from_document(&codex_config::parse_document(text)?)?;
        payload.builtin = Some(template.kind.to_string());
        if base_url.is_some() || api_key.is_some() {
            let body = payload
                .provider_body
                .as_deref()
                .ok_or_else(|| app_err!("内置档案缺少供应商配置"))?;
            payload.provider_body =
                Some(codex_config::update_provider_body(body, base_url, api_key)?);
        }
        let timestamp = now_ms().to_string();
        let summary = self
            .database
            .insert_profile(template.name, &payload, &timestamp)?;
        self.database
            .set_profile_icon(&summary.id, Some(template.icon), &timestamp)?;
        self.database.record_event(
            Some(&summary.id),
            "add_builtin",
            "success",
            Some("added built-in profile"),
            &timestamp,
        )?;
        let stored = self.database.profile(&summary.id)?;
        Ok(profile_summary(&stored))
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

    pub fn get_profile(&self, id: &str) -> AppResult<ProfileDetail> {
        let stored = self.database.profile(id)?;
        let payload = &stored.payload;
        let provider = payload
            .provider_body
            .as_deref()
            .map(parse_provider_detail)
            .transpose()?;
        let api_key = provider
            .as_ref()
            .and_then(|detail| detail.api_key.clone())
            .filter(|key| !is_builtin_placeholder(payload, key));
        let (config_fragment, catalog_content) = match payload.builtin.as_deref() {
            Some(kind) => {
                let template = builtin::template(kind)?;
                let api_key = payload.provider_body.as_deref().and_then(provider_api_key);
                let rendered = template.render_config(api_key.as_deref())?;
                let fragment = String::from_utf8_lossy(&rendered).into_owned();
                let catalog = template
                    .catalog
                    .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned());
                (fragment, catalog)
            }
            None => (
                profile_config_fragment(payload),
                payload
                    .model_values
                    .get("model_catalog_json")
                    .map(|raw| raw.trim().trim_matches('"'))
                    .map(|path| self.paths.codex_home.join(path))
                    .and_then(|file| read_optional_text(&file)),
            ),
        };
        Ok(ProfileDetail {
            id: stored.id.clone(),
            name: stored.name.clone(),
            icon: stored.icon.clone(),
            provider: payload.provider_id.clone(),
            base_url: provider.as_ref().and_then(|detail| detail.base_url.clone()),
            api_key,
            model_values: payload.model_values.clone(),
            config_fragment,
            auth_content: read_optional_text(&self.paths.codex_home.join("auth.json")),
            catalog_content,
            updated_at: stored.updated_at.clone(),
        })
    }

    pub fn update_profile(
        &self,
        id: &str,
        name: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> AppResult<ProfileSummary> {
        let name = validated_name(name)?;
        let profiles = self.database.profiles()?;
        if profiles
            .iter()
            .any(|profile| profile.id != id && profile.name.eq_ignore_ascii_case(&name))
        {
            return Err(app_err!("已存在同名配置档案"));
        }
        let stored = self.database.profile(id)?;
        let mut payload = stored.payload;
        if payload.provider_id.is_some() {
            let body = payload
                .provider_body
                .as_deref()
                .ok_or_else(|| app_err!("该档案缺少供应商配置数据"))?;
            if base_url.is_some() || api_key.is_some() {
                payload.provider_body =
                    Some(codex_config::update_provider_body(body, base_url, api_key)?);
            }
        } else if base_url.is_some() || api_key.is_some() {
            return Err(app_err!("该档案没有供应商配置，无法修改调用地址或密钥"));
        }
        let mut write_back = false;
        if (base_url.is_some() || api_key.is_some()) && payload.provider_id.is_some() {
            let profiles = self.database.profiles()?;
            let live = std::fs::read_to_string(self.paths.codex_config()).ok();
            if let Some(document) = live
                .as_deref()
                .and_then(|text| codex_config::parse_document(text).ok())
            {
                write_back = self
                    .matching_active_profile(&profiles, &document, None)?
                    .as_deref()
                    == Some(id);
            }
        }
        let updated = self
            .database
            .update_profile(id, &name, &payload, &now_ms().to_string())?;
        if write_back {
            if payload.builtin.is_some() {
                self.apply_builtin_profile(id, &payload, "update")?;
            } else {
                self.write_live_provider_update(
                    id,
                    payload.provider_id.as_deref().expect("已检查 provider_id"),
                    base_url,
                    api_key,
                )?;
            }
        }
        Ok(profile_summary(&updated))
    }

    fn write_live_provider_update(
        &self,
        profile_id: &str,
        provider_id: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
    ) -> AppResult<()> {
        let config_path = self.paths.codex_config();
        let original = std::fs::read_to_string(&config_path)
            .map_err(|error| app_err!("无法读取 {}: {error}", config_path.display()))?;
        let mut document = codex_config::parse_document(&original)?;
        codex_config::update_provider_in_document(&mut document, provider_id, base_url, api_key)?;
        backup_file(&config_path, &self.paths.config_backup, "config")?;
        atomic_write(&config_path, document.to_string().as_bytes())?;
        self.database.record_event(
            Some(profile_id),
            "update",
            "success",
            Some("provider settings written back to live config"),
            &now_ms().to_string(),
        )?;
        Ok(())
    }

    pub fn open_codex_file(&self, relative: &str) -> AppResult<()> {
        let reference = relative.trim().trim_matches('"');
        if reference.is_empty() {
            return Err(app_err!("未指定要打开的文件"));
        }
        let raw = Path::new(reference);
        let path = if raw.is_absolute() {
            raw.to_path_buf()
        } else if let Some(rest) = reference.strip_prefix("~/") {
            // 模板里的 ~/.codex/... 是相对用户主目录的完整路径
            self.paths
                .codex_home
                .parent()
                .unwrap_or(&self.paths.codex_home)
                .join(rest)
        } else {
            self.paths.codex_home.join(raw)
        };
        let canonical = path
            .canonicalize()
            .map_err(|_| app_err!("文件不存在：{}", path.display()))?;
        if !raw.is_absolute() {
            let root = self
                .paths
                .codex_home
                .canonicalize()
                .map_err(|_| app_err!("无法定位 Codex 目录"))?;
            if !canonical.starts_with(&root) {
                return Err(app_err!("只能打开 Codex 目录内的文件"));
            }
        }
        open_in_editor(&canonical)
    }

    pub fn apply_profile(&self, id: &str) -> AppResult<()> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        let config_path = self.paths.codex_config();
        let original = std::fs::read_to_string(&config_path)
            .map_err(|error| app_err!("无法读取 {}: {error}", config_path.display()))?;
        let mut document = codex_config::parse_document(&original)?;

        // 切换前把当前 live 配置回写进正在生效的档案，使档案跟随使用中的累计更新
        self.autosync_active_profile(id, &document)?;

        let payload = self.database.profile(id)?.payload;
        if payload.builtin.is_some() {
            return self.apply_builtin_profile(id, &payload, "apply");
        }

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

    /// 内置官方档案：整文件替换为模板原文（仅替换密钥占位符），
    /// 并写入本档案自带的关联文件（deepseek/智谱各自独立的 models.json、minimax 的 custom-catalog.json），
    /// 写生产文件前都先备份旧文件。
    fn apply_builtin_profile(
        &self,
        profile_id: &str,
        payload: &ProfilePayload,
        action: &str,
    ) -> AppResult<()> {
        let kind = payload
            .builtin
            .as_deref()
            .ok_or_else(|| app_err!("档案缺少内置类型"))?;
        let template = builtin::template(kind)?;
        let api_key = payload.provider_body.as_deref().and_then(provider_api_key);
        let rendered = template.render_config(api_key.as_deref())?;

        let config_path = self.paths.codex_config();
        backup_file(&config_path, &self.paths.config_backup, "config")?;
        atomic_write(&config_path, &rendered)?;

        if let Some((target, bytes)) = template.catalog {
            let destination = self.paths.codex_home.join(target);
            let stem = Path::new(target)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("codex-file");
            backup_file(&destination, &self.paths.codex_files_backup, stem)?;
            atomic_write(&destination, bytes)?;
        }

        self.database.record_event(
            Some(profile_id),
            action,
            "success",
            Some("built-in configuration applied"),
            &now_ms().to_string(),
        )?;
        Ok(())
    }

    fn autosync_active_profile(
        &self,
        target_id: &str,
        document: &toml_edit::DocumentMut,
    ) -> AppResult<()> {
        let profiles = self.database.profiles()?;
        let Some(active_id) = self.matching_active_profile(&profiles, document, Some(target_id))?
        else {
            return Ok(());
        };
        let Some(profile) = profiles.iter().find(|profile| profile.id == active_id) else {
            return Ok(());
        };
        // 内置档案是固定官方模板，应用时整文件替换，不参与累计更新回写
        if profile.payload.builtin.is_some() {
            return Ok(());
        }
        let Ok(live) = codex_config::capture_from_document(document) else {
            return Ok(());
        };
        if live == profile.payload {
            return Ok(());
        }
        if let Err(error) =
            self.database
                .update_profile(&active_id, &profile.name, &live, &now_ms().to_string())
        {
            let _ = self.database.record_event(
                Some(&active_id),
                "autosync",
                "failed",
                Some(&error.0),
                &now_ms().to_string(),
            );
        }
        Ok(())
    }

    /// 识别当前 live 配置对应的激活档案：严格匹配优先；
    /// 配置累计新键导致严格匹配失效时，用"档案是 live 子集"的宽松匹配，且仅当唯一候选。
    fn matching_active_profile(
        &self,
        profiles: &[StoredProfile],
        document: &toml_edit::DocumentMut,
        exclude: Option<&str>,
    ) -> AppResult<Option<String>> {
        if let Some(id) = self.active_profile_id(profiles)? {
            if exclude.is_none_or(|excluded| excluded != id) {
                return Ok(Some(id));
            }
        }
        let candidates: Vec<String> = profiles
            .iter()
            .filter(|profile| exclude.is_none_or(|excluded| excluded != profile.id))
            .filter(|profile| {
                codex_config::subset_match(document, &profile.payload).unwrap_or(false)
            })
            .map(|profile| profile.id.clone())
            .collect();
        Ok((candidates.len() == 1).then(|| candidates[0].clone()))
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

fn open_in_editor(path: &Path) -> AppResult<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| app_err!("无法打开文件：{error}"))
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| app_err!("无法打开文件：{error}"))
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| app_err!("无法打开文件：{error}"))
    }
}

fn read_optional_text(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .filter(|text| text.len() <= 512 * 1024)
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
    fn apply_profile_autosyncs_accumulated_active_profile() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        let context = AppContext::new(paths).unwrap();
        let write = |text: &str| std::fs::write(context.paths.codex_config(), text).unwrap();

        write(
            r#"
model = "glm-5.3"
model_provider = "ZAI"
model_reasoning_effort = "high"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
experimental_bearer_token = "secret"
"#,
        );
        let profile_a = context.capture_profile("A").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));

        write(
            r#"
model = "other-model"
model_provider = "ZAI"
model_reasoning_effort = "low"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://old.example"
experimental_bearer_token = "old"
"#,
        );
        let profile_b = context.capture_profile("B").unwrap();

        // B 使用期间 live 配置累计了新的模型键和 provider 字段
        write(
            r#"
model = "other-model"
model_provider = "ZAI"
model_reasoning_effort = "low"
model_catalog_json = "zai.json"

[mcp_servers.keep]
command = "node"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://old.example"
experimental_bearer_token = "old"
new_field = "accumulated"
"#,
        );

        context.apply_profile(&profile_a.id).unwrap();

        let stored_b = context.database.profile(&profile_b.id).unwrap();
        assert_eq!(
            stored_b
                .payload
                .model_values
                .get("model_catalog_json")
                .map(|raw| raw.trim().trim_matches('"')),
            Some("zai.json")
        );
        assert!(stored_b
            .payload
            .provider_body
            .as_deref()
            .unwrap()
            .contains("new_field = \"accumulated\""));
        assert_eq!(
            context.get_state().unwrap().active_profile_id.as_deref(),
            Some(profile_a.id.as_str())
        );
    }

    #[test]
    fn get_profile_returns_raw_file_contents() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            r#"
model = "glm-5.3"
model_provider = "ZAI"
model_catalog_json = "zai.json"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
experimental_bearer_token = "secret-token"
"#,
        )
        .unwrap();
        std::fs::write(
            paths.codex_home.join("zai.json"),
            r#"{"models":[{"id":"glm-5.3","api_key":"sk-secret"}]}"#,
        )
        .unwrap();
        std::fs::write(
            paths.codex_home.join("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"raw-token"}}"#,
        )
        .unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("ZAI").unwrap();
        let detail = context.get_profile(&profile.id).unwrap();

        assert!(detail.config_fragment.contains("experimental_bearer_token"));
        assert!(detail.config_fragment.contains("secret-token"));
        assert!(!detail.config_fragment.contains("••••••••"));
        assert_eq!(detail.api_key.as_deref(), Some("secret-token"));
        assert_eq!(
            detail.catalog_content.as_deref(),
            Some(r#"{"models":[{"id":"glm-5.3","api_key":"sk-secret"}]}"#)
        );
        assert_eq!(
            detail.auth_content.as_deref(),
            Some(r#"{"auth_mode":"chatgpt","tokens":{"access_token":"raw-token"}}"#)
        );
    }

    #[test]
    fn update_profile_writes_back_to_active_live_config() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            r#"
model = "glm-5.3"
model_provider = "ZAI"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://old.example"
experimental_bearer_token = "old-key"
"#,
        )
        .unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("ZAI").unwrap();
        context
            .update_profile(
                &profile.id,
                "ZAI",
                Some("https://new.example"),
                Some("new-key"),
            )
            .unwrap();

        let text = std::fs::read_to_string(context.paths.codex_config()).unwrap();
        assert!(text.contains(r#"base_url = "https://new.example""#));
        assert!(text.contains(r#"experimental_bearer_token = "new-key""#));
        assert!(!text.contains("old-key"));

        let detail = context.get_profile(&profile.id).unwrap();
        assert_eq!(detail.base_url.as_deref(), Some("https://new.example"));
        assert_eq!(detail.api_key.as_deref(), Some("new-key"));
    }

    #[test]
    fn only_exposed_paths_can_be_opened() {
        let home = tempfile::tempdir().unwrap();
        let context = AppContext::new(crate::paths::from_home(home.path()).unwrap()).unwrap();

        assert!(context.is_managed_path(&context.paths.database.display().to_string()));
        assert!(!context.is_managed_path("C:\\unmanaged-path"));
    }

    #[test]
    fn update_profile_rejects_duplicate_name() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        context.capture_profile("First").unwrap();
        // 档案 id 取毫秒时间戳，同毫秒内二次捕获会撞 id；真实 UI 不可能，测试里隔开
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = context.capture_profile("Second").unwrap();

        let error = context
            .update_profile(&second.id, "first", None, None)
            .unwrap_err();
        assert!(error.0.contains("已存在同名配置档案"));
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

    #[test]
    fn add_builtin_profile_creates_snapshot_only() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        let original = "model = \"glm-5.3\"\n";
        std::fs::write(paths.codex_config(), original).unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("deepseek", Some("https://custom.example"), Some("sk-test"))
            .unwrap();

        assert_eq!(profile.name, "DeepSeek 官方");
        assert_eq!(profile.model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(profile.provider.as_deref(), Some("deepseek"));
        assert_eq!(profile.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(profile.icon.as_deref(), Some("deepseek"));

        let stored = context.database.profile(&profile.id).unwrap();
        assert_eq!(stored.payload.builtin.as_deref(), Some("deepseek"));
        assert_eq!(
            stored
                .payload
                .model_values
                .get("model_catalog_json")
                .map(|raw| raw.trim().trim_matches('"')),
            Some("~/.codex/models.json")
        );
        assert!(stored
            .payload
            .provider_body
            .as_deref()
            .unwrap()
            .contains("sk-test"));
        assert!(stored
            .payload
            .provider_body
            .as_deref()
            .unwrap()
            .contains("https://custom.example"));

        // 添加只存快照，不写生产配置
        assert_eq!(
            std::fs::read_to_string(context.paths.codex_config()).unwrap(),
            original
        );

        let error = context
            .add_builtin_profile("deepseek", None, None)
            .unwrap_err();
        assert!(error.0.contains("已存在同名配置档案"));
    }

    #[test]
    fn add_builtin_profile_requires_api_key() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        let error = context
            .add_builtin_profile("deepseek", None, None)
            .unwrap_err();
        assert!(error.0.contains("请先填写 API 密钥"));
        assert!(context.database.profiles().unwrap().is_empty());
    }

    #[test]
    fn apply_builtin_profile_writes_exact_config_and_catalog() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            "model = \"other\"\n[mcp_servers.keep]\ncommand = \"node\"\n",
        )
        .unwrap();
        let old_models = b"{\"models\":[]}";
        std::fs::write(paths.codex_home.join("models.json"), old_models).unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("deepseek", None, Some("sk-test"))
            .unwrap();
        context.apply_profile(&profile.id).unwrap();

        // 整文件替换，模板之外的键全部清掉，仅密钥占位符被替换
        let config = std::fs::read(context.paths.codex_config()).unwrap();
        let expected = crate::builtin::template("deepseek")
            .unwrap()
            .render_config(Some("sk-test"))
            .unwrap();
        assert_eq!(config, expected);
        assert!(!String::from_utf8_lossy(&config).contains("<你的 DeepSeek API Key>"));
        // 关联文件按本档案字节写入，旧文件已备份
        let models = std::fs::read(context.paths.codex_home.join("models.json")).unwrap();
        assert_eq!(models, crate::builtin::DEEPSEEK_MODELS);
        let backup = std::fs::read_dir(context.paths.codex_files_backup.clone())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(backup).unwrap(), old_models);
        assert!(context.paths.config_backup.read_dir().unwrap().count() > 0);
    }

    #[test]
    fn update_builtin_profile_writes_key_back_when_active() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"other\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("deepseek", None, Some("sk-old"))
            .unwrap();
        context.apply_profile(&profile.id).unwrap();
        assert_eq!(
            context.get_state().unwrap().active_profile_id.as_deref(),
            Some(profile.id.as_str())
        );

        context
            .update_profile(
                &profile.id,
                "DeepSeek 官方",
                Some("https://api.deepseek.com/"),
                Some("sk-real"),
            )
            .unwrap();

        let config = std::fs::read(context.paths.codex_config()).unwrap();
        let expected = crate::builtin::template("deepseek")
            .unwrap()
            .render_config(Some("sk-real"))
            .unwrap();
        assert_eq!(config, expected);
        assert!(!String::from_utf8_lossy(&config).contains("<你的 DeepSeek API Key>"));

        let detail = context.get_profile(&profile.id).unwrap();
        assert_eq!(detail.api_key.as_deref(), Some("sk-real"));
        assert!(detail.config_fragment.contains("sk-real"));
    }

    #[test]
    fn builtin_catalogs_are_not_mixed() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"other\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        let deepseek = context
            .add_builtin_profile("deepseek", None, Some("sk-d"))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let zhipu = context
            .add_builtin_profile("zhipu", None, Some("sk-z"))
            .unwrap();

        context.apply_profile(&deepseek.id).unwrap();
        assert_eq!(
            std::fs::read(context.paths.codex_home.join("models.json")).unwrap(),
            crate::builtin::DEEPSEEK_MODELS
        );

        context.apply_profile(&zhipu.id).unwrap();
        assert_eq!(
            std::fs::read(context.paths.codex_home.join("models.json")).unwrap(),
            crate::builtin::ZHIPU_MODELS
        );
    }

    #[test]
    fn apply_minimax_inserts_catalog_line_and_writes_catalog() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"other\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("minimax", None, Some("mm-key"))
            .unwrap();
        context.apply_profile(&profile.id).unwrap();

        let config = std::fs::read(context.paths.codex_config()).unwrap();
        let expected = crate::builtin::template("minimax")
            .unwrap()
            .render_config(Some("mm-key"))
            .unwrap();
        assert_eq!(config, expected);
        assert!(String::from_utf8_lossy(&config)
            .contains("model_catalog_json = \"~/.codex/model-catalogs/custom-catalog.json\""));
        assert!(!String::from_utf8_lossy(&config).contains("<MINIMAX_API_KEY>"));

        let catalog = std::fs::read(
            context
                .paths
                .codex_home
                .join("model-catalogs")
                .join("custom-catalog.json"),
        )
        .unwrap();
        assert_eq!(catalog, crate::builtin::MINIMAX_CATALOG);
    }

    #[test]
    fn apply_chatgpt_writes_official_default_and_keeps_auth() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            "model_provider = \"ZAI\"\nmodel = \"glm-5.3\"\n\n[model_providers.ZAI]\nname = \"ZAI\"\nbase_url = \"https://open.bigmodel.cn/api/v1\"\nexperimental_bearer_token = \"old-key\"\n",
        )
        .unwrap();
        std::fs::write(paths.codex_home.join("auth.json"), b"{\"login\":\"kept\"}").unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context.add_builtin_profile("chatgpt", None, None).unwrap();
        context.apply_profile(&profile.id).unwrap();

        assert_eq!(
            std::fs::read(context.paths.codex_config()).unwrap(),
            crate::builtin::CHATGPT_CONFIG
        );
        assert_eq!(
            std::fs::read(context.paths.codex_home.join("auth.json")).unwrap(),
            b"{\"login\":\"kept\"}"
        );
        assert!(!context.paths.codex_home.join("models.json").exists());
    }

    #[test]
    fn builtin_placeholder_key_is_not_exposed_as_api_key() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        // 兼容仍带占位符密钥的旧数据：get_profile 不应把占位符当成密钥回显
        let payload = ProfilePayload {
            builtin: Some("deepseek".into()),
            model_values: [
                ("model".to_string(), "\"deepseek-v4-flash\"".into()),
                ("model_reasoning_effort".to_string(), "\"high\"".into()),
                ("model_catalog_json".to_string(), "\"~/.codex/models.json\"".into()),
            ]
            .into_iter()
            .collect(),
            provider_id: Some("deepseek".into()),
            provider_body: Some(
                "name = \"deepseek\"\nbase_url = \"https://api.deepseek.com/\"\nwire_api = \"responses\"\nexperimental_bearer_token = \"<你的 DeepSeek API Key>\""
                    .into(),
            ),
        };
        let summary = context
            .database
            .insert_profile("DeepSeek 旧数据", &payload, &now_ms().to_string())
            .unwrap();

        let detail = context.get_profile(&summary.id).unwrap();
        assert_eq!(detail.api_key, None);
        assert!(detail.config_fragment.contains("<你的 DeepSeek API Key>"));
    }
}
