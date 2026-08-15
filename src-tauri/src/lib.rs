pub mod auth;
pub mod codex;
pub mod commands;
pub mod database;
pub mod error;
pub mod fsutil;
pub mod models;
pub mod paths;
pub mod services;

use std::sync::Arc;

use crate::services::AppContext;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = paths::app_paths().expect("无法定位用户数据目录");
    let context = AppContext::new(paths.clone()).expect("无法初始化 SwitchGPT 数据库");
    let oauth_state = auth::CodexOAuthState(Arc::new(tokio::sync::RwLock::new(
        auth::codex_oauth::CodexOAuthManager::new(paths.root.join("codex_oauth_auth.json")),
    )));

    tauri::Builder::default()
        .manage(context)
        .manage(oauth_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::capture_profile,
            commands::rename_profile,
            commands::set_profile_icon,
            commands::get_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::apply_profile,
            commands::restart_codex,
            commands::set_window_theme,
            commands::auth_start_login,
            commands::auth_poll_for_account,
            commands::auth_get_status,
            commands::auth_remove_account,
            commands::get_settings,
            commands::save_settings,
            commands::open_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SwitchGPT");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppResult;

    #[test]
    fn service_context_initializes_empty_database() -> AppResult<()> {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths::from_home(dir.path())?;
        let context = AppContext::new(paths)?;
        let state = context.get_state()?;
        assert!(state.profiles.is_empty());
        assert!(state.active_profile_id.is_none());
        Ok(())
    }
}
