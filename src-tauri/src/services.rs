use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter};

use crate::auth::codex_oauth::CodexOAuthState;
use crate::builtin;
use crate::codex::{config as codex_config, process as codex_process};
use crate::database::{profile_summary, Database};
use crate::error::{app_err, AppResult};
use crate::fsutil::{atomic_write, backup_file, prune_backups};
use crate::models::{
    AppState, CodexAppStatus, DeepSeekBalanceInfo, PathInfo, ProfileDetail, ProfileKind,
    ProfilePayload, ProfileSummary, Settings,
};
use crate::paths::{now_ms, AppPaths};

struct ProviderDetail {
    base_url: Option<String>,
    api_key: Option<String>,
    fragment: String,
}

/// 供应商连通性测试结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileConnectionResult {
    pub ok: bool,
    pub latency_ms: Option<u128>,
    pub status: Option<u16>,
    pub error: Option<String>,
}

/// DeepSeek 余额查询结果（接口文档：https://api-docs.deepseek.com/zh-cn/api/get-user-balance）
#[derive(Debug, Clone, serde::Serialize)]
pub struct DeepSeekBalance {
    pub is_available: bool,
    pub balance_infos: Vec<DeepSeekBalanceInfo>,
    pub latency_ms: Option<u128>,
}

#[derive(Debug, serde::Deserialize)]
struct DeepSeekBalanceResponse {
    is_available: bool,
    balance_infos: Vec<DeepSeekBalanceInfo>,
}

/// 数据库备份文件信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct DatabaseBackupInfo {
    pub name: String,
    pub size_bytes: u64,
    pub created_at: i64,
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

/// 供应商已保存的真实 API 密钥（占位符视为未配置）。
fn stored_provider_api_key(payload: &ProfilePayload) -> Option<String> {
    payload
        .provider_body
        .as_deref()
        .and_then(provider_api_key)
        .filter(|key| !key.trim().is_empty() && !is_builtin_placeholder(payload, key))
}

/// 统一的 HTTP 客户端：8 秒超时。
fn http_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|error| app_err!("创建 HTTP 客户端失败: {error}"))
}

/// reqwest 错误转可读提示。
fn reqwest_error_message(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "请求超时".to_string()
    } else if error.is_connect() {
        "连接失败".to_string()
    } else {
        error.to_string()
    }
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
            fragment.push_str(&format!("[model_providers.{provider_id}]\n"));
            fragment.push_str(&detail.fragment);
        }
    }
    fragment
}

#[derive(Debug)]
pub struct AppContext {
    database: Arc<Database>,
    paths: AppPaths,
    operation: Mutex<()>,
}

impl AppContext {
    pub fn new(paths: AppPaths) -> AppResult<Self> {
        let database = Arc::new(Database::open(&paths)?);
        Ok(Self::new_with_database(paths, database))
    }

    pub fn new_with_database(paths: AppPaths, database: Arc<Database>) -> Self {
        Self {
            database,
            paths,
            operation: Mutex::new(()),
        }
    }

    pub fn get_state(&self) -> AppResult<AppState> {
        // 刷新/窗口激活等显式时机：外部改过 live 就把激活供应商快照同步回数据库（有差异才写）
        let live = self.live_document();
        if let Some(document) = live.as_ref() {
            let _ = self.sync_active_profile_document(document);
        }
        let settings = self.settings()?;
        let profiles = self.database.profiles()?;
        // 激活状态只来自手动应用/捕获（显式状态或应用事件），不做 live 配置推断，
        // 避免“添加供应商”被误判成“正在使用”。
        let active_profile_id = match self.active_profile_state()? {
            Some(id) if profiles.iter().any(|profile| profile.id == id) => Some(id),
            _ => match self.database.latest_applied_profile()? {
                Some(id) if profiles.iter().any(|profile| profile.id == id) => Some(id),
                _ => None,
            },
        };
        let live_payload = live
            .as_ref()
            .and_then(|document| codex_config::capture_from_document(document).ok());
        let process_ids = codex_process::find_process_ids(settings.codex_app_path.as_deref());
        let (display_path, source) =
            codex_process::codex_display_path(settings.codex_app_path.as_deref());

        Ok(AppState {
            profiles: profiles
                .iter()
                .map(|profile| {
                    let mut stored = profile.clone();
                    // 激活中的供应商：标签读取当前配置文件状态；其余供应商读取数据库最新字段
                    if Some(&stored.id) == active_profile_id.as_ref() {
                        if let Some(live) = &live_payload {
                            let mut live = live.clone();
                            // 供应商元数据（管理后台网址/余额开关）不在 live 配置里，覆盖时保留
                            live.admin_url = stored.payload.admin_url.clone();
                            live.show_balance = stored.payload.show_balance;
                            stored.payload = live;
                        }
                    }
                    profile_summary(&stored)
                })
                .collect::<Vec<ProfileSummary>>(),
            active_profile_id,
            codex: CodexAppStatus {
                running: !process_ids.is_empty(),
                display_path,
                source,
            },
            settings,
            paths: self.path_info(),
            balance_cache: self.load_balance_cache(),
        })
    }

    /// 轻量 Codex 运行状态查询（仅扫描进程，供前端轮询使用）。
    pub fn codex_status(&self) -> AppResult<CodexAppStatus> {
        let settings = self.settings()?;
        let process_ids = codex_process::find_process_ids(settings.codex_app_path.as_deref());
        let (display_path, source) =
            codex_process::codex_display_path(settings.codex_app_path.as_deref());
        Ok(CodexAppStatus {
            running: !process_ids.is_empty(),
            display_path,
            source,
        })
    }

    fn live_document(&self) -> Option<toml_edit::DocumentMut> {
        let text = std::fs::read_to_string(self.paths.codex_config()).ok()?;
        codex_config::parse_document(&text).ok()
    }

    pub fn capture_profile(&self, name: &str) -> AppResult<ProfileSummary> {
        let name = validated_name(name)?;
        let mut payload = codex_config::read_profile(&self.paths.codex_config())?;
        // 保存完整配置原文，编辑页按完整文件展示/编辑
        payload.raw_config = std::fs::read_to_string(self.paths.codex_config())
            .ok()
            .map(|text| text.trim_end().to_string());
        let timestamp = now_ms().to_string();
        let summary = self.database.insert_profile(&name, &payload, &timestamp)?;
        // 捕获即建立“当前 live = 该供应商”的显式关联：先把旧激活供应商的使用中累计改动
        // 同步回其快照，再把捕获结果设为使用中（捕获到的是什么就用什么，不比对内容）
        if let Some(document) = self.live_document() {
            self.autosync_active_profile(&summary.id, &document)?;
        }
        self.database.set_active_profile(Some(&summary.id))?;
        self.database.record_event(
            Some(&summary.id),
            "capture",
            "success",
            Some("captured live configuration and set active"),
            &timestamp,
        )?;
        Ok(summary)
    }

    pub fn add_builtin_profile(
        &self,
        kind: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
        admin_url: Option<&str>,
        account_id: Option<&str>,
    ) -> AppResult<ProfileSummary> {
        let template = builtin::template(kind)?;
        let base_url = base_url.map(str::trim).filter(|value| !value.is_empty());
        let api_key = api_key.map(str::trim).filter(|key| !key.is_empty());
        // 只创建快照，不写生产环境；快照内容与最终应用时渲染的 config 一致
        let rendered = template.render_config(None)?;
        let text =
            std::str::from_utf8(&rendered).map_err(|_| app_err!("内置模板不是有效 UTF-8"))?;
        let mut payload =
            codex_config::capture_from_document(&codex_config::parse_document(text)?)?;
        payload.builtin = Some(template.kind.to_string());
        if let Some(admin_url) = admin_url.map(str::trim).filter(|value| !value.is_empty()) {
            payload.admin_url = Some(admin_url.to_string());
        }
        if base_url.is_some() || api_key.is_some() {
            let body = payload
                .provider_body
                .as_deref()
                .ok_or_else(|| app_err!("内置供应商缺少配置"))?;
            payload.provider_body =
                Some(codex_config::update_provider_body(body, base_url, api_key)?);
        }
        let timestamp = now_ms().to_string();
        let summary = self
            .database
            .insert_profile(template.name, &payload, &timestamp)?;
        self.database
            .set_profile_icon(&summary.id, Some(template.icon), &timestamp)?;
        // 官方订阅档案创建时可直接绑定账号；第三方忽略绑定参数
        if payload.provider_id.is_none() {
            if let Some(account_id) = account_id {
                self.set_profile_account(&summary.id, Some(account_id))?;
            }
        }
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

    /// 自定义供应商：用户填写的三件套入库，config 必填，模型目录/认证文件有内容才存。
    pub fn add_custom_profile(
        &self,
        name: &str,
        config_text: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
        admin_url: Option<&str>,
        catalog_text: Option<&str>,
        auth_text: Option<&str>,
    ) -> AppResult<ProfileSummary> {
        let name = validated_name(name)?;
        if config_text.trim().is_empty() {
            return Err(app_err!("请填写 config.toml 内容"));
        }
        let document = codex_config::parse_document(config_text)?;
        let mut payload = codex_config::capture_from_document(&document)?;
        let base_url = base_url.map(str::trim).filter(|value| !value.is_empty());
        let api_key = api_key.map(str::trim).filter(|key| !key.is_empty());
        if let Some(admin_url) = admin_url.map(str::trim).filter(|value| !value.is_empty()) {
            payload.admin_url = Some(admin_url.to_string());
        }
        if base_url.is_some() || api_key.is_some() {
            let body = payload.provider_body.as_deref().ok_or_else(|| {
                app_err!("配置中缺少 model_providers 段落，无法写入调用地址/密钥")
            })?;
            payload.provider_body =
                Some(codex_config::update_provider_body(body, base_url, api_key)?);
        }
        payload.raw_config = Some(config_text.trim_end().to_string());
        if let Some(text) = catalog_text {
            let text = text.trim();
            if !text.is_empty() {
                serde_json::from_str::<serde_json::Value>(text)
                    .map_err(|error| app_err!("models.json 不是有效 JSON: {error}"))?;
                payload.raw_catalog = Some(text.to_string());
            }
        }
        if let Some(text) = auth_text {
            let text = text.trim();
            if !text.is_empty() {
                serde_json::from_str::<serde_json::Value>(text)
                    .map_err(|error| app_err!("auth.json 不是有效 JSON: {error}"))?;
                payload.raw_auth = Some(text.to_string());
            }
        }
        let timestamp = now_ms().to_string();
        let summary = self.database.insert_profile(&name, &payload, &timestamp)?;
        self.database.record_event(
            Some(&summary.id),
            "add_custom",
            "success",
            Some("added custom profile"),
            &timestamp,
        )?;
        let stored = self.database.profile(&summary.id)?;
        Ok(profile_summary(&stored))
    }

    /// 返回内置模板自带的关联文件原文（deepseek/智谱 的 models.json、minimax 的 custom-catalog.json），
    /// 供创建页在保存前预览；ChatGPT 无关联文件返回 None。
    pub fn get_builtin_catalog(&self, kind: &str) -> AppResult<Option<String>> {
        let template = builtin::template(kind)?;
        Ok(template
            .catalog
            .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned()))
    }

    /// 验证供应商密钥连通性：请求 OpenAI 兼容的 GET {base}/models（带 Bearer 密钥），
    /// 2xx 视为可用，401/403 视为密钥无效，返回延迟 / HTTP 状态 / 错误信息。
    /// 表单传入的地址/密钥实时生效（传了就用传的，空的直接报错）；
    /// 不传才回退已保存值（卡片上的测试按钮走这条）。
    pub async fn test_profile_connection(
        &self,
        id: &str,
        base_url_override: Option<&str>,
        api_key_override: Option<&str>,
    ) -> AppResult<ProfileConnectionResult> {
        let stored = self.database.profile(id)?;
        let payload = &stored.payload;
        if payload.provider_id.is_none() {
            return Err(app_err!("该供应商缺少配置，无法测试连通性"));
        }
        let body = payload
            .provider_body
            .as_deref()
            .ok_or_else(|| app_err!("该供应商缺少配置数据"))?;
        let detail = parse_provider_detail(body)?;
        let base_url = match base_url_override {
            Some(value) => {
                let value = value.trim();
                if value.is_empty() {
                    return Err(app_err!("请填写调用地址"));
                }
                value.to_string()
            }
            None => detail
                .base_url
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| app_err!("该供应商没有配置调用地址"))?,
        };
        let api_key = match api_key_override {
            Some(value) => {
                let value = value.trim();
                if value.is_empty() {
                    return Err(app_err!("请填写 API 密钥"));
                }
                value.to_string()
            }
            None => stored_provider_api_key(payload)
                .ok_or_else(|| app_err!("该供应商没有配置 API 密钥，请先填写后再测试"))?,
        };

        let models_url = format!("{}/models", base_url.trim_end_matches('/'));
        let client = http_client()?;

        let start = std::time::Instant::now();
        match client.get(&models_url).bearer_auth(api_key).send().await {
            Ok(response) => {
                let status = response.status();
                let latency_ms = Some(start.elapsed().as_millis());
                if status.is_success() {
                    Ok(ProfileConnectionResult {
                        ok: true,
                        latency_ms,
                        status: Some(status.as_u16()),
                        error: None,
                    })
                } else if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    Ok(ProfileConnectionResult {
                        ok: false,
                        latency_ms,
                        status: Some(status.as_u16()),
                        error: Some("API 密钥无效".to_string()),
                    })
                } else {
                    Ok(ProfileConnectionResult {
                        ok: false,
                        latency_ms,
                        status: Some(status.as_u16()),
                        error: Some(format!("接口返回 HTTP {status}")),
                    })
                }
            }
            Err(error) => {
                let status = error.status().map(|status| status.as_u16());
                let error_message = reqwest_error_message(&error);
                Ok(ProfileConnectionResult {
                    ok: false,
                    latency_ms: None,
                    status,
                    error: Some(error_message),
                })
            }
        }
    }

    /// 按供应商查询 DeepSeek 余额：使用该供应商自己保存的 API 密钥，以配置为单位查询。
    pub async fn get_deepseek_balance(&self, id: &str) -> AppResult<DeepSeekBalance> {
        let stored = self.database.profile(id)?;
        let payload = &stored.payload;
        if payload.provider_id.as_deref() != Some("deepseek") {
            return Err(app_err!("该供应商不是 DeepSeek，无法查询余额"));
        }
        let body = payload
            .provider_body
            .as_deref()
            .ok_or_else(|| app_err!("该供应商缺少配置数据"))?;
        let detail = parse_provider_detail(body)?;
        let api_key = stored_provider_api_key(payload)
            .ok_or_else(|| app_err!("该供应商没有配置 API 密钥，无法查询余额"))?;
        let base = detail
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("https://api.deepseek.com");
        let balance_url = format!("{}/user/balance", base.trim_end_matches('/'));
        let client = http_client()?;

        let start = std::time::Instant::now();
        let response = client.get(&balance_url).bearer_auth(&api_key).send().await;
        match response {
            Ok(response) => {
                let status = response.status();
                let latency_ms = Some(start.elapsed().as_millis());
                if status.is_success() {
                    let parsed = response
                        .json::<DeepSeekBalanceResponse>()
                        .await
                        .map_err(|error| app_err!("余额接口响应解析失败: {error}"))?;
                    Ok(DeepSeekBalance {
                        is_available: parsed.is_available,
                        balance_infos: parsed.balance_infos,
                        latency_ms,
                    })
                } else if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::FORBIDDEN
                {
                    Err(app_err!("API 密钥无效或无权查询余额（HTTP {status}）"))
                } else {
                    let message = response
                        .json::<serde_json::Value>()
                        .await
                        .ok()
                        .and_then(|value| {
                            value
                                .get("error")
                                .and_then(|error| error.get("message"))
                                .and_then(serde_json::Value::as_str)
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| format!("接口返回 HTTP {status}"));
                    Err(app_err!("余额查询失败：{message}"))
                }
            }
            Err(error) => {
                let error_message = reqwest_error_message(&error);
                Err(app_err!("余额查询失败：{error_message}"))
            }
        }
    }

    pub fn export_database(&self) -> AppResult<PathBuf> {
        let directory = &self.paths.database_backup;
        std::fs::create_dir_all(directory)
            .map_err(|error| app_err!("无法创建备份目录: {error}"))?;
        let name = format!("cgswitch-export-{}.db", now_ms());
        let target = directory.join(&name);
        self.database.export_database(&target)?;
        prune_backups(directory, "cgswitch-export-", ".db", 20);
        self.database.record_event(
            None,
            "export",
            "success",
            Some("database exported"),
            &now_ms().to_string(),
        )?;
        Ok(target)
    }

    /// 从用户选择的备份文件导入并恢复。
    pub fn import_database(&self, path: &str) -> AppResult<()> {
        let source = PathBuf::from(path);
        let canonical = source
            .canonicalize()
            .map_err(|_| app_err!("备份文件不存在：{path}"))?;
        let live = self
            .paths
            .database
            .canonicalize()
            .unwrap_or_else(|_| self.paths.database.clone());
        if canonical == live {
            return Err(app_err!("不能导入当前正在使用的数据库文件"));
        }
        self.database.restore_from_backup(&canonical)?;
        self.database.record_event(
            None,
            "import",
            "success",
            Some("database imported"),
            &now_ms().to_string(),
        )?;
        Ok(())
    }

    pub fn list_database_backups(&self) -> AppResult<Vec<DatabaseBackupInfo>> {
        let directory = &self.paths.database_backup;
        let mut backups = Vec::new();
        if let Ok(entries) = std::fs::read_dir(directory) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !(name.starts_with("cgswitch-export-") && name.ends_with(".db")) {
                    continue;
                }
                let size_bytes = entry.metadata().map(|metadata| metadata.len()).unwrap_or(0);
                let created_at = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or(0);
                backups.push(DatabaseBackupInfo {
                    name,
                    size_bytes,
                    created_at,
                });
            }
        }
        backups.sort_by(|left, right| right.name.cmp(&left.name));
        Ok(backups)
    }

    pub fn restore_database(&self, name: &str) -> AppResult<()> {
        let path = self.database_backup_path(name)?;
        self.database.restore_from_backup(&path)?;
        self.database.record_event(
            None,
            "restore",
            "success",
            Some("database restored"),
            &now_ms().to_string(),
        )?;
        Ok(())
    }

    pub fn delete_database_backup(&self, name: &str) -> AppResult<()> {
        let path = self.database_backup_path(name)?;
        std::fs::remove_file(&path).map_err(|error| app_err!("删除备份失败: {error}"))?;
        Ok(())
    }

    /// 重命名备份（标题写入文件名，保留 cgswitch-export- 前缀与 .db 后缀）。
    pub fn rename_database_backup(&self, old_name: &str, title: &str) -> AppResult<()> {
        let from = self.database_backup_path(old_name)?;
        let mut stem = title.trim().to_string();
        if let Some(rest) = stem.strip_prefix("cgswitch-export-") {
            stem = rest.to_string();
        }
        if stem.ends_with(".db") {
            stem.truncate(stem.len() - 3);
        }
        let stem: String = stem
            .chars()
            .filter(|ch| !matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
            .take(80)
            .collect();
        let stem = stem.trim();
        if stem.is_empty() {
            return Err(app_err!("备份标题不能为空"));
        }
        let to = self
            .paths
            .database_backup
            .join(format!("cgswitch-export-{stem}.db"));
        if to == from {
            return Ok(());
        }
        if to.exists() {
            return Err(app_err!("同名备份已存在"));
        }
        std::fs::rename(&from, &to).map_err(|error| app_err!("重命名备份失败: {error}"))?;
        Ok(())
    }

    fn database_backup_path(&self, name: &str) -> AppResult<PathBuf> {
        let valid = name.starts_with("cgswitch-export-")
            && name.ends_with(".db")
            && Path::new(name).file_name().and_then(|file| file.to_str()) == Some(name);
        if !valid {
            return Err(app_err!("无效的备份文件名"));
        }
        Ok(self.paths.database_backup.join(name))
    }

    pub fn rename_profile(&self, id: &str, name: &str) -> AppResult<()> {
        let name = validated_name(name)?;
        self.database
            .rename_profile(id, &name, &now_ms().to_string())
    }

    pub fn delete_profile(&self, id: &str) -> AppResult<()> {
        self.database.delete_profile(id)?;
        if self.active_profile_state()?.as_deref() == Some(id) {
            self.database.set_active_profile(None)?;
        }
        Ok(())
    }

    pub fn set_profile_icon(&self, id: &str, icon: Option<&str>) -> AppResult<()> {
        let icon = validated_icon(icon)?;
        self.database
            .set_profile_icon(id, icon.as_deref(), &now_ms().to_string())
    }

    /// 供应商级开关：是否在卡片显示并自动刷新 DeepSeek 余额。
    pub fn set_profile_show_balance(&self, id: &str, enabled: bool) -> AppResult<()> {
        let stored = self.database.profile(id)?;
        let mut payload = stored.payload;
        payload.show_balance = enabled;
        self.database
            .update_profile(id, &stored.name, &payload, &now_ms().to_string())
            .map(|_| ())
    }

    /// 供应商级余额缓存：上次成功查询结果写入 ~/.cgswitch/balance-cache.json，
    /// 保证卡片首次渲染/切换视图时数字就在，不出现“消失→出现”的闪烁。
    pub fn set_profile_balance(
        &self,
        profile_id: &str,
        info: &DeepSeekBalanceInfo,
    ) -> AppResult<()> {
        let mut cache = self.load_balance_cache();
        cache.insert(profile_id.to_string(), info.clone());
        self.save_balance_cache(&cache)
    }

    fn balance_cache_path(&self) -> PathBuf {
        self.paths.root.join("balance-cache.json")
    }

    fn load_balance_cache(&self) -> BTreeMap<String, DeepSeekBalanceInfo> {
        std::fs::read_to_string(self.balance_cache_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn save_balance_cache(&self, cache: &BTreeMap<String, DeepSeekBalanceInfo>) -> AppResult<()> {
        let text = serde_json::to_string(cache)
            .map_err(|error| app_err!("余额缓存序列化失败: {error}"))?;
        atomic_write(&self.balance_cache_path(), text.as_bytes())
    }

    /// 完整复制供应商（配置、关联文件、图标、账号绑定），新供应商名加“副本”后缀，同名时追加序号。
    pub fn duplicate_profile(&self, id: &str) -> AppResult<ProfileSummary> {
        // 使用中的供应商：先把 live 的 config/models.json 改动同步回快照，副本取到最新状态
        let active = self.is_active_profile(id)?;
        if active {
            if let Some(document) = self.live_document() {
                let _ = self.sync_active_profile_document(&document);
            }
        }
        let mut stored = self.database.profile(id)?;
        // 使用中的第三方供应商：快照没单独保存 auth 时连当前 live auth.json 一起复制，
        // 保证副本应用后凭据与源一致；官方订阅的 auth 由账号动态生成，不复制。
        if active && stored.kind == ProfileKind::ThirdParty && stored.payload.raw_auth.is_none() {
            stored.payload.raw_auth = read_optional_text(&self.paths.codex_home.join("auth.json"));
        }
        let profiles = self.database.profiles()?;
        let base: String = stored.name.trim().chars().take(47).collect();
        let mut candidate = format!("{base} 副本");
        let mut counter = 2;
        while profiles
            .iter()
            .any(|profile| profile.name.eq_ignore_ascii_case(&candidate))
        {
            candidate = format!("{base} 副本 {counter}");
            counter += 1;
        }
        let timestamp = now_ms().to_string();
        let summary = self
            .database
            .insert_profile(&candidate, &stored.payload, &timestamp)?;
        self.database
            .set_profile_icon(&summary.id, stored.icon.as_deref(), &timestamp)?;
        // 官方供应商的订阅账号绑定一并复制（第三方恒为 None 不会进这个分支）
        if stored.account_id.is_some() {
            self.database.set_profile_account(
                &summary.id,
                stored.account_id.as_deref(),
                &timestamp,
            )?;
        }
        self.database.record_event(
            Some(&summary.id),
            "duplicate",
            "success",
            Some("profile duplicated"),
            &timestamp,
        )?;
        let stored = self.database.profile(&summary.id)?;
        Ok(profile_summary(&stored))
    }

    pub fn get_profile(&self, id: &str) -> AppResult<ProfileDetail> {
        // 打开激活供应商的编辑页：先把外部改动同步回数据库快照
        if self.is_active_profile(id)? {
            if let Some(document) = self.live_document() {
                let _ = self.sync_active_profile_document(&document);
            }
        }
        let stored = self.database.profile(id)?;
        let payload = &stored.payload;
        let active = self.is_active_profile(id)?;
        let provider = payload
            .provider_body
            .as_deref()
            .map(parse_provider_detail)
            .transpose()?;
        let stored_key = provider.as_ref().and_then(|detail| detail.api_key.clone());
        let api_key = stored_key
            .as_deref()
            .filter(|key| !is_builtin_placeholder(payload, key))
            .map(str::to_string);

        // 使用中：live 文件是唯一事实源；未使用：数据库快照
        let live_config = active
            .then(|| read_optional_text(&self.paths.codex_config()))
            .flatten();
        let live_catalog = if active {
            payload
                .model_values
                .get("model_catalog_json")
                .and_then(|raw| self.resolve_codex_path(raw))
                .and_then(|file| read_optional_text(&file))
        } else {
            None
        };
        let live_auth = active
            .then(|| read_optional_text(&self.paths.codex_home.join("auth.json")))
            .flatten();

        // 使用中：live 文件原样展示；未使用：数据库快照原样展示（所见即所得，不再掩码）
        let raw_config = live_config.or_else(|| payload.raw_config.clone());
        let config_fragment = match raw_config.as_deref() {
            Some(raw) => match payload.builtin.as_deref() {
                // 内置供应商：占位符替换为已存密钥，展示应用时的真实配置
                Some(kind) => {
                    let template = builtin::template(kind)?;
                    String::from_utf8_lossy(
                        &template.substitute_key(raw.as_bytes().to_vec(), stored_key.as_deref())?,
                    )
                    .into_owned()
                }
                None => raw.to_string(),
            },
            None => match payload.builtin.as_deref() {
                Some(kind) => {
                    let template = builtin::template(kind)?;
                    String::from_utf8_lossy(&template.render_config(stored_key.as_deref())?)
                        .into_owned()
                }
                None => profile_config_fragment(payload),
            },
        };
        let catalog_content = if active {
            live_catalog.or_else(|| payload.raw_catalog.clone())
        } else {
            payload.raw_catalog.clone()
        }
        .or_else(|| {
            payload
                .builtin
                .as_deref()
                .and_then(|kind| builtin::template(kind).ok())
                .and_then(|template| template.catalog)
                .map(|(_, bytes)| String::from_utf8_lossy(bytes).into_owned())
        });

        Ok(ProfileDetail {
            id: stored.id.clone(),
            name: stored.name.clone(),
            account_id: stored.account_id.clone(),
            icon: stored.icon.clone(),
            provider: payload.provider_id.clone(),
            base_url: provider.as_ref().and_then(|detail| detail.base_url.clone()),
            api_key,
            model_values: payload.model_values.clone(),
            config_fragment,
            raw_config,
            auth_content: payload.raw_auth.clone().or_else(|| live_auth),
            catalog_content,
            raw_catalog: payload.raw_catalog.clone(),
            raw_auth: payload.raw_auth.clone(),
            admin_url: payload.admin_url.clone(),
            show_balance: payload.show_balance,
            updated_at: stored.updated_at.clone(),
        })
    }

    /// 保存供应商自身的完整配置原文：内置供应商存 raw_config（应用时整文件回填）；
    /// 普通供应商解析回结构化字段（继续走合并回填）。models.json 统一存 raw_catalog。
    pub fn update_profile_config(
        &self,
        id: &str,
        config_text: &str,
        catalog_text: Option<&str>,
        auth_text: Option<&str>,
    ) -> AppResult<ProfileDetail> {
        let stored = self.database.profile(id)?;
        let mut payload = stored.payload;

        // 清空 auth 内容 = 移除档案级覆盖，恢复为账号自动凭据
        let auth_override = auth_text
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
        let document = codex_config::parse_document(config_text)?;
        if let Some(provider_id) = payload.provider_id.as_deref() {
            let provider_ok = document
                .as_table()
                .get("model_provider")
                .and_then(toml_edit::Item::as_str)
                .is_some_and(|current| current == provider_id)
                && document
                    .as_table()
                    .get("model_providers")
                    .and_then(toml_edit::Item::as_table)
                    .and_then(|providers| providers.get(provider_id))
                    .is_some();
            if !provider_ok {
                return Err(app_err!(
                    "编辑后的配置缺少或改名了 model_providers.{provider_id}，已取消保存"
                ));
            }
        }
        if let Some(text) = catalog_text {
            serde_json::from_str::<serde_json::Value>(text)
                .map_err(|error| app_err!("models.json 不是有效 JSON: {error}"))?;
        }
        if let Some(text) = auth_override.as_deref() {
            serde_json::from_str::<serde_json::Value>(text)
                .map_err(|error| app_err!("auth.json 不是有效 JSON: {error}"))?;
        }

        // 所见即所得：编辑器文本是唯一事实源，内置/普通供应商都重新解析结构化字段
        let parsed = codex_config::capture_from_document(&document)?;
        payload.model_values = parsed.model_values;
        payload.provider_body = parsed.provider_body;
        payload.raw_config = Some(config_text.to_string());
        if catalog_text.is_some() {
            payload.raw_catalog = catalog_text.map(str::to_string);
        }
        if auth_text.is_some() {
            payload.raw_auth = auth_override.clone();
        }
        self.database
            .update_profile(id, &stored.name, &payload, &now_ms().to_string())?;

        // 使用中：编辑内容立即写进当前 Codex 文件（是否生效由 Codex 重启决定）
        if self.is_active_profile(id)? {
            let config_path = self.paths.codex_config();
            backup_file(&config_path, &self.paths.config_backup, "config")?;
            atomic_write(&config_path, config_text.as_bytes())?;
            if catalog_text.is_some() {
                self.write_raw_catalog(&payload)?;
            }
            if auth_text.is_some() && payload.raw_auth.is_some() {
                self.write_raw_auth(&payload)?;
            }
        }
        self.get_profile(id)
    }

    pub fn update_profile(
        &self,
        id: &str,
        name: &str,
        base_url: Option<&str>,
        api_key: Option<&str>,
        admin_url: Option<&str>,
    ) -> AppResult<ProfileSummary> {
        let name = validated_name(name)?;
        let stored = self.database.profile(id)?;
        let mut payload = stored.payload;
        let admin_url = admin_url.map(str::trim).filter(|value| !value.is_empty());
        if let Some(url) = admin_url {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(app_err!("管理后台网址必须以 http:// 或 https:// 开头"));
            }
        }
        payload.admin_url = admin_url.map(str::to_string);
        if payload.provider_id.is_some() {
            let body = payload
                .provider_body
                .as_deref()
                .ok_or_else(|| app_err!("该供应商缺少配置数据"))?;
            if base_url.is_some() || api_key.is_some() {
                payload.provider_body =
                    Some(codex_config::update_provider_body(body, base_url, api_key)?);
            }
        } else if base_url.is_some() || api_key.is_some() {
            return Err(app_err!("该供应商缺少配置，无法修改调用地址或密钥"));
        }
        let write_back = (base_url.is_some() || api_key.is_some())
            && payload.provider_id.is_some()
            && self.is_active_profile(id)?;
        let updated = self
            .database
            .update_profile(id, &name, &payload, &now_ms().to_string())?;
        if write_back {
            // 使用中：只就地更新 live 的供应商段落，保留 Codex 期间生成的其他内容
            self.write_live_provider_update(
                id,
                payload.provider_id.as_deref().expect("已检查 provider_id"),
                base_url,
                api_key,
            )?;
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

    pub fn apply_profile(&self, id: &str) -> AppResult<()> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        let config_path = self.paths.codex_config();
        let original = std::fs::read_to_string(&config_path)
            .map_err(|error| app_err!("无法读取 {}: {error}", config_path.display()))?;
        let mut document = codex_config::parse_document(&original)?;

        // 切换前把当前 live 配置回写进正在生效的供应商，使供应商跟随使用中的累计更新
        self.autosync_active_profile(id, &document)?;

        let payload = self.database.profile(id)?.payload;
        if payload.builtin.is_some() {
            self.apply_builtin_profile(id, &payload, "apply")?;
        } else if let Some(raw) = &payload.raw_config {
            // 完整快照供应商：直接回填完整原文（含 MCP、插件、注释等全部内容）
            backup_file(&config_path, &self.paths.config_backup, "config")?;
            atomic_write(&config_path, raw.as_bytes())?;
            self.write_raw_catalog(&payload)?;
            self.write_raw_auth(&payload)?;
            self.database.record_event(
                Some(id),
                "apply",
                "success",
                Some("configuration applied"),
                &now_ms().to_string(),
            )?;
        } else {
            codex_config::apply_to_document(&mut document, &payload)?;
            let updated = document.to_string();

            backup_file(&config_path, &self.paths.config_backup, "config")?;
            atomic_write(&config_path, updated.as_bytes())?;
            self.write_raw_catalog(&payload)?;
            self.write_raw_auth(&payload)?;
            self.database.record_event(
                Some(id),
                "apply",
                "success",
                Some("configuration applied"),
                &now_ms().to_string(),
            )?;
        }
        // 显式记录当前激活供应商，避免依赖应用日志反推
        self.database.set_active_profile(Some(id))?;
        Ok(())
    }

    /// 把 OAuth 账号的订阅凭据按官方格式写入 ~/.codex/auth.json，
    /// 让 Codex 直接使用该 ChatGPT 订阅额度，而不是 API key。
    pub async fn apply_oauth_auth(
        &self,
        oauth: &CodexOAuthState,
        account_id: &str,
    ) -> AppResult<()> {
        let content = oauth
            .0
            .read()
            .await
            .codex_auth_json(account_id)
            .await
            .map_err(|error| app_err!("{error}"))?;
        self.write_codex_auth_json(&content)
    }

    /// 把订阅凭据原文写入 ~/.codex/auth.json（写前备份旧文件）。
    pub fn write_codex_auth_json(&self, content: &str) -> AppResult<()> {
        let destination = self.paths.codex_home.join("auth.json");
        backup_file(&destination, &self.paths.codex_files_backup, "auth")?;
        atomic_write(&destination, content.as_bytes())?;
        Ok(())
    }

    /// 是否为官方订阅供应商（无 API 供应商，凭据走 ChatGPT 订阅）。
    pub fn is_subscription_profile(&self, id: &str) -> AppResult<bool> {
        Ok(self.database.profile(id)?.kind == ProfileKind::Official)
    }

    /// 官方供应商绑定的订阅账号；未绑定返回 None（由调用方回退默认账号）。
    pub fn bound_account_id(&self, id: &str) -> AppResult<Option<String>> {
        Ok(self.database.profile(id)?.account_id.clone())
    }

    fn active_profile_state(&self) -> AppResult<Option<String>> {
        Ok(self.database.app_state()?.0)
    }

    /// 该供应商是否为当前使用中（以显式激活状态为准，不做配置比对）。
    pub fn is_active_profile(&self, id: &str) -> AppResult<bool> {
        Ok(self.active_profile_state()?.as_deref() == Some(id))
    }

    /// 官方供应商是否保存了自己的 auth.json 覆盖（有则应用时不再用账号现生成）。
    pub fn has_auth_override(&self, id: &str) -> AppResult<bool> {
        Ok(self.database.profile(id)?.payload.raw_auth.is_some())
    }

    /// 官方供应商绑定订阅账号；第三方供应商直接拒绝。None 表示跟随默认账号。
    pub fn set_profile_account(&self, id: &str, account_id: Option<&str>) -> AppResult<()> {
        let stored = self.database.profile(id)?;
        if stored.kind != ProfileKind::Official {
            return Err(app_err!("第三方供应商不支持绑定订阅账号"));
        }
        if let Some(account_id) = account_id {
            if !self
                .database
                .accounts()?
                .iter()
                .any(|account| account.id == account_id)
            {
                return Err(app_err!("订阅账号不存在"));
            }
        }
        self.database
            .set_profile_account(id, account_id, &now_ms().to_string())
    }

    /// 内置官方供应商：整文件替换为模板原文（仅替换密钥占位符），
    /// 并写入本供应商自带的关联文件（deepseek/智谱各自独立的 models.json、minimax 的 custom-catalog.json），
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
            .ok_or_else(|| app_err!("供应商缺少内置类型"))?;
        let template = builtin::template(kind)?;
        let api_key = payload.provider_body.as_deref().and_then(provider_api_key);
        // 带密钥占位符的内置供应商：应用前必须已配置真实密钥，避免把占位符写进 live 配置
        if template.placeholder.is_some()
            && api_key
                .as_deref()
                .is_none_or(|key| key.trim().is_empty() || is_builtin_placeholder(payload, key))
        {
            return Err(app_err!(
                "该供应商尚未配置 API 密钥，请先在编辑页填写 API 密钥后再应用"
            ));
        }
        let rendered = match &payload.raw_config {
            Some(raw) => template.substitute_key(raw.as_bytes().to_vec(), api_key.as_deref())?,
            None => template.render_config(api_key.as_deref())?,
        };

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
            match &payload.raw_catalog {
                Some(raw) => atomic_write(&destination, raw.as_bytes())?,
                None => atomic_write(&destination, bytes)?,
            }
        }

        self.database.record_event(
            Some(profile_id),
            action,
            "success",
            Some("built-in configuration applied"),
            &now_ms().to_string(),
        )?;
        self.write_raw_auth(payload)?;
        Ok(())
    }

    /// 把供应商自己编辑保存的 models.json 原文写入 model_catalog_json 指向的位置。
    fn write_raw_catalog(&self, payload: &ProfilePayload) -> AppResult<()> {
        let Some(raw) = payload.raw_catalog.as_deref() else {
            return Ok(());
        };
        let Some(raw_path) = payload.model_values.get("model_catalog_json") else {
            return Ok(());
        };
        let Some(destination) = self.resolve_codex_path(raw_path) else {
            return Ok(());
        };
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| app_err!("无法创建目录 {}: {error}", parent.display()))?;
        }
        let stem = destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("catalog");
        backup_file(&destination, &self.paths.codex_files_backup, stem)?;
        atomic_write(&destination, raw.as_bytes())?;
        Ok(())
    }

    /// 把供应商自己编辑保存的 auth.json 原文写入 ~/.codex/auth.json。
    fn write_raw_auth(&self, payload: &ProfilePayload) -> AppResult<()> {
        let Some(raw) = payload.raw_auth.as_deref() else {
            return Ok(());
        };
        let destination = self.paths.codex_home.join("auth.json");
        backup_file(&destination, &self.paths.codex_files_backup, "auth")?;
        atomic_write(&destination, raw.as_bytes())?;
        Ok(())
    }

    /// 解析 model_catalog_json 指向的路径：支持绝对路径、~/ 开头、以及相对 ~/.codex 的路径。
    fn resolve_codex_path(&self, raw: &str) -> Option<PathBuf> {
        let path = raw.trim().trim_matches('"');
        if path.is_empty() {
            return None;
        }
        let raw_path = Path::new(path);
        if let Some(rest) = path.strip_prefix("~/") {
            Some(
                self.paths
                    .codex_home
                    .parent()
                    .unwrap_or(&self.paths.codex_home)
                    .join(rest),
            )
        } else if raw_path.is_absolute() {
            Some(raw_path.to_path_buf())
        } else {
            Some(self.paths.codex_home.join(raw_path))
        }
    }

    /// 把 live 文档同步进当前激活供应商的快照：无激活供应商或内容无差异时不做任何写库。
    /// 供 get_state（刷新/窗口激活）与 get_profile（打开编辑页）按需调用。
    fn sync_active_profile_document(&self, document: &toml_edit::DocumentMut) -> AppResult<bool> {
        let Some(active_id) = self.active_profile_state()? else {
            return Ok(false);
        };
        let Some(profile) = self
            .database
            .profiles()?
            .iter()
            .find(|profile| profile.id == active_id)
            .cloned()
        else {
            return Ok(false);
        };
        let Ok(mut live) = codex_config::capture_from_document(document) else {
            return Ok(false);
        };
        live.builtin = profile.payload.builtin.clone();
        // 供应商元数据（管理后台网址/余额开关）不属于 live 文档，同步时保留
        live.admin_url = profile.payload.admin_url.clone();
        live.show_balance = profile.payload.show_balance;
        // 使用中模型目录按 live 文件回写；档案自己保存的 auth 覆盖保持不变
        live.raw_catalog = profile
            .payload
            .model_values
            .get("model_catalog_json")
            .and_then(|raw| self.resolve_codex_path(raw))
            .and_then(|file| read_optional_text(&file))
            .or_else(|| profile.payload.raw_catalog.clone());
        live.raw_auth = profile.payload.raw_auth.clone();
        // 快照跟随当前 live 完整文本，保证供应商是完整状态（所见即所得，不掩码密钥）
        live.raw_config = Some(document.to_string());
        if live == profile.payload {
            return Ok(false);
        }
        if let Err(error) =
            self.database
                .update_profile(&active_id, &profile.name, &live, &now_ms().to_string())
        {
            let _ = self.database.record_event(
                Some(&active_id),
                "sync",
                "failed",
                Some(&error.0),
                &now_ms().to_string(),
            );
            return Ok(false);
        }
        Ok(true)
    }

    fn autosync_active_profile(
        &self,
        target_id: &str,
        document: &toml_edit::DocumentMut,
    ) -> AppResult<()> {
        // 只回写手动应用过的供应商，不做 live 配置推断
        let Some(active_id) = self.active_profile_state()? else {
            return Ok(());
        };
        if active_id == target_id {
            return Ok(());
        }
        self.sync_active_profile_document(document)?;
        Ok(())
    }

    pub fn restart_codex(&self, app: &AppHandle) -> AppResult<()> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        let settings = self.settings()?;
        emit(app, "stopping", None);

        let process_ids = codex_process::find_process_ids(settings.codex_app_path.as_deref());
        if !process_ids.is_empty() {
            codex_process::terminate_process_ids(&process_ids);
            emit(app, "waiting", None);
            // 固定等待 5 秒（可配置的“重启等待超时”已移除）
            let exited = codex_process::wait_for_exit(&process_ids, 5_000, 100);
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
        let path = &self.paths.settings;
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // 首次运行：写出默认设置文件，方便直接手动编辑
                let defaults = Settings::default();
                let text = serde_json::to_string_pretty(&defaults)
                    .map_err(|_| app_err!("默认设置序列化失败"))?;
                atomic_write(path, text.as_bytes())?;
                return Ok(defaults);
            }
            Err(error) => return Err(app_err!("无法读取设置文件 {}: {error}", path.display())),
        };
        serde_json::from_str(&text)
            .map_err(|error| app_err!("设置文件 {} 无效: {error}", path.display()))
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<Settings> {
        let mut settings = settings.clone();
        settings.theme = settings.theme.trim().to_lowercase();
        if !["system", "light", "dark"].contains(&settings.theme.as_str()) {
            return Err(app_err!("不支持的主题设置"));
        }
        settings.codex_app_path = settings
            .codex_app_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let text =
            serde_json::to_string_pretty(&settings).map_err(|_| app_err!("设置序列化失败"))?;
        atomic_write(&self.paths.settings, text.as_bytes())?;
        Ok(settings)
    }

    pub fn open_path(&self, path: &str) -> AppResult<()> {
        if !self.is_managed_path(path) {
            return Err(app_err!("不能打开未列出的本机路径"));
        }
        open_in_file_explorer(Path::new(path))
    }

    fn path_info(&self) -> Vec<PathInfo> {
        vec![
            PathInfo {
                label: "数据库".into(),
                path: self.paths.database.display().to_string(),
            },
            PathInfo {
                label: "设置文件".into(),
                path: self.paths.settings.display().to_string(),
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
        ]
    }

    fn is_managed_path(&self, path: &str) -> bool {
        self.path_info().iter().any(|item| item.path == path)
    }
}

fn validated_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 50 {
        return Err(app_err!("供应商名称长度必须在 1 到 50 个字符之间"));
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
        // A 显式激活，成为唯一“使用中”来源
        context.apply_profile(&profile_a.id).unwrap();

        // A 使用期间 live 配置累计了新的模型键和 provider 字段
        write(
            r#"
model = "glm-5.3"
model_provider = "ZAI"
model_reasoning_effort = "high"
model_catalog_json = "zai.json"

[mcp_servers.keep]
command = "node"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
experimental_bearer_token = "secret"
new_field = "accumulated"
"#,
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
        let profile_b = context.capture_profile("B").unwrap();

        // 切到 B：autosync 应把 A 使用期间的累计改动写回 A 的快照
        context.apply_profile(&profile_b.id).unwrap();

        let stored_a = context.database.profile(&profile_a.id).unwrap();
        assert_eq!(
            stored_a
                .payload
                .model_values
                .get("model_catalog_json")
                .map(|raw| raw.trim().trim_matches('"')),
            Some("zai.json")
        );
        assert!(stored_a
            .payload
            .provider_body
            .as_deref()
            .unwrap()
            .contains("new_field = \"accumulated\""));
        assert_eq!(
            context.get_state().unwrap().active_profile_id.as_deref(),
            Some(profile_b.id.as_str())
        );
    }

    #[test]
    fn capture_sets_active_and_autosyncs_previous() {
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
        assert_eq!(
            context.get_state().unwrap().active_profile_id.as_deref(),
            Some(profile_a.id.as_str())
        );

        // A 使用期间 live 累计了新键，再次捕获 B：A 快照被同步，激活转到 B
        write(
            r#"
model = "glm-5.3"
model_provider = "ZAI"
model_reasoning_effort = "high"
model_catalog_json = "zai.json"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
experimental_bearer_token = "secret"
new_field = "accumulated"
"#,
        );
        let profile_b = context.capture_profile("B").unwrap();

        let state = context.get_state().unwrap();
        assert_eq!(
            state.active_profile_id.as_deref(),
            Some(profile_b.id.as_str())
        );
        let stored_a = context.database.profile(&profile_a.id).unwrap();
        assert!(stored_a
            .payload
            .provider_body
            .as_deref()
            .unwrap()
            .contains("new_field = \"accumulated\""));
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

        // 捕获即设为使用中；先清掉激活状态，验证“未使用”只读库快照
        context.database.set_active_profile(None).unwrap();
        let inactive = context.get_profile(&profile.id).unwrap();
        assert_eq!(inactive.catalog_content, None);
        assert_eq!(inactive.auth_content, None);

        // 使用中：live 文件是唯一事实源
        context.apply_profile(&profile.id).unwrap();
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
        context.apply_profile(&profile.id).unwrap();
        context
            .update_profile(
                &profile.id,
                "ZAI",
                Some("https://new.example"),
                Some("new-key"),
                None,
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
    fn update_profile_allows_duplicate_name() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        context.capture_profile("First").unwrap();
        // 供应商 id 取毫秒时间戳，同毫秒内二次捕获会撞 id；真实 UI 不可能，测试里隔开
        std::thread::sleep(std::time::Duration::from_millis(2));
        let second = context.capture_profile("Second").unwrap();

        // 名字不是唯一键，重命名为已存在的名字应允许，靠 ID 区分
        let updated = context
            .update_profile(&second.id, "first", None, None, None)
            .unwrap();
        assert_eq!(updated.name, "first");
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
            .add_builtin_profile(
                "deepseek",
                Some("https://custom.example"),
                Some("sk-test"),
                None,
                None,
            )
            .unwrap();

        assert_eq!(profile.name, "DeepSeek");
        assert_eq!(profile.model.as_deref(), Some("deepseek-v4-flash"));
        assert_eq!(profile.provider.as_deref(), Some("deepseek"));
        assert_eq!(profile.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(profile.icon.as_deref(), Some("deepseek"));
        assert!(profile.has_key);

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

        // 同名模板允许重复添加，名字相同，靠 ID 区分
        let duplicate = context
            .add_builtin_profile("deepseek", None, Some("sk-test"), None, None)
            .unwrap();
        assert_eq!(duplicate.name, "DeepSeek");
        assert_ne!(duplicate.id, profile.id);
    }

    #[test]
    fn get_builtin_catalog_returns_embedded_file_content() {
        let home = tempfile::tempdir().unwrap();
        let context = AppContext::new(crate::paths::from_home(home.path()).unwrap()).unwrap();

        assert_eq!(
            context.get_builtin_catalog("deepseek").unwrap(),
            Some(String::from_utf8_lossy(crate::builtin::DEEPSEEK_MODELS).into_owned())
        );
        assert_eq!(
            context.get_builtin_catalog("zhipu").unwrap(),
            Some(String::from_utf8_lossy(crate::builtin::ZHIPU_MODELS).into_owned())
        );
        assert_eq!(
            context.get_builtin_catalog("minimax").unwrap(),
            Some(String::from_utf8_lossy(crate::builtin::MINIMAX_CATALOG).into_owned())
        );
        assert_eq!(context.get_builtin_catalog("chatgpt").unwrap(), None);
        assert!(context.get_builtin_catalog("unknown").is_err());
    }

    #[tokio::test]
    async fn deepseek_balance_rejects_non_deepseek_or_keyless() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();
        let context = AppContext::new(paths).unwrap();

        // 非 DeepSeek 供应商拒绝
        let minimax = context
            .add_builtin_profile("minimax", None, Some("mm-key"), None, None)
            .unwrap();
        let error = context.get_deepseek_balance(&minimax.id).await.unwrap_err();
        assert!(error.0.contains("该供应商不是 DeepSeek"));

        // DeepSeek 但只有占位符密钥（未配置真实密钥）拒绝
        let keyless = context
            .add_builtin_profile("deepseek", None, None, None, None)
            .unwrap();
        let error = context.get_deepseek_balance(&keyless.id).await.unwrap_err();
        assert!(error.0.contains("没有配置 API 密钥"));
    }

    #[test]
    fn get_state_tags_active_profile_from_live_config() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            "model = \"glm-5.3\"\nmodel_provider = \"ZAI\"\nmodel_reasoning_effort = \"high\"\n\n[model_providers.ZAI]\nname = \"ZAI\"\nbase_url = \"https://api.example\"\nexperimental_bearer_token = \"secret\"\n",
        )
        .unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("ZAI High").unwrap();
        context.apply_profile(&profile.id).unwrap();

        // 数据库快照滞后：DB 里推理强度是 old，live 配置手动改成 medium 并累计新键
        let mut payload = context.database.profile(&profile.id).unwrap().payload;
        payload
            .model_values
            .insert("model_reasoning_effort".into(), "\"old\"".into());
        context
            .database
            .update_profile(&profile.id, "ZAI High", &payload, &now_ms().to_string())
            .unwrap();
        std::fs::write(
            context.paths.codex_config(),
            "model = \"glm-5.3\"\nmodel_provider = \"ZAI\"\nmodel_reasoning_effort = \"medium\"\nmodel_catalog_json = \"zai.json\"\n\n[mcp_servers.keep]\ncommand = \"node\"\n\n[model_providers.ZAI]\nname = \"ZAI\"\nbase_url = \"https://api.example\"\nexperimental_bearer_token = \"secret\"\n",
        )
        .unwrap();

        let state = context.get_state().unwrap();
        assert_eq!(
            state.active_profile_id.as_deref(),
            Some(profile.id.as_str())
        );
        let summary = state
            .profiles
            .iter()
            .find(|item| item.id == profile.id)
            .unwrap();
        assert_eq!(summary.model.as_deref(), Some("glm-5.3"));
        assert_eq!(summary.provider.as_deref(), Some("ZAI"));
        assert_eq!(summary.reasoning_effort.as_deref(), Some("medium"));
        // get_state 按需同步：外部改动已回写进数据库快照
        assert_eq!(
            context
                .database
                .profile(&profile.id)
                .unwrap()
                .payload
                .model_values
                .get("model_reasoning_effort")
                .map(|raw| raw.trim().trim_matches('"')),
            Some("medium")
        );
    }

    #[test]
    fn get_state_and_profile_sync_active_snapshot_from_live() {
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
        let profile = context.capture_profile("ZAI").unwrap();

        // 外部把 live 换成另一套配置
        write(
            r#"
model = "glm-5.3-pro"
model_provider = "ZAI"
model_reasoning_effort = "max"
model_catalog_json = "zai.json"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://new.example"
experimental_bearer_token = "secret"
"#,
        );

        // 打开编辑页（get_profile）即触发同步：DB 快照跟随 live
        let detail = context.get_profile(&profile.id).unwrap();
        assert_eq!(
            detail
                .model_values
                .get("model")
                .map(|value| value.trim().trim_matches('"')),
            Some("glm-5.3-pro")
        );
        let stored = context.database.profile(&profile.id).unwrap();
        assert_eq!(
            stored
                .payload
                .model_values
                .get("model")
                .map(|value| value.trim().trim_matches('"')),
            Some("glm-5.3-pro")
        );
        assert!(stored
            .payload
            .raw_config
            .as_deref()
            .unwrap()
            .contains("model = \"glm-5.3-pro\""));

        // 再外部改一次，get_state（刷新按钮/窗口激活）也会同步
        write(
            r#"
model = "glm-5.4"
model_provider = "ZAI"
model_reasoning_effort = "low"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://new.example"
experimental_bearer_token = "secret"
"#,
        );
        let state = context.get_state().unwrap();
        let summary = state
            .profiles
            .iter()
            .find(|item| item.id == profile.id)
            .unwrap();
        assert_eq!(summary.model.as_deref(), Some("glm-5.4"));
        let stored = context.database.profile(&profile.id).unwrap();
        assert_eq!(
            stored
                .payload
                .model_values
                .get("model")
                .map(|value| value.trim().trim_matches('"')),
            Some("glm-5.4")
        );
    }

    #[test]
    fn show_balance_toggle_survives_live_sync() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(
            paths.codex_config(),
            "model = \"glm-5.3\"\nmodel_provider = \"ZAI\"\n\n[model_providers.ZAI]\nname = \"ZAI\"\nbase_url = \"https://api.example\"\nexperimental_bearer_token = \"secret\"\n",
        )
        .unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("ZAI").unwrap();
        assert!(!profile.show_balance); // 默认关闭

        context
            .set_profile_show_balance(&profile.id, false)
            .unwrap();

        // 外部改 live 后触发同步，供应商级开关不能被重置回默认值
        std::fs::write(
            context.paths.codex_config(),
            "model = \"glm-5.4\"\nmodel_provider = \"ZAI\"\nmodel_reasoning_effort = \"low\"\n\n[model_providers.ZAI]\nname = \"ZAI\"\nbase_url = \"https://new.example\"\nexperimental_bearer_token = \"secret\"\n",
        )
        .unwrap();
        let state = context.get_state().unwrap();
        let summary = state
            .profiles
            .iter()
            .find(|item| item.id == profile.id)
            .unwrap();
        assert!(!summary.show_balance);
        let stored = context.database.profile(&profile.id).unwrap();
        assert!(!stored.payload.show_balance);
    }

    #[test]
    fn adding_preset_does_not_activate() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        // 添加供应商是纯入库动作，绝不激活（只有手动应用/捕获才建立使用中）
        context
            .add_builtin_profile("deepseek", None, Some("sk-test"), None, None)
            .unwrap();
        let state = context.get_state().unwrap();
        assert_eq!(state.active_profile_id, None);
        assert_eq!(state.profiles.len(), 1);
    }

    #[test]
    fn export_and_restore_database_round_trip() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();

        let context = AppContext::new(paths.clone()).unwrap();
        let profile = context.capture_profile("A").unwrap();

        let exported = context.export_database().unwrap();
        assert!(exported.exists());
        let name = exported.file_name().unwrap().to_string_lossy().into_owned();
        assert!(context
            .list_database_backups()
            .unwrap()
            .iter()
            .any(|backup| backup.name == name));

        // 把当前库改乱，再从备份恢复
        context.database.delete_profile(&profile.id).unwrap();
        assert!(context.database.profiles().unwrap().is_empty());
        context.restore_database(&name).unwrap();
        assert_eq!(context.database.profiles().unwrap().len(), 1);

        // 非法文件名拒绝
        assert!(context.restore_database("../evil.db").is_err());
        assert!(context.delete_database_backup("..\\evil.db").is_err());
        assert!(context
            .restore_database("cgswitch-export-nothere.db")
            .is_err());
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
            .add_builtin_profile("deepseek", None, Some("sk-test"), None, None)
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
        // 关联文件按本供应商字节写入，旧文件已备份
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
            .add_builtin_profile("deepseek", None, Some("sk-old"), None, None)
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
                None,
            )
            .unwrap();

        // 使用中改密钥：只就地更新供应商段落，模板其余内容保持不变
        let config =
            String::from_utf8(std::fs::read(context.paths.codex_config()).unwrap()).unwrap();
        assert!(config.contains("model = \"deepseek-v4-flash\""));
        assert!(config.contains("experimental_bearer_token = \"sk-real\""));
        assert!(!config.contains("sk-old"));
        assert!(!config.contains("<你的 DeepSeek API Key>"));

        let detail = context.get_profile(&profile.id).unwrap();
        assert_eq!(detail.api_key.as_deref(), Some("sk-real"));
        // 所见即所得：编辑器直接展示真实密钥
        assert!(detail
            .config_fragment
            .contains(r#"experimental_bearer_token = "sk-real""#));
    }

    #[test]
    fn unused_builtin_edit_save_writes_db_only_without_key_prompt() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        let original = "model = \"other\"\n";
        std::fs::write(paths.codex_config(), original).unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("deepseek", None, Some("sk-test"), None, None)
            .unwrap();

        // 未使用：编辑保存只写库，不要求密钥占位符、不碰 live 配置
        let detail = context.get_profile(&profile.id).unwrap();
        let edited = detail.config_fragment.replace("sk-test", "sk-edited");
        context
            .update_profile(
                &profile.id,
                "DeepSeek",
                Some("https://api.deepseek.com/"),
                Some("sk-edited"),
                None,
            )
            .unwrap();
        let updated = context
            .update_profile_config(&profile.id, &edited, None, None)
            .unwrap();
        assert!(updated.raw_config.as_deref().unwrap().contains("sk-edited"));
        assert_eq!(
            std::fs::read_to_string(context.paths.codex_config()).unwrap(),
            original
        );
    }

    #[test]
    fn keyless_builtin_saves_to_db_but_apply_requires_key() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"other\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("deepseek", None, None, None, None)
            .unwrap();
        assert!(!profile.has_key);
        let detail = context.get_profile(&profile.id).unwrap();
        assert_eq!(detail.api_key.as_deref(), None);
        assert!(detail.config_fragment.contains("<你的 DeepSeek API Key>"));

        let error = context.apply_profile(&profile.id).unwrap_err();
        assert!(error.0.contains("尚未配置 API 密钥"));
        assert_eq!(
            std::fs::read_to_string(context.paths.codex_config()).unwrap(),
            "model = \"other\"\n"
        );
    }

    #[test]
    fn active_builtin_save_without_placeholder_keeps_edited_text() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"other\"\n").unwrap();

        let context = AppContext::new(paths).unwrap();
        let profile = context
            .add_builtin_profile("deepseek", None, Some("sk-test"), None, None)
            .unwrap();
        context.apply_profile(&profile.id).unwrap();

        // 编辑文本里用户已把占位符改成真实密钥：保存不再报“缺少密钥占位符”
        let edited = r#"
model = "deepseek-v4-flash"
model_provider = "deepseek"
preferred_auth_method = "apikey"
forced_login_method = "api"
model_reasoning_effort = "high"
model_catalog_json = "~/.codex/models.json"

[model_providers.deepseek]
name = "deepseek"
base_url = "https://api.deepseek.com/"
wire_api = "responses"
experimental_bearer_token = "sk-in-editor"
"#;
        context
            .update_profile_config(&profile.id, edited, None, None)
            .unwrap();
        let live = std::fs::read_to_string(context.paths.codex_config()).unwrap();
        assert!(live.contains("sk-in-editor"));
        assert!(!live.contains("<你的 DeepSeek API Key>"));
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
            .add_builtin_profile("deepseek", None, Some("sk-d"), None, None)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let zhipu = context
            .add_builtin_profile("zhipu", None, Some("sk-z"), None, None)
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
            .add_builtin_profile("minimax", None, Some("mm-key"), None, None)
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
        let profile = context
            .add_builtin_profile("chatgpt", None, None, None, None)
            .unwrap();
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
            ..Default::default()
        };
        let summary = context
            .database
            .insert_profile("DeepSeek 旧数据", &payload, &now_ms().to_string())
            .unwrap();

        let detail = context.get_profile(&summary.id).unwrap();
        assert_eq!(detail.api_key, None);
        assert!(detail.config_fragment.contains("<你的 DeepSeek API Key>"));
        let state = context.get_state().unwrap();
        let stored_summary = state
            .profiles
            .iter()
            .find(|item| item.id == summary.id)
            .unwrap();
        assert!(!stored_summary.has_key);
    }

    #[test]
    fn update_profile_config_saves_captured_fragment_as_structured() {
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

[model_providers.ZAI]
name = "ZAI"
base_url = "https://old.example"
experimental_bearer_token = "secret"
"#,
        )
        .unwrap();
        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("GLM").unwrap();

        let edited = r#"
model = "glm-5.5"
model_provider = "ZAI"
model_reasoning_effort = "medium"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://new.example"
experimental_bearer_token = "new-key"
"#;
        let detail = context
            .update_profile_config(&profile.id, edited, None, None)
            .unwrap();
        assert_eq!(detail.raw_config.as_deref(), Some(edited));
        assert_eq!(
            detail
                .model_values
                .get("model")
                .map(|v| v.trim().trim_matches('"')),
            Some("glm-5.5")
        );
        assert!(detail.config_fragment.contains("https://new.example"));

        context.apply_profile(&profile.id).unwrap();
        let live = std::fs::read_to_string(context.paths.codex_config()).unwrap();
        assert!(live.contains("glm-5.5"));
        assert!(live.contains("https://new.example"));
    }

    #[test]
    fn update_profile_config_rejects_missing_or_renamed_provider() {
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
"#,
        )
        .unwrap();
        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("GLM").unwrap();

        let missing = context
            .update_profile_config(&profile.id, "model = \"glm-5.3\"\n", None, None)
            .unwrap_err();
        assert!(missing.0.contains("model_providers.ZAI"));

        let renamed = context
            .update_profile_config(
                &profile.id,
                "model = \"glm-5.3\"\nmodel_provider = \"OTHER\"\n\n[model_providers.OTHER]\nname = \"OTHER\"\n",
                None,
                None,
            )
            .unwrap_err();
        assert!(renamed.0.contains("model_providers.ZAI"));
    }

    #[test]
    fn update_profile_config_builtin_raw_applies_with_key_and_catalog() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        let context = AppContext::new(paths).unwrap();
        std::fs::write(context.paths.codex_config(), "model = \"other\"\n").unwrap();
        let profile = context
            .add_builtin_profile("zhipu", None, Some("sk-test"), None, None)
            .unwrap();

        let edited = r#"
model = "glm-5.3"
model_provider = "ZAI"
model_reasoning_effort = "max"
model_catalog_json = "~/.codex/models.json"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://open.bigmodel.cn/api/v1"
experimental_bearer_token = "sk-test"
extra = "edited"
"#;
        let catalog = r#"{"models":[{"id":"glm-5.3","name":"GLM 5.3"}]}"#;
        let detail = context
            .update_profile_config(&profile.id, edited, Some(catalog), None)
            .unwrap();
        assert_eq!(detail.raw_config.as_deref(), Some(edited));

        context.apply_profile(&profile.id).unwrap();
        let live = std::fs::read_to_string(context.paths.codex_config()).unwrap();
        assert!(live.contains("extra = \"edited\""));
        assert!(live.contains(r#"experimental_bearer_token = "sk-test""#));
        assert_eq!(
            std::fs::read_to_string(context.paths.codex_home.join("models.json")).unwrap(),
            catalog
        );
    }

    #[test]
    fn autosync_preserves_profile_edited_catalog() {
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
model_catalog_json = "zai.json"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
"#,
        );
        let profile_a = context.capture_profile("A").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        write(
            r#"
model = "other-model"
model_provider = "ZAI"
model_catalog_json = "zai.json"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
"#,
        );
        let profile_b = context.capture_profile("B").unwrap();

        let detail_a = context.get_profile(&profile_a.id).unwrap();
        let catalog = r#"{"models":[{"id":"edited"}]}"#;
        context
            .update_profile_config(
                &profile_a.id,
                &detail_a.config_fragment,
                Some(catalog),
                None,
            )
            .unwrap();

        context.apply_profile(&profile_b.id).unwrap();
        let stored_a = context.database.profile(&profile_a.id).unwrap();
        assert_eq!(stored_a.payload.raw_catalog.as_deref(), Some(catalog));
    }

    #[test]
    fn update_profile_saves_and_clears_admin_url() {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        std::fs::create_dir_all(&paths.codex_home).unwrap();
        std::fs::write(paths.codex_config(), "model = \"glm-5.3\"\n").unwrap();
        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("GLM").unwrap();

        let summary = context
            .update_profile(
                &profile.id,
                "GLM",
                None,
                None,
                Some("https://console.example.com"),
            )
            .unwrap();
        assert_eq!(
            summary.admin_url.as_deref(),
            Some("https://console.example.com")
        );

        let invalid = context
            .update_profile(&profile.id, "GLM", None, None, Some("console.example.com"))
            .unwrap_err();
        assert!(invalid.0.contains("http"));

        context
            .update_profile(&profile.id, "GLM", None, None, Some(""))
            .unwrap();
        let detail = context.get_profile(&profile.id).unwrap();
        assert_eq!(detail.admin_url, None);
    }

    #[test]
    fn duplicate_profile_copies_payload_with_suffix() {
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

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.example"
"#,
        )
        .unwrap();
        let context = AppContext::new(paths).unwrap();
        let profile = context.capture_profile("GLM").unwrap();
        context
            .update_profile(
                &profile.id,
                "GLM",
                None,
                None,
                Some("https://console.example.com"),
            )
            .unwrap();
        context
            .set_profile_icon(&profile.id, Some("zhipu"))
            .unwrap();

        let dup = context.duplicate_profile(&profile.id).unwrap();
        assert_eq!(dup.name, "GLM 副本");
        assert_eq!(
            dup.admin_url.as_deref(),
            Some("https://console.example.com")
        );
        assert_eq!(dup.icon.as_deref(), Some("zhipu"));
        let original = context.database.profile(&profile.id).unwrap();
        let copied = context.database.profile(&dup.id).unwrap();
        assert_eq!(copied.payload, original.payload);

        std::thread::sleep(std::time::Duration::from_millis(2));
        let dup2 = context.duplicate_profile(&profile.id).unwrap();
        assert_eq!(dup2.name, "GLM 副本 2");
    }

    #[test]
    fn duplicate_profile_copies_live_auth_and_account_binding() {
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
base_url = "https://api.example"
"#,
        )
        .unwrap();
        std::fs::write(
            paths.codex_home.join("auth.json"),
            r#"{"OPENAI_API_KEY":"sk-live"}"#,
        )
        .unwrap();
        let context = AppContext::new(paths).unwrap();
        // 捕获的第三方供应商是使用中、快照无 auth；复制时应带上当前 live auth.json
        let profile = context.capture_profile("GLM").unwrap();
        assert!(context
            .database
            .profile(&profile.id)
            .unwrap()
            .payload
            .raw_auth
            .is_none());
        let dup = context.duplicate_profile(&profile.id).unwrap();
        assert_eq!(
            context
                .database
                .profile(&dup.id)
                .unwrap()
                .payload
                .raw_auth
                .as_deref(),
            Some(r#"{"OPENAI_API_KEY":"sk-live"}"#)
        );

        // 官方供应商：订阅账号绑定一并复制
        context
            .database
            .upsert_account(&crate::database::StoredAccount {
                id: "acc-1".into(),
                email: Some("a@example.com".into()),
                id_token: None,
                refresh_token: "rt".into(),
                authenticated_at: 1,
            })
            .unwrap();
        let official = context
            .add_builtin_profile("chatgpt", None, None, None, Some("acc-1"))
            .unwrap();
        let dup2 = context.duplicate_profile(&official.id).unwrap();
        assert_eq!(
            context
                .database
                .profile(&dup2.id)
                .unwrap()
                .account_id
                .as_deref(),
            Some("acc-1")
        );
    }
}
