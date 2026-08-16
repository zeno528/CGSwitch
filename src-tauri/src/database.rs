use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};

use crate::error::{app_err, AppResult};
use crate::models::{ProfileKind, ProfilePayload, ProfileSummary};
use crate::paths::AppPaths;

const SCHEMA_V1: &str = r#"
CREATE TABLE app_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  active_profile_id TEXT,
  default_account_id TEXT
);

CREATE TABLE accounts (
  id TEXT PRIMARY KEY,
  email TEXT,
  id_token TEXT,
  refresh_token TEXT NOT NULL,
  authenticated_at INTEGER NOT NULL
);

CREATE TABLE profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  icon TEXT,
  kind TEXT NOT NULL,
  account_id TEXT REFERENCES accounts(id) ON DELETE SET NULL,
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

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(SCHEMA_V1)])
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

        migrations()
            .to_latest(&mut connection)
            .map_err(|error| app_err!("数据库迁移失败: {error}"))?;

        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn profiles(&self) -> AppResult<Vec<StoredProfile>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, payload_json, icon, kind, account_id, created_at, updated_at
                 FROM profiles ORDER BY created_at ASC, id ASC",
            )
            .map_err(|error| app_err!("无法读取配置预设: {error}"))?;
        let rows = statement
            .query_map([], profile_from_row)
            .map_err(|error| app_err!("无法读取配置预设: {error}"))?;

        let mut profiles = Vec::new();
        for row in rows {
            profiles.push(row.map_err(|error| app_err!("配置预设数据无效: {error}"))?);
        }
        Ok(profiles)
    }

    pub fn profile(&self, id: &str) -> AppResult<StoredProfile> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT id, name, payload_json, icon, kind, account_id, created_at, updated_at
                 FROM profiles WHERE id = ?1",
                params![id],
                profile_from_row,
            )
            .optional()
            .map_err(|error| app_err!("无法读取配置预设: {error}"))?
            .ok_or_else(|| app_err!("配置预设不存在"))
    }

    pub fn insert_profile(
        &self,
        name: &str,
        payload: &ProfilePayload,
        timestamp: &str,
    ) -> AppResult<ProfileSummary> {
        let id = format!("profile-{timestamp}");
        let payload_json =
            serde_json::to_string(payload).map_err(|_| app_err!("配置预设序列化失败"))?;
        let kind = if payload.provider_id.is_none() {
            ProfileKind::Official
        } else {
            ProfileKind::ThirdParty
        };
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO profiles(id, name, payload_json, kind, created_at, updated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, name, payload_json, kind.as_db(), timestamp],
            )
            .map_err(|error| app_err!("无法保存配置预设: {error}"))?;
        Ok(summary(&id, name, payload, None, timestamp, timestamp))
    }

    pub fn set_profile_icon(&self, id: &str, icon: Option<&str>, timestamp: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE profiles SET icon=?2, updated_at=?3 WHERE id=?1",
                params![id, icon, timestamp],
            )
            .map_err(|error| app_err!("无法更新配置预设图标: {error}"))?;
        if changed == 0 {
            return Err(app_err!("配置预设不存在"));
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
            serde_json::to_string(payload).map_err(|_| app_err!("配置预设序列化失败"))?;
        let connection = self.lock()?;
        connection
            .query_row(
                "UPDATE profiles SET name=?2, payload_json=?3, updated_at=?4 WHERE id=?1
                 RETURNING id, name, payload_json, icon, kind, account_id, created_at, updated_at",
                params![id, name, payload_json, timestamp],
                profile_from_row,
            )
            .optional()
            .map_err(|error| app_err!("无法更新配置预设: {error}"))?
            .ok_or_else(|| app_err!("配置预设不存在"))
    }

    pub fn rename_profile(&self, id: &str, name: &str, timestamp: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE profiles SET name=?2, updated_at=?3 WHERE id=?1",
                params![id, name, timestamp],
            )
            .map_err(|error| app_err!("无法重命名配置预设: {error}"))?;
        if changed == 0 {
            return Err(app_err!("配置预设不存在"));
        }
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute("DELETE FROM profiles WHERE id=?1", params![id])
            .map_err(|error| app_err!("无法删除配置预设: {error}"))?;
        if changed == 0 {
            return Err(app_err!("配置预设不存在"));
        }
        Ok(())
    }

    pub fn accounts(&self) -> AppResult<Vec<StoredAccount>> {
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT id, email, id_token, refresh_token, authenticated_at
                 FROM accounts ORDER BY authenticated_at DESC, id ASC",
            )
            .map_err(|error| app_err!("无法读取订阅账号: {error}"))?;
        let rows = statement
            .query_map([], account_from_row)
            .map_err(|error| app_err!("无法读取订阅账号: {error}"))?;

        let mut accounts = Vec::new();
        for row in rows {
            accounts.push(row.map_err(|error| app_err!("订阅账号数据无效: {error}"))?);
        }
        Ok(accounts)
    }

    pub fn upsert_account(&self, account: &StoredAccount) -> AppResult<()> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO accounts(id, email, id_token, refresh_token, authenticated_at)
                 VALUES(?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                   email=excluded.email,
                   id_token=excluded.id_token,
                   refresh_token=excluded.refresh_token,
                   authenticated_at=excluded.authenticated_at",
                params![
                    account.id,
                    account.email,
                    account.id_token,
                    account.refresh_token,
                    account.authenticated_at,
                ],
            )
            .map_err(|error| app_err!("无法保存订阅账号: {error}"))?;
        Ok(())
    }

    pub fn delete_account(&self, id: &str) -> AppResult<()> {
        let connection = self.lock()?;
        let changed = connection
            .execute("DELETE FROM accounts WHERE id=?1", params![id])
            .map_err(|error| app_err!("无法删除订阅账号: {error}"))?;
        if changed == 0 {
            return Err(app_err!("订阅账号不存在"));
        }
        Ok(())
    }

    /// 读取单行应用状态：返回 (active_profile_id, default_account_id)。
    pub fn app_state(&self) -> AppResult<(Option<String>, Option<String>)> {
        let connection = self.lock()?;
        connection
            .query_row(
                "SELECT active_profile_id, default_account_id FROM app_state WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| app_err!("无法读取应用状态: {error}"))
            .map(|state| state.unwrap_or((None, None)))
    }

    pub fn set_active_profile(&self, id: Option<&str>) -> AppResult<()> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO app_state(singleton, active_profile_id) VALUES(1, ?1)
                 ON CONFLICT(singleton) DO UPDATE SET active_profile_id=excluded.active_profile_id",
                params![id],
            )
            .map_err(|error| app_err!("无法保存应用状态: {error}"))?;
        Ok(())
    }

    pub fn set_default_account(&self, id: Option<&str>) -> AppResult<()> {
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO app_state(singleton, default_account_id) VALUES(1, ?1)
                 ON CONFLICT(singleton) DO UPDATE SET default_account_id=excluded.default_account_id",
                params![id],
            )
            .map_err(|error| app_err!("无法保存应用状态: {error}"))?;
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

    /// 最近一次成功应用的预设 id（应用记录被删除时返回 None 由调用方回退匹配）。
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
            .and_then(|_| transaction.execute("DELETE FROM accounts", []))
            .and_then(|_| transaction.execute("DELETE FROM app_state", []))
            .map_err(|error| app_err!("恢复前清理数据失败: {error}"))?;

        copy_table(
            &source,
            &transaction,
            "accounts",
            "SELECT id, email, id_token, refresh_token, authenticated_at FROM accounts",
            "INSERT INTO accounts(id, email, id_token, refresh_token, authenticated_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
        )?;
        copy_table(
            &source,
            &transaction,
            "app_state",
            "SELECT singleton, active_profile_id, default_account_id FROM app_state",
            "INSERT INTO app_state(singleton, active_profile_id, default_account_id)
             VALUES(?1, ?2, ?3)",
        )?;
        copy_table(
            &source,
            &transaction,
            "profiles",
            "SELECT id, name, payload_json, icon, kind, account_id, created_at, updated_at FROM profiles",
            "INSERT INTO profiles(id, name, payload_json, icon, kind, account_id, created_at, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
    pub kind: ProfileKind,
    pub account_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub email: Option<String>,
    pub id_token: Option<String>,
    pub refresh_token: String,
    pub authenticated_at: i64,
}

fn profile_from_row(row: &Row<'_>) -> rusqlite::Result<StoredProfile> {
    let id = row.get(0)?;
    let name = row.get(1)?;
    let payload_json = row.get::<_, String>(2)?;
    let payload = serde_json::from_str(&payload_json).map_err(|_| rusqlite::Error::InvalidQuery)?;
    let kind_raw: String = row.get(4)?;
    let kind = ProfileKind::from_db(&kind_raw)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(StoredProfile {
        id,
        name,
        payload,
        icon: row.get(3)?,
        kind,
        account_id: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn account_from_row(row: &Row<'_>) -> rusqlite::Result<StoredAccount> {
    Ok(StoredAccount {
        id: row.get(0)?,
        email: row.get(1)?,
        id_token: row.get(2)?,
        refresh_token: row.get(3)?,
        authenticated_at: row.get(4)?,
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
    fn migration_creates_schema() {
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
        assert!(names.contains(&"app_state".into()));
        assert!(names.contains(&"switch_events".into()));
        assert!(names.contains(&"accounts".into()));
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
    fn profile_kind_and_accounts_state_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(dir.path()).unwrap();
        let db = Database::open(&paths).unwrap();

        // 无供应商 → 官方；有供应商 → 第三方
        let mut official = ProfilePayload::default();
        official.model_values
            .insert("model".into(), "\"gpt-5.6\"".into());
        let official_id = db.insert_profile("官方", &official, "1").unwrap().id;
        let mut third = ProfilePayload::default();
        third.provider_id = Some("ZAI".into());
        third.provider_body = Some("name = \"ZAI\"".into());
        let third_id = db.insert_profile("第三方", &third, "2").unwrap().id;
        assert_eq!(db.profile(&official_id).unwrap().kind, ProfileKind::Official);
        assert_eq!(db.profile(&third_id).unwrap().kind, ProfileKind::ThirdParty);

        let account = StoredAccount {
            id: "acc-1".into(),
            email: Some("a@example.com".into()),
            id_token: Some("id-jwt".into()),
            refresh_token: "rt-1".into(),
            authenticated_at: 100,
        };
        db.upsert_account(&account).unwrap();
        assert_eq!(db.accounts().unwrap()[0].refresh_token, "rt-1");
        db.delete_account("acc-1").unwrap();
        assert!(db.accounts().unwrap().is_empty());

        assert_eq!(db.app_state().unwrap(), (None, None));
        db.set_active_profile(Some(&official_id)).unwrap();
        db.set_default_account(Some("acc-1")).unwrap();
        let (active, default) = db.app_state().unwrap();
        assert_eq!(active.as_deref(), Some(official_id.as_str()));
        assert_eq!(default.as_deref(), Some("acc-1"));
        db.set_active_profile(None).unwrap();
        db.set_default_account(None).unwrap();
        assert_eq!(db.app_state().unwrap(), (None, None));
    }

}
