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

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{Manager, WindowEvent};

use crate::services::AppContext;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let paths = paths::app_paths().expect("无法定位用户数据目录");
    let context = AppContext::new(paths.clone()).expect("无法初始化 SwitchGPT 数据库");
    let oauth_state = auth::CodexOAuthState(Arc::new(tokio::sync::RwLock::new(
        auth::codex_oauth::CodexOAuthManager::new(paths.root.join("codex_oauth_auth.json")),
    )));

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::Builder::new().build())
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
            commands::open_url,
            commands::get_settings,
            commands::save_settings,
            commands::open_path,
        ])
        .setup(|app| {
            use tauri_plugin_autostart::ManagerExt;

            let settings = app.state::<AppContext>().settings()?;
            if settings.autostart_enabled {
                if let Err(error) = app.autolaunch().enable() {
                    eprintln!("同步开机自启设置失败: {error}");
                }
            }

            let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出 SwitchGPT", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("缺少应用图标").clone())
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            if settings.silent_start {
                if let Some(window) = app.get_webview_window("main") {
                    window.hide()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let minimize_to_tray = window
                    .app_handle()
                    .state::<AppContext>()
                    .settings()
                    .map(|settings| settings.minimize_to_tray)
                    .unwrap_or(false);
                if minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
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
