use std::path::{Path, PathBuf};

use crate::error::{err, AppResult};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
    pub database: PathBuf,
    pub settings: PathBuf,
    pub config_backup: PathBuf,
    pub database_backup: PathBuf,
    pub codex_files_backup: PathBuf,
    pub plugin_backup: PathBuf,
    pub codex_home: PathBuf,
    /// Codex 插件生态的公共根（个人 marketplace 与插件 store 都在其下），创建交给插件安装流程。
    pub agents_home: PathBuf,
    /// CGswitch 自管数据：已装插件来源 + 被移除外部条目的暂存（marketplace.json 是 Codex 的文件，不塞私有字段）。
    pub plugin_state: PathBuf,
}

impl AppPaths {
    pub fn codex_config(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }

    pub fn ensure(&self) -> AppResult<()> {
        for dir in [
            &self.root,
            &self.config_backup,
            &self.database_backup,
            &self.codex_files_backup,
            &self.plugin_backup,
        ] {
            std::fs::create_dir_all(dir)
                .map_err(|error| err(format!("无法创建目录 {}: {error}", dir.display())))?;
        }
        Ok(())
    }
}

pub fn app_paths() -> AppResult<AppPaths> {
    from_home(&home_dir())
}

pub fn from_home(home: &Path) -> AppResult<AppPaths> {
    let root = home.join(".cgswitch");
    let agents_home = home.join(".agents");
    Ok(AppPaths {
        database: root.join("cgswitch.db"),
        settings: root.join("settings.json"),
        config_backup: root.join("backups").join("config"),
        database_backup: root.join("backups").join("database"),
        codex_files_backup: root.join("backups").join("codex-files"),
        plugin_backup: root.join("backups").join("plugins"),
        plugin_state: root.join("plugin-state.json"),
        codex_home: home.join(".codex"),
        agents_home,
        root,
    })
}

pub fn home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|base| base.home_dir().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_use_cgswitch_and_codex_directories() {
        let home = Path::new("/home/user");
        let paths = from_home(home).unwrap();
        assert_eq!(paths.root, home.join(".cgswitch"));
        assert_eq!(paths.database, home.join(".cgswitch").join("cgswitch.db"));
        assert_eq!(
            paths.codex_config(),
            home.join(".codex").join("config.toml")
        );
    }
}
