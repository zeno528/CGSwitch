use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
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

const SCHEMA_V3: &str = r#"
ALTER TABLE profiles RENAME TO profiles_old;

CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  icon TEXT
);

INSERT INTO profiles (id, name, payload_json, created_at, updated_at, icon)
  SELECT id, name, payload_json, created_at, updated_at, icon FROM profiles_old;

CREATE TABLE switch_events_new (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  profile_id TEXT,
  action TEXT NOT NULL,
  status TEXT NOT NULL,
  message TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY(profile_id) REFERENCES profiles(id) ON DELETE SET NULL
);

INSERT INTO switch_events_new (id, profile_id, action, status, message, created_at)
  SELECT id, profile_id, action, status, message, created_at FROM switch_events;

DROP TABLE switch_events;
ALTER TABLE switch_events_new RENAME TO switch_events;
DROP TABLE profiles_old;

CREATE INDEX idx_switch_events_created_at ON switch_events(created_at DESC);
CREATE INDEX idx_switch_events_profile_id ON switch_events(profile_id);
"#;

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(SCHEMA_V1), M::up(SCHEMA_V2), M::up(SCHEMA_V3)])
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
        let pending = migrations()
            .pending_migrations(&connection)
            .map_err(|error| app_err!("无法检查数据库迁移: {error}"))?;
        // 仅在确有 pending 迁移时快照（普通重启不备份），迁移备份保留最近 5 份
        if old_version > 0 && pending > 0 {
            let backup = paths.database_backup.join(format!(
                "cgswitch-v{old_version}-{}.db",
                crate::paths::now_ms()
            ));
            if let Err(error) = connection.execute(
                "VACUUM INTO ?1",
                params![backup.to_string_lossy().to_string()],
            ) {
                // 迁移本身是事务性的，备份失败不阻塞启动，仅告警
                eprintln!("迁移前数据库备份失败: {error}");
            } else {
                crate::fsutil::prune_backups(&paths.database_backup, "cgswitch-v", ".db", 5);
            }
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
            .prepare("SELECT id, name, payload_json, icon, created_at, updated_at FROM profiles ORDER BY created_at ASC, id ASC")
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

    /// 最近一次成功应用的档案 id（应用记录被删除时返回 None 由调用方回退匹配）。
    pub fn latest_applied_profile(&self) -> AppResult<Option<String>> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT profile_id FROM switch_events
                 WHERE action = 'apply' AND status = 'success' AND profile_id IS NOT NULL
                 ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| app_err!("无法读取最近应用记录: {error}"))
    }

    /// 把当前数据库导出为一致性快照文件（VACUUM INTO）。
    pub fn export_database(&self, target: &Path) -> AppResult<()> {
        let connection = self.lock()?;
        connection
            .execute(
                "VACUUM INTO ?1",
                params![target.to_string_lossy().to_string()],
            )
            .map_err(|error| app_err!("数据库导出失败: {error}"))?;
        Ok(())
    }

    /// 从备份文件把数据恢复进当前数据库（清空现有数据后复制）。
    pub fn restore_from_backup(&self, backup: &Path) -> AppResult<()> {
        let source = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|error| app_err!("无法打开备份文件: {error}"))?;
        let has_profiles: i64 = source
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='profiles'",
                [],
                |row| row.get(0),
            )
            .map_err(|error| app_err!("备份文件不是有效的 CGSwitch 数据库: {error}"))?;
        if has_profiles == 0 {
            return Err(app_err!("备份文件不是有效的 CGSwitch 数据库"));
        }

        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| app_err!("无法开启恢复事务: {error}"))?;
        transaction
            .execute("DELETE FROM switch_events", [])
            .and_then(|_| transaction.execute("DELETE FROM profiles", []))
            .and_then(|_| transaction.execute("DELETE FROM settings", []))
            .map_err(|error| app_err!("恢复前清理数据失败: {error}"))?;

        copy_table(
            &source,
            &transaction,
            "settings",
            "SELECT key, value FROM settings",
            "INSERT INTO settings(key, value) VALUES(?1, ?2)",
        )?;
        copy_table(
            &source,
            &transaction,
            "profiles",
            "SELECT id, name, payload_json, icon, created_at, updated_at FROM profiles",
            "INSERT INTO profiles(id, name, payload_json, icon, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        copy_table(
            &source,
            &transaction,
            "switch_events",
            "SELECT id, profile_id, action, status, message, created_at FROM switch_events",
            "INSERT INTO switch_events(id, profile_id, action, status, message, created_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        )?;

        transaction
            .commit()
            .map_err(|error| app_err!("恢复事务提交失败: {error}"))?;
        Ok(())
    }

    fn lock(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| app_err!("数据库连接锁已损坏，请重启 CGSwitch"))
    }
}

fn copy_table(
    source: &Connection,
    destination: &rusqlite::Transaction<'_>,
    table: &str,
    select_sql: &str,
    insert_sql: &str,
) -> AppResult<()> {
    let mut statement = source
        .prepare(select_sql)
        .map_err(|error| app_err!("读取备份表 {table} 失败: {error}"))?;
    let column_count = statement.column_count();
    let mut rows = statement
        .query([])
        .map_err(|error| app_err!("读取备份表 {table} 失败: {error}"))?;
    while let Some(row) = rows
        .next()
        .map_err(|error| app_err!("读取备份表 {table} 失败: {error}"))?
    {
        let mut values: Vec<rusqlite::types::Value> = Vec::with_capacity(column_count);
        for index in 0..column_count {
            values.push(
                row.get::<_, rusqlite::types::Value>(index)
                    .map_err(|error| app_err!("读取备份表 {table} 失败: {error}"))?,
            );
        }
        destination
            .execute(insert_sql, rusqlite::params_from_iter(values))
            .map_err(|error| app_err!("恢复表 {table} 失败: {error}"))?;
    }
    Ok(())
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
        has_key: payload_has_key(payload),
        admin_url: payload.admin_url.clone(),
        icon: icon.map(str::to_string),
        created_at: created_at.into(),
        updated_at: updated_at.into(),
    }
}

fn payload_has_key(payload: &ProfilePayload) -> bool {
    let Some(body) = payload.provider_body.as_deref() else {
        return false;
    };
    let Ok(document) = body.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    let Some(key) = document
        .as_table()
        .get("experimental_bearer_token")
        .and_then(toml_edit::Item::as_str)
    else {
        return false;
    };
    if key.trim().is_empty() {
        return false;
    }
    if let Some(kind) = payload.builtin.as_deref() {
        if let Ok(template) = crate::builtin::template(kind) {
            if template
                .placeholder
                .is_some_and(|placeholder| placeholder == key.as_bytes())
            {
                return false;
            }
        }
    }
    true
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

    #[test]
    fn migration_backup_only_when_pending_and_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(dir.path()).unwrap();
        paths.ensure().unwrap();

        // 构造一个 v1 数据库，并预置 6 份旧迁移备份
        let mut conn = Connection::open(&paths.database).unwrap();
        Migrations::new(vec![M::up(SCHEMA_V1)])
            .to_latest(&mut conn)
            .unwrap();
        drop(conn);
        for i in 0..6 {
            std::fs::write(paths.database_backup.join(format!("cgswitch-v1-{i}.db")), b"x")
                .unwrap();
        }

        // 首次打开：v1 -> v2 有 pending 迁移，生成 1 份新备份并裁剪到 5 份
        Database::open(&paths).unwrap();
        let count = std::fs::read_dir(&paths.database_backup).unwrap().count();
        assert_eq!(count, 5);

        // 再次打开：已到最新版本，不再生成备份
        Database::open(&paths).unwrap();
        assert_eq!(std::fs::read_dir(&paths.database_backup).unwrap().count(), 5);
    }

}
