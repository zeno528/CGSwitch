use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension, Row};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};

use crate::error::{app_err, AppResult};
use crate::models::{ProfilePayload, ProfileSummary, Settings};
use crate::paths::AppPaths;

const SCHEMA_V1: &str = r#"
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE switch_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  profile_id TEXT,
  action TEXT NOT NULL,
  status TEXT NOT NULL,
  message TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(profile_id) REFERENCES profiles(id) ON DELETE SET NULL
);

CREATE INDEX idx_switch_events_created_at ON switch_events(created_at DESC);
CREATE INDEX idx_switch_events_profile_id ON switch_events(profile_id);
"#;

const SCHEMA_V2: &str = "ALTER TABLE profiles ADD COLUMN icon TEXT;";

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(SCHEMA_V1), M::up(SCHEMA_V2)])
}

#[derive(Debug)]
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(paths: &AppPaths) -> AppResult<Self> {
        paths.ensure()?;
        let mut connection = Connection::open(&paths.database)
            .map_err(|error| app_err!("无法打开数据库 {}: {error}", paths.database.display()))?;

        connection
            .pragma_update(None, "journal_mode", "WAL")
            .and_then(|_| connection.pragma_update(None, "foreign_keys", "ON"))
            .and_then(|_| connection.pragma_update(None, "synchronous", "NORMAL"))
            .map_err(|error| app_err!("无法初始化 SQLite: {error}"))?;

        let old_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(|error| app_err!("无法读取数据库版本: {error}"))?;
        if old_version > 0 {
            let backup = paths.database_backup.join(format!(
                "switchgpt-v{old_version}-{}.db",
                crate::paths::now_ms()
            ));
            connection
                .execute(
                    "VACUUM INTO ?1",
                    params![backup.to_string_lossy().to_string()],
                )
                .map_err(|error| app_err!("迁移前备份数据库失败: {error}"))?;
        }

        migrations()
            .to_latest(&mut connection)
            .map_err(|error| app_err!("数据库迁移失败: {error}"))?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn settings(&self) -> AppResult<Settings> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT key, value FROM settings")
            .map_err(|error| app_err!("无法读取设置: {error}"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| app_err!("无法读取设置: {error}"))?;

        let mut json = serde_json::Map::new();
        for row in rows {
            let (key, value) = row.map_err(|error| app_err!("无法读取设置行: {error}"))?;
            let decoded = serde_json::from_str::<serde_json::Value>(&value)
                .unwrap_or(serde_json::Value::Null);
            json.insert(key, decoded);
        }
        let raw = serde_json::Value::Object(json);
        let settings = serde_json::from_value(raw).unwrap_or_default();
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<()> {
        let values = [
            ("theme", serde_json::json!(settings.theme)),
            ("codex_app_path", serde_json::json!(settings.codex_app_path)),
            ("auto_restart", serde_json::json!(settings.auto_restart)),
            (
                "autostart_enabled",
                serde_json::json!(settings.autostart_enabled),
            ),
            ("silent_start", serde_json::json!(settings.silent_start)),
            (
                "minimize_to_tray",
                serde_json::json!(settings.minimize_to_tray),
            ),
            (
                "restart_timeout_ms",
                serde_json::json!(settings.restart_timeout_ms),
            ),
        ];
        let connection = self.lock()?;
        for (key, value) in values {
            connection
                .execute(
                    "INSERT INTO settings(key, value) VALUES(?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    params![key, value.to_string()],
                )
                .map_err(|error| app_err!("无法保存设置 {key}: {error}"))?;
        }
        Ok(())
    }

    pub fn profiles(&self) -> AppResult<Vec<StoredProfile>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare("SELECT id, name, payload_json, icon, created_at, updated_at FROM profiles ORDER BY updated_at DESC")
            .map_err(|error| app_err!("无法读取配置档案: {error}"))?;
        let rows = statement
            .query_map([], profile_from_row)
            .map_err(|error| app_err!("无法读取配置档案: {error}"))?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row.map_err(|error| app_err!("配置档案数据无效: {error}"))?);
        }
        Ok(profiles)
    }

    pub fn profile(&self, id: &str) -> AppResult<StoredProfile> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT id, name, payload_json, icon, created_at, updated_at FROM profiles WHERE id = ?1",
                params![id],
                profile_from_row,
            )
            .optional()
            .map_err(|error| app_err!("无法读取配置档案: {error}"))?
            .ok_or_else(|| app_err!("配置档案不存在"))
    }

    pub fn insert_profile(
        &self,
        name: &str,
        payload: &ProfilePayload,
        timestamp: &str,
    ) -> AppResult<ProfileSummary> {
        let id = format!("profile-{timestamp}");
        let payload_json =
            serde_json::to_string(payload).map_err(|_| app_err!("配置档案序列化失败"))?;
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO profiles(id, name, payload_json, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?4)",
                params![id, name, payload_json, timestamp],
            )
            .map_err(|error| app_err!("无法保存配置档案: {error}"))?;
        Ok(summary(&id, name, payload, None, timestamp, timestamp))
    }

    pub fn set_profile_icon(&self, id: &str, icon: Option<&str>, timestamp: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE profiles SET icon=?2, updated_at=?3 WHERE id=?1",
                params![id, icon, timestamp],
            )
            .map_err(|error| app_err!("无法更新配置档案图标: {error}"))?;
        if changed == 0 {
            return Err(app_err!("配置档案不存在"));
        }
        Ok(())
    }

    pub fn update_profile(
        &self,
        id: &str,
        name: &str,
        payload: &ProfilePayload,
        timestamp: &str,
    ) -> AppResult<StoredProfile> {
        let payload_json =
            serde_json::to_string(payload).map_err(|_| app_err!("配置档案序列化失败"))?;
        let connection = self.lock()?;
        connection
            .query_row(
                "UPDATE profiles SET name=?2, payload_json=?3, updated_at=?4 WHERE id=?1
                 RETURNING id, name, payload_json, icon, created_at, updated_at",
                params![id, name, payload_json, timestamp],
                profile_from_row,
            )
            .optional()
            .map_err(|error| app_err!("无法更新配置档案: {error}"))?
            .ok_or_else(|| app_err!("配置档案不存在"))
    }

    pub fn rename_profile(&self, id: &str, name: &str, timestamp: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE profiles SET name=?2, updated_at=?3 WHERE id=?1",
                params![id, name, timestamp],
            )
            .map_err(|error| app_err!("无法重命名配置档案: {error}"))?;
        if changed == 0 {
            return Err(app_err!("配置档案不存在"));
        }
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute("DELETE FROM profiles WHERE id=?1", params![id])
            .map_err(|error| app_err!("无法删除配置档案: {error}"))?;
        if changed == 0 {
            return Err(app_err!("配置档案不存在"));
        }
        Ok(())
    }

    pub fn record_event(
        &self,
        profile_id: Option<&str>,
        action: &str,
        status: &str,
        message: Option<&str>,
        timestamp: &str,
    ) -> AppResult<()> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO switch_events(profile_id, action, status, message, created_at)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![profile_id, action, status, message, timestamp],
            )
            .map_err(|error| app_err!("无法记录操作日志: {error}"))?;
        Ok(())
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| app_err!("数据库连接锁已损坏，请重启 SwitchGPT"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    pub payload: ProfilePayload,
    pub icon: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn profile_from_row(row: &Row<'_>) -> rusqlite::Result<StoredProfile> {
    let id = row.get(0)?;
    let name = row.get(1)?;
    let payload_json = row.get::<_, String>(2)?;
    let payload = serde_json::from_str(&payload_json).map_err(|_| rusqlite::Error::InvalidQuery)?;
    Ok(StoredProfile {
        id,
        name,
        payload,
        icon: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

fn summary(
    id: &str,
    name: &str,
    payload: &ProfilePayload,
    icon: Option<&str>,
    created_at: &str,
    updated_at: &str,
) -> ProfileSummary {
    ProfileSummary {
        id: id.into(),
        name: name.into(),
        model: display_text(payload.model_values.get("model")),
        provider: payload.provider_id.clone(),
        reasoning_effort: display_text(payload.model_values.get("model_reasoning_effort")),
        icon: icon.map(str::to_string),
        created_at: created_at.into(),
        updated_at: updated_at.into(),
    }
}

fn display_text(value: Option<&String>) -> Option<String> {
    value.map(|raw| raw.trim().trim_matches('"').to_string())
}

pub fn profile_summary(profile: &StoredProfile) -> ProfileSummary {
    summary(
        &profile.id,
        &profile.name,
        &profile.payload,
        profile.icon.as_deref(),
        &profile.created_at,
        &profile.updated_at,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_creates_schema_and_settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(dir.path()).unwrap();
        let db = Database::open(&paths).unwrap();

        let names: Vec<String> = db
            .lock()
            .unwrap()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert!(names.contains(&"profiles".into()));
        assert!(names.contains(&"settings".into()));
        assert!(names.contains(&"switch_events".into()));

        let settings = Settings {
            auto_restart: true,
            ..Settings::default()
        };
        db.save_settings(&settings).unwrap();
        assert_eq!(db.settings().unwrap(), settings);
    }

    #[test]
    fn set_profile_icon_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(dir.path()).unwrap();
        let db = Database::open(&paths).unwrap();

        let payload = ProfilePayload::default();
        let summary = db.insert_profile("GLM High", &payload, "1").unwrap();
        assert_eq!(summary.icon, None);

        db.set_profile_icon(&summary.id, Some("zhipu"), "2")
            .unwrap();
        assert_eq!(db.profiles().unwrap()[0].icon.as_deref(), Some("zhipu"));

        db.set_profile_icon(&summary.id, None, "3").unwrap();
        assert_eq!(db.profiles().unwrap()[0].icon, None);

        assert!(db.set_profile_icon("missing", Some("zhipu"), "4").is_err());
    }
}
