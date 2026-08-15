use tauri::{AppHandle, State};

use crate::error::AppResult;
use crate::models::{AppState, ProfileSummary, Settings};
use crate::services::AppContext;

#[tauri::command]
pub fn get_state(state: State<'_, AppContext>) -> AppResult<AppState> {
    state.get_state()
}

#[tauri::command]
pub fn capture_profile(name: String, state: State<'_, AppContext>) -> AppResult<ProfileSummary> {
    state.capture_profile(&name)
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
pub fn get_settings(state: State<'_, AppContext>) -> AppResult<Settings> {
    state.settings()
}

#[tauri::command]
pub fn save_settings(settings: Settings, state: State<'_, AppContext>) -> AppResult<Settings> {
    state.save_settings(&settings)
}

#[tauri::command]
pub fn open_path(path: String, state: State<'_, AppContext>) -> AppResult<()> {
    state.open_path(&path)
}
