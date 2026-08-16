//! ChatGPT 官方订阅的 OAuth Device Code 认证（对齐官方 Codex CLI 的登录流程）。
//!
//! 1. 向 OpenAI 申请设备码，展示 user_code 与验证网址，用户在浏览器完成授权；
//! 2. 轮询获取 authorization_code + code_verifier，再换取
//!    access_token / refresh_token / id_token；
//! 3. 账号持久化（只存 refresh_token 与账号标识），access_token 内存缓存、到期前自动刷新。
//!
//! 认证一次后账号常驻，后续添加 ChatGPT 供应商无需重复认证。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::database::{Database, StoredAccount};
use crate::error::AppResult;

/// OpenAI OAuth 客户端 ID（与官方 Codex CLI 相同）
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DEVICE_AUTH_USERCODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_AUTH_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";
const TOKEN_REFRESH_BUFFER_MS: i64 = 60_000;
const DEVICE_CODE_DEFAULT_EXPIRES_IN: u64 = 900;
const POLLING_SAFETY_MARGIN_SECS: u64 = 3;
const CODEX_USER_AGENT: &str = "cgswitch-codex-oauth";

#[derive(Debug, thiserror::Error)]
pub enum CodexOAuthError {
    #[error("等待用户授权中")]
    AuthorizationPending,
    #[error("用户拒绝授权")]
    AccessDenied,
    #[error("设备码已过期")]
    ExpiredToken,
    #[error("OAuth 请求失败: {0}")]
    RequestFailed(String),
    #[error("Refresh Token 失效或已过期")]
    RefreshTokenInvalid,
    #[error("网络错误: {0}")]
    NetworkError(String),
    #[error("解析错误: {0}")]
    ParseError(String),
    #[error("账号不存在: {0}")]
    AccountNotFound(String),
    #[error("IO 错误: {0}")]
    IoError(String),
}

impl From<reqwest::Error> for CodexOAuthError {
    fn from(error: reqwest::Error) -> Self {
        CodexOAuthError::NetworkError(error.to_string())
    }
}

/// 返回给前端的设备码信息
#[derive(Debug, Clone, Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// 已认证账号摘要
#[derive(Debug, Clone, Serialize)]
pub struct ManagedAccount {
    pub id: String,
    pub login: String,
    pub authenticated_at: i64,
    pub is_default: bool,
}

/// 认证状态摘要
#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub default_account_id: Option<String>,
    pub accounts: Vec<ManagedAccount>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawDeviceCodeResponse {
    device_auth_id: String,
    user_code: String,
    #[serde(default)]
    interval: Option<serde_json::Value>,
    #[serde(default)]
    expires_in: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawDevicePollSuccess {
    authorization_code: String,
    code_verifier: String,
}

#[derive(Debug, Clone, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct IdTokenClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    organizations: Vec<OrgClaim>,
    #[serde(default, rename = "https://api.openai.com/auth")]
    openai_auth: Option<OpenAiAuthClaim>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct OrgClaim {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct OpenAiAuthClaim {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

/// 内存缓存的 access_token
#[derive(Debug, Clone)]
struct CachedAccessToken {
    token: String,
    expires_at_ms: i64,
}

impl CachedAccessToken {
    fn is_expiring_soon(&self) -> bool {
        self.expires_at_ms - now_ms() < TOKEN_REFRESH_BUFFER_MS
    }
}

/// 进行中的设备码流程
#[derive(Debug, Clone)]
struct PendingDeviceCode {
    user_code: String,
    expires_at_ms: i64,
}

/// 持久化的账号数据（只存 refresh_token，access_token 不落盘）
#[derive(Debug, Clone)]
struct CodexAccountData {
    account_id: String,
    email: Option<String>,
    id_token: Option<String>,
    refresh_token: String,
    authenticated_at: i64,
}

impl From<StoredAccount> for CodexAccountData {
    fn from(account: StoredAccount) -> Self {
        Self {
            account_id: account.id,
            email: account.email,
            id_token: account.id_token,
            refresh_token: account.refresh_token,
            authenticated_at: account.authenticated_at,
        }
    }
}

/// 多账号认证管理器
pub struct CodexOAuthManager {
    client: reqwest::Client,
    accounts: Arc<RwLock<HashMap<String, CodexAccountData>>>,
    default_account_id: Arc<RwLock<Option<String>>>,
    access_tokens: Arc<RwLock<HashMap<String, CachedAccessToken>>>,
    refresh_locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
    pending_device_codes: Arc<RwLock<HashMap<String, PendingDeviceCode>>>,
    database: Arc<Database>,
}

impl CodexOAuthManager {
    pub fn new(database: Arc<Database>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(CODEX_USER_AGENT)
            .build()
            .expect("创建 HTTP 客户端失败");
        let manager = Self {
            client,
            accounts: Arc::new(RwLock::new(HashMap::new())),
            default_account_id: Arc::new(RwLock::new(None)),
            access_tokens: Arc::new(RwLock::new(HashMap::new())),
            refresh_locks: Arc::new(RwLock::new(HashMap::new())),
            pending_device_codes: Arc::new(RwLock::new(HashMap::new())),
            database,
        };
        if let Err(error) = manager.load_accounts() {
            eprintln!("[auth] 加载认证账号失败: {error}");
        }
        manager
    }

    /// 数据库恢复/导入后，重新从 SQLite 加载账号（不再触发旧 JSON 导入）。
    pub fn reload_from_database(&self) -> AppResult<()> {
        self.load_accounts()
    }

    // ==================== 设备码流程 ====================

    /// 启动设备码流程，返回需要展示给用户的 user_code 与验证网址
    pub async fn start_device_flow(&self) -> Result<DeviceCodeResponse, CodexOAuthError> {
        let response = self
            .client
            .post(DEVICE_AUTH_USERCODE_URL)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "client_id": CODEX_CLIENT_ID }))
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::RequestFailed(format!(
                "设备码请求失败: {status} - {text}"
            )));
        }
        let device: RawDeviceCodeResponse = response
            .json()
            .await
            .map_err(|error| CodexOAuthError::ParseError(error.to_string()))?;

        let interval = parse_interval(device.interval.as_ref());
        let expires_in = device.expires_in.unwrap_or(DEVICE_CODE_DEFAULT_EXPIRES_IN);
        let expires_at_ms = now_ms() + expires_in as i64 * 1000;

        {
            let mut pending = self.pending_device_codes.write().await;
            let now = now_ms();
            pending.retain(|_, entry| entry.expires_at_ms > now);
            pending.insert(
                device.device_auth_id.clone(),
                PendingDeviceCode {
                    user_code: device.user_code.clone(),
                    expires_at_ms,
                },
            );
        }

        Ok(DeviceCodeResponse {
            device_code: device.device_auth_id,
            user_code: device.user_code,
            verification_uri: DEVICE_VERIFICATION_URL.to_string(),
            expires_in,
            interval,
        })
    }

    /// 轮询设备码状态，用户尚未授权时返回 `Ok(None)`
    pub async fn poll_for_token(
        &self,
        device_code: &str,
    ) -> Result<Option<ManagedAccount>, CodexOAuthError> {
        let entry = self
            .pending_device_codes
            .read()
            .await
            .get(device_code)
            .cloned()
            .ok_or_else(|| {
                CodexOAuthError::RequestFailed("未找到对应的用户码，请重新启动登录流程".to_string())
            })?;
        if entry.expires_at_ms <= now_ms() {
            self.pending_device_codes.write().await.remove(device_code);
            return Err(CodexOAuthError::ExpiredToken);
        }

        let response = self
            .client
            .post(DEVICE_AUTH_TOKEN_URL)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "device_auth_id": device_code,
                "user_code": entry.user_code,
            }))
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
            return Err(CodexOAuthError::AuthorizationPending);
        }
        if status == reqwest::StatusCode::GONE {
            self.pending_device_codes.write().await.remove(device_code);
            return Err(CodexOAuthError::ExpiredToken);
        }
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::RequestFailed(format!(
                "设备码轮询失败: {status} - {text}"
            )));
        }

        let success: RawDevicePollSuccess = response
            .json()
            .await
            .map_err(|error| CodexOAuthError::ParseError(error.to_string()))?;
        let tokens = self
            .exchange_code_for_tokens(&success.authorization_code, &success.code_verifier)
            .await?;
        self.pending_device_codes.write().await.remove(device_code);

        let refresh_token = tokens
            .refresh_token
            .clone()
            .ok_or_else(|| CodexOAuthError::RequestFailed("响应缺少 refresh_token".to_string()))?;
        let (account_id, email) = extract_identity_from_tokens(&tokens);
        let account_id = account_id.ok_or_else(|| {
            CodexOAuthError::ParseError("无法从 token 中提取账号标识".to_string())
        })?;

        self.access_tokens.write().await.insert(
            account_id.clone(),
            CachedAccessToken {
                token: tokens.access_token.clone(),
                expires_at_ms: compute_expires_at_ms(tokens.expires_in),
            },
        );

        let account = self
            .add_account_internal(account_id, refresh_token, email, tokens.id_token.clone())
            .await?;
        Ok(Some(account))
    }

    async fn exchange_code_for_tokens(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthTokenResponse, CodexOAuthError> {
        let response = self
            .client
            .post(OAUTH_TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", DEVICE_REDIRECT_URI),
                ("client_id", CODEX_CLIENT_ID),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::RequestFailed(format!(
                "换取 Token 失败: {status} - {text}"
            )));
        }
        response
            .json()
            .await
            .map_err(|error| CodexOAuthError::ParseError(error.to_string()))
    }

    async fn refresh_with_token(
        &self,
        refresh_token: &str,
    ) -> Result<OAuthTokenResponse, CodexOAuthError> {
        let response = self
            .client
            .post(OAUTH_TOKEN_URL)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id", CODEX_CLIENT_ID),
                ("scope", "openid profile email"),
            ])
            .send()
            .await?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(CodexOAuthError::RefreshTokenInvalid);
        }
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(CodexOAuthError::RequestFailed(format!(
                "刷新 Token 失败: {status} - {text}"
            )));
        }
        response
            .json()
            .await
            .map_err(|error| CodexOAuthError::ParseError(error.to_string()))
    }

    // ==================== Token 获取（含自动刷新） ====================

    /// 获取账号的有效 access_token，临近过期时自动刷新
    pub async fn get_valid_token_for_account(
        &self,
        account_id: &str,
    ) -> Result<String, CodexOAuthError> {
        if let Some(cached) = self.access_tokens.read().await.get(account_id) {
            if !cached.is_expiring_soon() {
                return Ok(cached.token.clone());
            }
        }

        let refresh_lock = self.get_refresh_lock(account_id).await;
        let _guard = refresh_lock.lock().await;

        if let Some(cached) = self.access_tokens.read().await.get(account_id) {
            if !cached.is_expiring_soon() {
                return Ok(cached.token.clone());
            }
        }

        let refresh_token = {
            let accounts = self.accounts.read().await;
            accounts
                .get(account_id)
                .map(|account| account.refresh_token.clone())
                .ok_or_else(|| CodexOAuthError::AccountNotFound(account_id.to_string()))?
        };
        let new_tokens = self.refresh_with_token(&refresh_token).await?;

        let new_refresh = new_tokens.refresh_token.clone();
        let new_id_token = new_tokens.id_token.clone();
        if new_refresh.is_some() || new_id_token.is_some() {
            let mut accounts = self.accounts.write().await;
            if let Some(account) = accounts.get_mut(account_id) {
                let mut changed = false;
                if let Some(token) = new_refresh {
                    if account.refresh_token != token {
                        account.refresh_token = token;
                        changed = true;
                    }
                }
                if let Some(token) = new_id_token {
                    if account.id_token.as_deref() != Some(token.as_str()) {
                        account.id_token = Some(token);
                        changed = true;
                    }
                }
                if changed {
                    self.save_account(account)?;
                }
            }
        }

        self.access_tokens.write().await.insert(
            account_id.to_string(),
            CachedAccessToken {
                token: new_tokens.access_token.clone(),
                expires_at_ms: compute_expires_at_ms(new_tokens.expires_in),
            },
        );
        Ok(new_tokens.access_token)
    }

    /// 生成官方 Codex CLI 的 auth.json 内容（ChatGPT 订阅登录格式）。
    pub async fn codex_auth_json(&self, account_id: &str) -> Result<String, CodexOAuthError> {
        let access_token = self.get_valid_token_for_account(account_id).await?;
        let (refresh_token, id_token) = {
            let accounts = self.accounts.read().await;
            let account = accounts
                .get(account_id)
                .ok_or_else(|| CodexOAuthError::AccountNotFound(account_id.to_string()))?;
            (account.refresh_token.clone(), account.id_token.clone())
        };
        let id_token = id_token.ok_or_else(|| {
            CodexOAuthError::RequestFailed("账号缺少 id_token，请重新登录".to_string())
        })?;
        let auth = serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": id_token,
                "access_token": access_token,
                "refresh_token": refresh_token,
                "account_id": account_id,
            },
            "last_refresh": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        });
        serde_json::to_string_pretty(&auth)
            .map_err(|error| CodexOAuthError::ParseError(error.to_string()))
    }

    // ==================== 账号管理 ====================

    pub async fn list_accounts(&self) -> Vec<ManagedAccount> {
        let accounts = self.accounts.read().await.clone();
        let default_id = self.resolve_default_account_id().await;
        sorted_accounts(&accounts, default_id.as_deref())
    }

    pub async fn get_status(&self) -> AuthStatus {
        let accounts = self.accounts.read().await.clone();
        let default_id = self.resolve_default_account_id().await;
        AuthStatus {
            authenticated: !accounts.is_empty(),
            default_account_id: default_id.clone(),
            accounts: sorted_accounts(&accounts, default_id.as_deref()),
        }
    }

    pub async fn default_account_id(&self) -> Option<String> {
        self.resolve_default_account_id().await
    }

    /// 切换当前订阅账号（用于多账号管理）。
    pub async fn set_default_account(&self, account_id: &str) -> Result<(), CodexOAuthError> {
        {
            let accounts = self.accounts.read().await;
            if !accounts.contains_key(account_id) {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
        }
        {
            let mut default = self.default_account_id.write().await;
            if default.as_deref() == Some(account_id) {
                return Ok(());
            }
            *default = Some(account_id.to_string());
        }
        self.save_default_account(Some(account_id))
    }

    pub async fn remove_account(&self, account_id: &str) -> Result<(), CodexOAuthError> {
        {
            let mut accounts = self.accounts.write().await;
            if accounts.remove(account_id).is_none() {
                return Err(CodexOAuthError::AccountNotFound(account_id.to_string()));
            }
        }
        self.database
            .delete_account(account_id)
            .map_err(|error| CodexOAuthError::IoError(error.to_string()))?;
        self.access_tokens.write().await.remove(account_id);
        self.refresh_locks.write().await.remove(account_id);
        {
            let accounts = self.accounts.read().await;
            let mut default = self.default_account_id.write().await;
            if default.as_deref() == Some(account_id) {
                *default = fallback_default_account_id(&accounts);
            }
        }
        let default = self.default_account_id.read().await.clone();
        self.save_default_account(default.as_deref())
    }

    pub async fn is_authenticated(&self) -> bool {
        !self.accounts.read().await.is_empty()
    }

    // ==================== 内部方法 ====================

    async fn add_account_internal(
        &self,
        account_id: String,
        refresh_token: String,
        email: Option<String>,
        id_token: Option<String>,
    ) -> Result<ManagedAccount, CodexOAuthError> {
        let data = CodexAccountData {
            account_id: account_id.clone(),
            email,
            id_token,
            refresh_token,
            authenticated_at: now_secs(),
        };
        self.save_account(&data)?;
        {
            let mut accounts = self.accounts.write().await;
            accounts.insert(account_id.clone(), data);
        }
        {
            let mut default = self.default_account_id.write().await;
            *default = Some(account_id.clone());
        }
        self.save_default_account(Some(account_id.as_str()))?;
        Ok(ManagedAccount {
            id: account_id.clone(),
            login: display_login(
                &account_id,
                self.accounts
                    .read()
                    .await
                    .get(&account_id)
                    .and_then(|a| a.email.clone()),
            ),
            authenticated_at: now_secs(),
            is_default: self.default_account_id.read().await.as_deref()
                == Some(account_id.as_str()),
        })
    }

    async fn resolve_default_account_id(&self) -> Option<String> {
        let stored = self.default_account_id.read().await.clone();
        let accounts = self.accounts.read().await;
        if let Some(id) = stored {
            if accounts.contains_key(&id) {
                return Some(id);
            }
        }
        fallback_default_account_id(&accounts)
    }

    async fn get_refresh_lock(&self, account_id: &str) -> Arc<Mutex<()>> {
        if let Some(lock) = self.refresh_locks.read().await.get(account_id) {
            return Arc::clone(lock);
        }
        Arc::clone(
            self.refresh_locks
                .write()
                .await
                .entry(account_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    fn save_account(&self, account: &CodexAccountData) -> Result<(), CodexOAuthError> {
        self.database
            .upsert_account(&StoredAccount {
                id: account.account_id.clone(),
                email: account.email.clone(),
                id_token: account.id_token.clone(),
                refresh_token: account.refresh_token.clone(),
                authenticated_at: account.authenticated_at,
            })
            .map_err(|error| CodexOAuthError::IoError(error.to_string()))
    }

    fn save_default_account(&self, id: Option<&str>) -> Result<(), CodexOAuthError> {
        self.database
            .set_default_account(id)
            .map_err(|error| CodexOAuthError::IoError(error.to_string()))
    }

    fn load_accounts(&self) -> AppResult<()> {
        let stored = self.database.accounts()?;
        let accounts: HashMap<String, CodexAccountData> = stored
            .into_iter()
            .map(|account| (account.id.clone(), account.into()))
            .collect();
        let default = self
            .database
            .app_state()?
            .1
            .or_else(|| fallback_default_account_id(&accounts));
        if let Ok(mut slot) = self.accounts.try_write() {
            *slot = accounts;
        }
        if let Ok(mut slot) = self.default_account_id.try_write() {
            *slot = default;
        }
        Ok(())
    }
}

/// Tauri 托管状态
pub struct CodexOAuthState(pub Arc<RwLock<CodexOAuthManager>>);

fn sorted_accounts(
    accounts: &HashMap<String, CodexAccountData>,
    default_account_id: Option<&str>,
) -> Vec<ManagedAccount> {
    let mut list: Vec<ManagedAccount> = accounts
        .iter()
        .map(|(id, data)| ManagedAccount {
            id: id.clone(),
            login: display_login(id, data.email.clone()),
            authenticated_at: data.authenticated_at,
            is_default: default_account_id == Some(id.as_str()),
        })
        .collect();
    list.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| b.authenticated_at.cmp(&a.authenticated_at))
            .then_with(|| a.login.cmp(&b.login))
    });
    list
}

fn display_login(account_id: &str, email: Option<String>) -> String {
    email.unwrap_or_else(|| format!("ChatGPT ({account_id})"))
}

fn fallback_default_account_id(accounts: &HashMap<String, CodexAccountData>) -> Option<String> {
    accounts
        .iter()
        .max_by(|(id_a, a), (id_b, b)| {
            a.authenticated_at
                .cmp(&b.authenticated_at)
                .then_with(|| id_b.cmp(id_a))
        })
        .map(|(id, _)| id.clone())
}

fn parse_interval(value: Option<&serde_json::Value>) -> u64 {
    let raw = match value {
        Some(serde_json::Value::Number(number)) => number.as_u64().unwrap_or(5),
        Some(serde_json::Value::String(text)) => text.parse::<u64>().unwrap_or(5),
        _ => 5,
    };
    raw.max(1) + POLLING_SAFETY_MARGIN_SECS
}

fn compute_expires_at_ms(expires_in: Option<i64>) -> i64 {
    now_ms() + expires_in.unwrap_or(3600) * 1000
}

fn parse_jwt_claims(token: &str) -> Option<IdTokenClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let decoded = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn extract_identity_from_tokens(tokens: &OAuthTokenResponse) -> (Option<String>, Option<String>) {
    let mut account_id: Option<String> = None;
    let mut email: Option<String> = None;

    if let Some(id_token) = tokens.id_token.as_deref() {
        if let Some(claims) = parse_jwt_claims(id_token) {
            account_id = claims
                .chatgpt_account_id
                .clone()
                .or_else(|| {
                    claims
                        .openai_auth
                        .as_ref()
                        .and_then(|auth| auth.chatgpt_account_id.clone())
                })
                .or_else(|| claims.organizations.first().and_then(|org| org.id.clone()));
            email = claims.email.clone();
        }
    }

    if account_id.is_none() {
        if let Some(claims) = parse_jwt_claims(&tokens.access_token) {
            account_id = claims
                .chatgpt_account_id
                .clone()
                .or_else(|| {
                    claims
                        .openai_auth
                        .as_ref()
                        .and_then(|auth| auth.chatgpt_account_id.clone())
                })
                .or_else(|| claims.organizations.first().and_then(|org| org.id.clone()));
            if email.is_none() {
                email = claims.email.clone();
            }
        }
    }

    (account_id, email)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(dir: &std::path::Path) -> Arc<Database> {
        let paths = crate::paths::from_home(dir).unwrap();
        Arc::new(Database::open(&paths).unwrap())
    }

    #[test]
    fn parse_interval_handles_number_string_and_default() {
        assert_eq!(
            parse_interval(Some(&serde_json::json!(5))),
            5 + POLLING_SAFETY_MARGIN_SECS
        );
        assert_eq!(
            parse_interval(Some(&serde_json::json!("10"))),
            10 + POLLING_SAFETY_MARGIN_SECS
        );
        assert_eq!(parse_interval(None), 5 + POLLING_SAFETY_MARGIN_SECS);
        assert_eq!(
            parse_interval(Some(&serde_json::json!(0))),
            1 + POLLING_SAFETY_MARGIN_SECS
        );
    }

    #[test]
    fn parse_jwt_claims_extracts_account_and_email() {
        let header = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\"}");
        let payload = URL_SAFE_NO_PAD
            .encode(b"{\"chatgpt_account_id\":\"acc-123\",\"email\":\"test@example.com\"}");
        let claims = parse_jwt_claims(&format!("{header}.{payload}.")).unwrap();
        assert_eq!(claims.chatgpt_account_id.as_deref(), Some("acc-123"));
        assert_eq!(claims.email.as_deref(), Some("test@example.com"));
    }

    #[test]
    fn parse_jwt_claims_rejects_malformed() {
        assert!(parse_jwt_claims("not-a-jwt").is_none());
        assert!(parse_jwt_claims("only.two").is_none());
    }

    #[test]
    fn cached_token_expiry_window() {
        let now = now_ms();
        assert!(CachedAccessToken {
            token: "t".into(),
            expires_at_ms: now + 30_000,
        }
        .is_expiring_soon());
        assert!(!CachedAccessToken {
            token: "t".into(),
            expires_at_ms: now + 3_600_000,
        }
        .is_expiring_soon());
    }

    #[tokio::test]
    async fn manager_persists_accounts_to_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let database = setup(dir.path());
        {
            let manager = CodexOAuthManager::new(database.clone());
            manager
                .add_account_internal(
                    "acc-123".to_string(),
                    "rt-secret".to_string(),
                    Some("user@example.com".to_string()),
                    Some("id-jwt".to_string()),
                )
                .await
                .unwrap();
        }
        let manager = CodexOAuthManager::new(database);
        let status = manager.get_status().await;
        assert_eq!(status.accounts.len(), 1);
        assert_eq!(status.accounts[0].id, "acc-123");
        assert_eq!(status.accounts[0].login, "user@example.com");
        assert!(status.accounts[0].is_default);
    }

    #[tokio::test]
    async fn manager_remove_account_updates_default() {
        let dir = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(setup(dir.path()));
        manager
            .add_account_internal(
                "acc-123".to_string(),
                "rt".to_string(),
                Some("a@example.com".to_string()),
                None,
            )
            .await
            .unwrap();
        manager
            .add_account_internal(
                "acc-456".to_string(),
                "rt2".to_string(),
                Some("b@example.com".to_string()),
                None,
            )
            .await
            .unwrap();

        manager.remove_account("acc-123").await.unwrap();
        let accounts = manager.list_accounts().await;
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "acc-456");
    }

    #[tokio::test]
    async fn codex_auth_json_matches_official_chatgpt_shape() {
        let dir = tempfile::tempdir().unwrap();
        let manager = CodexOAuthManager::new(setup(dir.path()));
        manager
            .add_account_internal(
                "acc-1".to_string(),
                "rt-1".to_string(),
                Some("a@example.com".to_string()),
                Some("id-jwt".to_string()),
            )
            .await
            .unwrap();
        manager.access_tokens.write().await.insert(
            "acc-1".to_string(),
            CachedAccessToken {
                token: "at-1".to_string(),
                expires_at_ms: now_ms() + 3_600_000,
            },
        );

        let json = manager.codex_auth_json("acc-1").await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["auth_mode"], "chatgpt");
        assert!(value["OPENAI_API_KEY"].is_null());
        assert_eq!(value["tokens"]["id_token"], "id-jwt");
        assert_eq!(value["tokens"]["access_token"], "at-1");
        assert_eq!(value["tokens"]["refresh_token"], "rt-1");
        assert_eq!(value["tokens"]["account_id"], "acc-1");
        assert!(value["last_refresh"].is_string());
    }
}
