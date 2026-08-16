use tauri::{AppHandle, State};

use crate::auth::codex_oauth::{
    AuthStatus, CodexOAuthError, CodexOAuthState, DeviceCodeResponse, ManagedAccount,
};
use crate::error::{app_err, AppResult};
use crate::models::{AppState, CodexAppStatus, ProfileDetail, ProfileSummary, Settings};
use crate::services::{AppContext, ProfileConnectionResult};

#[tauri::command]
pub fn get_state(state: State<'_, AppContext>) -> AppResult<AppState> {
    state.get_state()
}

#[tauri::command]
pub fn get_codex_status(state: State<'_, AppContext>) -> AppResult<CodexAppStatus> {
    state.codex_status()
}

#[tauri::command]
pub fn capture_profile(name: String, state: State<'_, AppContext>) -> AppResult<ProfileSummary> {
    state.capture_profile(&name)
}

#[tauri::command]
pub fn add_builtin_profile(
    kind: String,
    base_url: Option<String>,
    api_key: Option<String>,
    state: State<'_, AppContext>,
) -> AppResult<ProfileSummary> {
    state.add_builtin_profile(&kind, base_url.as_deref(), api_key.as_deref())
}

#[tauri::command]
pub fn get_builtin_catalog(
    kind: String,
    state: State<'_, AppContext>,
) -> AppResult<Option<String>> {
    state.get_builtin_catalog(&kind)
}

#[tauri::command]
pub async fn test_profile_connection(
    id: String,
    state: State<'_, AppContext>,
) -> AppResult<ProfileConnectionResult> {
    state.test_profile_connection(&id).await
}

#[tauri::command]
pub fn rename_profile(id: String, name: String, state: State<'_, AppContext>) -> AppResult<()> {
    state.rename_profile(&id, &name)
}

#[tauri::command]
pub fn set_profile_icon(
    id: String,
    icon: Option<String>,
    state: State<'_, AppContext>,
) -> AppResult<()> {
    state.set_profile_icon(&id, icon.as_deref())
}

#[tauri::command]
pub fn get_profile(id: String, state: State<'_, AppContext>) -> AppResult<ProfileDetail> {
    state.get_profile(&id)
}

#[tauri::command]
pub fn update_profile(
    id: String,
    name: String,
    base_url: Option<String>,
    api_key: Option<String>,
    state: State<'_, AppContext>,
) -> AppResult<ProfileSummary> {
    state.update_profile(&id, &name, base_url.as_deref(), api_key.as_deref())
}

#[tauri::command]
pub fn delete_profile(id: String, state: State<'_, AppContext>) -> AppResult<()> {
    state.delete_profile(&id)
}

#[tauri::command]
pub fn apply_profile(id: String, state: State<'_, AppContext>) -> AppResult<()> {
    state.apply_profile(&id)
}

#[tauri::command]
pub fn restart_codex(app: AppHandle, state: State<'_, AppContext>) -> AppResult<()> {
    state.restart_codex(&app)
}

#[tauri::command]
pub fn set_window_theme(dark: bool, app: AppHandle) -> AppResult<()> {
    #[cfg(not(windows))]
    {
        let _ = (dark, app);
    }
    #[cfg(windows)]
    {
        use crate::error::app_err;
        use std::ffi::c_void;
        use tauri::Manager; // get_webview_window
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_NCACTIVATE};

        if let Some(window) = app.get_webview_window("main") {
            let hwnd = window
                .hwnd()
                .map_err(|error| app_err!("无法获取窗口句柄: {error}"))?;
            let ours = HWND(hwnd.0);
            let value = i32::from(dark);
            let raw: *const c_void = &value as *const i32 as *const c_void;
            unsafe {
                DwmSetWindowAttribute(
                    ours,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    raw,
                    std::mem::size_of::<i32>() as u32,
                )
            }
            .map_err(|error| app_err!("无法设置窗口标题栏主题: {error}"))?;
            // 强制立即重绘标题栏，避免 DWM 延迟刷新导致与内容主题切换不同步
            unsafe {
                SendMessageW(ours, WM_NCACTIVATE, Some(WPARAM(0)), Some(LPARAM(0)));
                SendMessageW(ours, WM_NCACTIVATE, Some(WPARAM(1)), Some(LPARAM(0)));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn auth_start_login(
    state: State<'_, CodexOAuthState>,
) -> Result<DeviceCodeResponse, String> {
    state
        .0
        .read()
        .await
        .start_device_flow()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn auth_poll_for_account(
    device_code: String,
    state: State<'_, CodexOAuthState>,
) -> Result<Option<ManagedAccount>, String> {
    match state.0.write().await.poll_for_token(&device_code).await {
        Ok(account) => Ok(account),
        Err(CodexOAuthError::AuthorizationPending) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub async fn auth_get_status(state: State<'_, CodexOAuthState>) -> Result<AuthStatus, String> {
    Ok(state.0.read().await.get_status().await)
}

#[tauri::command]
pub async fn auth_remove_account(
    account_id: String,
    state: State<'_, CodexOAuthState>,
) -> Result<(), String> {
    state
        .0
        .write()
        .await
        .remove_account(&account_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_url(url: String) -> AppResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(app_err!("仅支持打开 http(s) 链接"));
    }
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let url_wide = HSTRING::from(&url);
        let operation = HSTRING::from("open");
        let result =
            unsafe { ShellExecuteW(None, &operation, &url_wide, None, None, SW_SHOWNORMAL) };
        if result.0 as usize <= 32 {
            return Err(app_err!("无法打开系统浏览器"));
        }
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppContext>) -> AppResult<Settings> {
    state.settings()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    settings: Settings,
    state: State<'_, AppContext>,
) -> AppResult<Settings> {
    let saved = state.save_settings(&settings)?;
    sync_autostart(&app, &saved)?;
    Ok(saved)
}

fn sync_autostart(app: &AppHandle, settings: &Settings) -> AppResult<()> {
    use tauri_plugin_autostart::ManagerExt;
    if settings.autostart_enabled {
        app.autolaunch()
            .enable()
            .map_err(|error| app_err!("同步开机自启设置失败: {error}"))
    } else {
        let _ = app.autolaunch().disable();
        Ok(())
    }
}

#[tauri::command]
pub fn open_path(path: String, state: State<'_, AppContext>) -> AppResult<()> {
    state.open_path(&path)
}

#[tauri::command]
pub fn open_codex_file(relative: String, state: State<'_, AppContext>) -> AppResult<()> {
    state.open_codex_file(&relative)
}
