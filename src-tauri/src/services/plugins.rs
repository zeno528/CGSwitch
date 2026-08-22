//! 插件市场服务：GitHub 插件的发现、安装与本地生命周期。
//!
//! 安装模型（以实测的 Codex CLI 0.149 为准）：
//! - 安装 = `codex plugin marketplace add <git 源>` + `codex plugin add <插件@市场>`；
//!   卸载 = `codex plugin remove <插件@市场>`——官方路径，状态由 Codex 自己维护；
//! - 预览走 CGswitch 自己的 GitHub 拉取（清单、文件列表、内容类型，不落盘）；
//! - 列表以 `codex plugin list` 为主源（覆盖官方运行时/捆绑/第三方市场，含启停状态），
//!   CLI 不在时回退扫 `~/.codex/plugins/cache/`；Skill 注册表 `~/.agents/.skill-lock.json`
//!   与家目录四套 marketplace 布局的 local 条目也在列；
//! - origin 语义：cgswitch=本应用经 CLI 安装；codex=用户自装的第三方市场插件（可卸载）；
//!   official=openai 运行时/捆绑市场（只读）；skill=Skill 注册表（只读）；
//!   personal/claude/cursor=家目录 local 条目（可禁用/移除，条目暂存可恢复）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::plugin_net::{
    fetch_raw_file, fetch_repo_tree, parse_github_url, preview_file_limit, TreeEntry,
};
use super::{app_err, atomic_write, backup_file, now_ms, AppContext, AppResult};

/// 前端展示用的已安装插件摘要。
#[derive(Debug, Clone, Serialize, Default)]
pub struct PluginSummary {
    pub name: String,
    pub version: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub capabilities: Vec<String>,
    pub contains: Vec<String>,
    pub enabled: bool,
    /// cgswitch=本应用安装；codex=用户自装第三方市场；official=openai 官方市场（只读）；
    /// skill=Skill 注册表（只读）；personal/claude/cursor=家目录 local 条目。
    pub origin: String,
    /// 来自 `codex plugin list` 的市场名（卸载选择器要用）。
    pub marketplace: Option<String>,
    pub store_path: String,
    pub source_url: Option<String>,
    pub installed_at: Option<i64>,
}

/// 预览阶段的候选插件（一个仓库可能包含多个插件根目录）。
#[derive(Debug, Clone, Serialize)]
pub struct PluginCandidate {
    pub sub_path: String,
    pub name: String,
    pub version: Option<String>,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub capabilities: Vec<String>,
    pub contains: Vec<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginPreview {
    pub repo: String,
    pub reference: String,
    pub default_branch: String,
    pub candidates: Vec<PluginCandidate>,
}

/// plugin.json 的解析子集（官方字段很多，这里只取列表与详情需要的）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginManifest {
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    interface: Option<PluginInterface>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginInterface {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
}

/// CGswitch 自管：经 CLI 安装的插件标记（plugin-state.json）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginOrigin {
    source_url: String,
    marketplace: String,
    #[serde(default)]
    version: Option<String>,
    installed_at: i64,
}

/// 被摘除的外部条目暂存（禁用/移除后可恢复）。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StashedEntry {
    /// 所属 marketplace 的布局相对路径（如 `.claude-plugin/marketplace.json`）。
    marketplace: String,
    entry: Value,
}

/// `~/.agents/.skill-lock.json` 的解析结构（Codex 的 Skill 安装注册表）。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillLock {
    #[serde(default)]
    skills: BTreeMap<String, SkillLockEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillLockEntry {
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    source_url: Option<String>,
    #[serde(default)]
    installed_at: Option<String>,
}

const MANIFEST_RELATIVE_PATH: &str = ".codex-plugin/plugin.json";
/// Claude 布局的清单回退路径（多 Agent 插件两种都有，如 ponytail）。
const CLAUDE_MANIFEST_RELATIVE_PATH: &str = ".claude-plugin/plugin.json";
const SKILL_LOCK_RELATIVE_PATH: &str = ".agents/.skill-lock.json";
/// Codex marketplace 插件的物化缓存（相对 codex home，CLI 缺席时的回退数据源）。
const PLUGIN_CACHE_RELATIVE_PATH: &str = "plugins/cache";

/// 家目录下的四套 marketplace 布局（顺序即 Codex 的发现顺序）。
/// (布局相对路径, 该布局下外部条目的 origin 标签, 是否可编辑)。
const HOME_MARKETPLACE_LAYOUTS: &[(&str, &str, bool)] = &[
    (".agents/plugins/marketplace.json", "personal", true),
    (".agents/plugins/api_marketplace.json", "official", false),
    (".claude-plugin/marketplace.json", "claude", true),
    (".cursor-plugin/marketplace.json", "cursor", true),
];

impl PluginSummary {
    fn from_parts(
        name: &str,
        manifest: Option<&PluginManifest>,
        contains: Vec<String>,
        origin: &str,
        store_path: &Path,
        plugin_origin: Option<&PluginOrigin>,
    ) -> Self {
        let interface = manifest.and_then(|item| item.interface.as_ref());
        Self {
            name: manifest
                .map(|item| item.name.clone())
                .unwrap_or_else(|| name.to_string()),
            version: manifest
                .and_then(|item| item.version.clone())
                .or(plugin_origin.and_then(|origin| origin.version.clone())),
            display_name: interface.and_then(|item| item.display_name.clone()),
            description: manifest.and_then(|item| item.description.clone()),
            category: interface.and_then(|item| item.category.clone()),
            capabilities: interface
                .map(|item| item.capabilities.clone())
                .unwrap_or_default(),
            contains,
            enabled: false,
            origin: origin.to_string(),
            marketplace: plugin_origin.map(|item| item.marketplace.clone()),
            store_path: store_path.display().to_string(),
            source_url: plugin_origin.map(|item| item.source_url.clone()),
            installed_at: plugin_origin.map(|item| item.installed_at),
        }
    }
}

// ==================== codex CLI 执行层 ====================

fn codex_cli_file_name() -> &'static str {
    if cfg!(windows) {
        "codex.exe"
    } else {
        "codex"
    }
}

/// codex CLI 探测链：`~/.codex/bin`（CLI 安装约定）、Desktop appserver 自带的副本、PATH。
fn find_codex_cli(home: &Path) -> Option<PathBuf> {
    let mut candidates = vec![
        home.join(".codex").join("bin").join(codex_cli_file_name()),
        home.join(".codex")
            .join("plugins")
            .join(".plugin-appserver")
            .join(codex_cli_file_name()),
    ];
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            candidates.push(dir.join(codex_cli_file_name()));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// 跑 `codex plugin <args>`，返回 stdout；失败时把 CLI 的报错带出来。
fn run_codex_plugin(home: &Path, args: &[&str]) -> AppResult<String> {
    let cli = find_codex_cli(home).ok_or_else(|| {
        app_err!(
            "未找到 codex CLI（已尝试 ~/.codex/bin、桌面版 appserver 目录与 PATH），无法管理插件"
        )
    })?;
    let mut command = std::process::Command::new(&cli);
    command.arg("plugin").args(args);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|error| app_err!("执行 codex CLI 失败: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(app_err!("codex CLI 报错：{detail}"));
    }
    Ok(stdout)
}

/// 解析 `codex plugin list` 的表格输出：
/// `<插件>@<市场>  installed, enabled|disabled  <版本>  <路径>`；`not installed` 跳过。
fn parse_plugin_list_output(text: &str) -> Vec<(String, String, bool, Option<String>, String)> {
    let mut items = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with("PLUGIN ")
            || trimmed.starts_with("Marketplace `")
        {
            continue;
        }
        let Some((selector, rest)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((plugin, marketplace)) = selector.split_once('@') else {
            continue;
        };
        let rest = rest.trim_start();
        if rest.starts_with("not installed") {
            continue;
        }
        if !rest.starts_with("installed,") {
            continue;
        }
        let enabled = rest.starts_with("installed, enabled");
        let after_status = rest
            .strip_prefix("installed, enabled")
            .or_else(|| rest.strip_prefix("installed, disabled"))
            .unwrap_or(rest)
            .trim_start();
        let mut parts = after_status.split_whitespace();
        let version = parts
            .next()
            .filter(|token| !token.contains('\\') && !token.contains('/'));
        let path = parts.collect::<Vec<_>>().join(" ");
        items.push((
            plugin.to_string(),
            marketplace.to_string(),
            enabled,
            version.map(str::to_string),
            path,
        ));
    }
    items
}

/// 从 `marketplace add` 的输出里解析市场名（如 “Marketplace `ponytail`”），失败回退仓库名。
fn parse_marketplace_name(output: &str, fallback: &str) -> String {
    if let Some(start) = output.find('`') {
        if let Some(length) = output[start + 1..].find('`') {
            let name = &output[start + 1..start + 1 + length];
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    fallback.to_string()
}

// ==================== marketplace.json（Value 级操作，保留用户手写字段） ====================

fn read_marketplace_json(path: &Path) -> AppResult<Value> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|error| {
            app_err!(
                "marketplace.json 不是有效 JSON（{}）: {error}",
                path.display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => Err(app_err!("无法读取 {}: {error}", path.display())),
    }
}

fn marketplace_plugins_mut(document: &mut Value) -> AppResult<&mut Vec<Value>> {
    if document.is_null() {
        *document = json!({});
    }
    let object = document
        .as_object_mut()
        .ok_or_else(|| app_err!("marketplace.json 顶层必须是 JSON 对象"))?;
    if !object.contains_key("name") {
        object.insert("name".into(), json!("cgswitch"));
    }
    if !object.contains_key("plugins") {
        object.insert("plugins".into(), json!([]));
    }
    object
        .get_mut("plugins")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| app_err!("marketplace.json 的 plugins 字段必须是数组"))
}

fn write_marketplace(path: &Path, document: &Value) -> AppResult<()> {
    let text =
        serde_json::to_string_pretty(document).map_err(|_| app_err!("marketplace 序列化失败"))?;
    atomic_write(path, text.as_bytes())
}

/// 从指定 marketplace 摘除条目（文件必须已存在），返回被摘除的条目供暂存恢复。
fn take_entry_from(context: &AppContext, path: &Path, name: &str) -> AppResult<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let mut document = read_marketplace_json(path)?;
    let plugins = marketplace_plugins_mut(&mut document)?;
    let mut removed = None;
    plugins.retain(|entry| {
        if entry.get("name").and_then(Value::as_str) == Some(name) {
            removed = Some(entry.clone());
            false
        } else {
            true
        }
    });
    if removed.is_some() {
        backup_file(path, &context.paths.plugin_backup, "marketplace")?;
        write_marketplace(path, &document)?;
    }
    Ok(removed)
}

/// 把暂存的条目放回其所属 marketplace。
fn restore_entry_to(context: &AppContext, path: &Path, entry: Value) -> AppResult<()> {
    let name = entry
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let mut document = read_marketplace_json(path)?;
    let plugins = marketplace_plugins_mut(&mut document)?;
    plugins.retain(|item| item.get("name").and_then(Value::as_str) != Some(name.as_str()));
    plugins.push(entry);
    if path.is_file() {
        backup_file(path, &context.paths.plugin_backup, "marketplace")?;
    }
    write_marketplace(path, &document)
}

// ==================== plugin-state.json（CGswitch 自管） ====================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PluginSidecar {
    #[serde(default)]
    origins: BTreeMap<String, PluginOrigin>,
    #[serde(default)]
    disabled: BTreeMap<String, StashedEntry>,
}

fn load_sidecar(path: &Path) -> PluginSidecar {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_sidecar(path: &Path, sidecar: &PluginSidecar) -> AppResult<()> {
    let text = serde_json::to_string_pretty(sidecar).map_err(|_| app_err!("插件状态序列化失败"))?;
    atomic_write(path, text.as_bytes())
}

// ==================== 发现逻辑 ====================

struct DiscoveredMarketplace {
    label: &'static str,
    document: Value,
}

fn discover_marketplaces(home: &Path) -> Vec<DiscoveredMarketplace> {
    let mut found = Vec::new();
    for (layout, label, _editable) in HOME_MARKETPLACE_LAYOUTS {
        let path = home.join(layout);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(document) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        found.push(DiscoveredMarketplace { label, document });
    }
    found
}

/// 条目的 local 源路径（兼容官方两种写法：字符串形式与 `{source:"local", path}` 对象形式）。
fn entry_local_path(entry: &Value) -> Option<String> {
    match entry.get("source")? {
        Value::String(path) => Some(path.clone()),
        Value::Object(object) => {
            if object.get("source").and_then(Value::as_str) == Some("local") {
                object.get("path")?.as_str().map(String::from)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// local 路径相对 marketplace 根（= 用户主目录）解析；拒绝绝对路径与 `..` 穿越。
/// `.cursor-plugin` 布局允许无 `./` 前缀（与 Codex 行为一致）。
fn resolve_local_path(home: &Path, raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    let relative = trimmed.strip_prefix("./").unwrap_or(trimmed);
    let relative_path = Path::new(relative);
    if !relative_path.is_relative() {
        return None;
    }
    let mut full = home.to_path_buf();
    for component in relative_path.components() {
        match component {
            std::path::Component::Normal(part) => full.push(part),
            _ => return None,
        }
    }
    Some(full)
}

// ==================== 共享工具 ====================

fn parse_manifest_text(text: &str) -> AppResult<PluginManifest> {
    serde_json::from_str(text).map_err(|error| app_err!("plugin.json 不是有效 JSON: {error}"))
}

fn read_manifest(plugin_root: &Path) -> Option<PluginManifest> {
    let text = std::fs::read_to_string(plugin_root.join(MANIFEST_RELATIVE_PATH))
        .or_else(|_| std::fs::read_to_string(plugin_root.join(CLAUDE_MANIFEST_RELATIVE_PATH)))
        .ok()?;
    parse_manifest_text(&text).ok()
}

fn parse_rfc3339_ms(text: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.timestamp_millis())
}

/// 插件名做选择器/目录名：拒绝路径分隔与目录穿越，其余保留官方命名。
fn validate_plugin_name(name: &str) -> AppResult<()> {
    let valid = !name.is_empty()
        && name.len() <= 80
        && !name.contains(['/', '\\', ':', '@'])
        && name != "."
        && name != "..";
    if valid {
        Ok(())
    } else {
        Err(app_err!("插件名「{name}」包含非法字符"))
    }
}

/// 从文件树里找插件根目录（.codex-plugin/plugin.json 的父目录），仓库根插件表示为空串。
fn plugin_roots(entries: &[TreeEntry]) -> Vec<String> {
    let mut roots: Vec<String> = entries
        .iter()
        .filter(|entry| entry.kind == "blob" && entry.path.ends_with(MANIFEST_RELATIVE_PATH))
        .map(|entry| {
            entry
                .path
                .strip_suffix(&format!("/{MANIFEST_RELATIVE_PATH}"))
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

/// 判断 root 是否等于指定子路径或位于其下（子目录过滤用）。
fn root_within(root: &str, sub_path: &str) -> bool {
    let sub = sub_path.trim_matches('/');
    if sub.is_empty() {
        return true;
    }
    root == sub || root.starts_with(&format!("{sub}/"))
}

fn files_under_root<'a>(entries: &'a [TreeEntry], root: &str) -> Vec<&'a str> {
    entries
        .iter()
        .filter(|entry| entry.kind == "blob" && root_within(&entry.path, root))
        .map(|entry| entry.path.as_str())
        .collect()
}

/// 从插件文件清单推导内容类型（白皮书的「包含内容」维度）。
fn derive_contains(files: &[&str]) -> Vec<String> {
    let mut contains = Vec::new();
    let has = |predicate: &dyn Fn(&str) -> bool| files.iter().any(|path| predicate(path));
    if has(&|path| path.starts_with("skills/") || path == "skills") {
        contains.push("skills".into());
    }
    if has(&|path| path.ends_with("/.mcp.json") || path == ".mcp.json") {
        contains.push("mcp".into());
    }
    if has(&|path| path.ends_with("/.app.json") || path == ".app.json") {
        contains.push("app".into());
    }
    if has(&|path| path.ends_with("/hooks.json") || path == "hooks.json") {
        contains.push("hooks".into());
    }
    if has(&|path| path.starts_with("agents/")) {
        contains.push("agents".into());
    }
    if has(&|path| path.starts_with("commands/")) {
        contains.push("commands".into());
    }
    contains
}

/// 相对路径安全落盘：只允许普通路径段，拒绝绝对路径与 `..` 穿越。
/// （CLI 通道不再需要；保留供将来本地缓存路径校验复用）
#[cfg(test)]
fn safe_join(base: &Path, relative: &str) -> Option<PathBuf> {
    let relative_path = Path::new(relative);
    if !relative_path.is_relative() {
        return None;
    }
    let mut destination = base.to_path_buf();
    for component in relative_path.components() {
        match component {
            std::path::Component::Normal(part) => destination.push(part),
            _ => return None,
        }
    }
    Some(destination)
}

fn store_contains(plugin_dir: &Path) -> Vec<String> {
    let files = walk_files(plugin_dir);
    let relative: Vec<&str> = files.iter().map(|path| path.as_str()).collect();
    derive_contains(&relative)
}

/// 列出插件目录内全部文件的目录内相对路径（插件体量小，直接递归）。
fn walk_files(root: &Path) -> Vec<String> {
    fn visit(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(base, &path, out);
            } else if let Ok(relative) = path.strip_prefix(base) {
                out.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut files = Vec::new();
    visit(root, root, &mut files);
    files
}

/// 读取 `~/.agents/.skill-lock.json`（Codex 的 Skill 安装注册表，实测布局）。
fn read_skill_lock(home: &Path) -> Vec<PluginSummary> {
    let Ok(text) = std::fs::read_to_string(home.join(SKILL_LOCK_RELATIVE_PATH)) else {
        return Vec::new();
    };
    let Ok(lock) = serde_json::from_str::<SkillLock>(&text) else {
        return Vec::new();
    };
    lock.skills
        .into_iter()
        .map(|(name, entry)| PluginSummary {
            name: name.clone(),
            contains: vec!["skills".into()],
            enabled: true,
            origin: "skill".into(),
            store_path: home.join(".agents/skills").join(name).display().to_string(),
            source_url: match (entry.source_url, entry.source) {
                (Some(url), _) => Some(url),
                (None, Some(source)) => Some(format!("https://github.com/{source}")),
                (None, None) => None,
            },
            installed_at: entry.installed_at.as_deref().and_then(parse_rfc3339_ms),
            ..PluginSummary::default()
        })
        .collect()
}

/// CLI 缺席时的回退：扫 `~/.codex/plugins/cache/<marketplace>/<plugin>/<version>/`。
fn scan_codex_plugin_cache(codex_home: &Path) -> Vec<PluginSummary> {
    let cache_root = codex_home.join(PLUGIN_CACHE_RELATIVE_PATH);
    let Ok(marketplaces) = std::fs::read_dir(&cache_root) else {
        return Vec::new();
    };
    let mut summaries = Vec::new();
    for marketplace in marketplaces.flatten() {
        let marketplace_name = marketplace.file_name().to_string_lossy().to_string();
        let Ok(plugins) = std::fs::read_dir(marketplace.path()) else {
            continue;
        };
        for plugin in plugins.flatten() {
            let Ok(versions) = std::fs::read_dir(plugin.path()) else {
                continue;
            };
            let Some((version_dir_name, version_dir)) = versions
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| {
                    (
                        entry.file_name().to_string_lossy().to_string(),
                        entry.path(),
                    )
                })
                .max_by(|(left, _), (right, _)| left.cmp(right))
            else {
                continue;
            };
            let manifest = read_manifest(&version_dir);
            let plugin_dir_name = plugin.file_name().to_string_lossy().to_string();
            let mut summary = PluginSummary::from_parts(
                &plugin_dir_name,
                manifest.as_ref(),
                store_contains(&version_dir),
                "codex",
                &version_dir,
                None,
            );
            summary.version = Some(version_dir_name).or(summary.version);
            summary.marketplace = Some(marketplace_name.clone());
            summary.enabled = true;
            summaries.push(summary);
        }
    }
    summaries
}

// ==================== AppContext 服务 ====================

impl AppContext {
    /// 已安装列表：`codex plugin list`（主源，含启停）→ 插件缓存（CLI 缺席回退）
    /// → Skill 注册表 → 家目录 local 条目 → 暂存条目。
    pub async fn list_plugins(&self) -> AppResult<Vec<PluginSummary>> {
        let Some(home) = self.paths.agents_home.parent().map(Path::to_path_buf) else {
            return Ok(Vec::new());
        };
        let codex_home = self.paths.codex_home.clone();
        let state_path = self.paths.plugin_state.clone();
        let summaries = tauri::async_runtime::spawn_blocking(move || {
            list_plugins_sync(&home, &codex_home, &state_path)
        })
        .await
        .map_err(|error| app_err!("插件列表任务失败: {error}"))??;
        Ok(summaries)
    }

    /// 预览：仓库元数据 + 每个插件根的清单与文件列表（不落盘）。
    pub async fn preview_plugin(&self, url: &str) -> AppResult<PluginPreview> {
        let source = parse_github_url(url)?;
        let tree = fetch_repo_tree(&source).await?;
        let roots: Vec<String> = plugin_roots(&tree.entries)
            .into_iter()
            .filter(|root| root_within(root, source.sub_path.as_deref().unwrap_or_default()))
            .collect();
        if roots.is_empty() {
            return Err(app_err!(
                "仓库里没有找到 {MANIFEST_RELATIVE_PATH}，请确认这是一个 Codex 插件仓库"
            ));
        }

        let mut candidates = Vec::new();
        for root in roots {
            let files = files_under_root(&tree.entries, &root);
            let relative_manifest = if root.is_empty() {
                MANIFEST_RELATIVE_PATH.to_string()
            } else {
                format!("{root}/{MANIFEST_RELATIVE_PATH}")
            };
            let manifest_bytes =
                fetch_raw_file(&source, &tree.reference, &relative_manifest).await?;
            let manifest = parse_manifest_text(&String::from_utf8_lossy(&manifest_bytes))?;
            validate_plugin_name(&manifest.name)?;
            let contains = derive_contains(&files);
            candidates.push(PluginCandidate {
                sub_path: root,
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                display_name: manifest
                    .interface
                    .as_ref()
                    .and_then(|item| item.display_name.clone()),
                description: manifest.description.clone(),
                capabilities: manifest
                    .interface
                    .as_ref()
                    .map(|item| item.capabilities.clone())
                    .unwrap_or_default(),
                contains,
                files: files
                    .iter()
                    .map(|path| path.to_string())
                    .take(preview_file_limit())
                    .collect(),
            });
        }
        Ok(PluginPreview {
            repo: format!("{}/{}", source.owner, source.repo),
            reference: tree.reference,
            default_branch: tree.default_branch,
            candidates,
        })
    }

    /// 安装：预览拿到插件名 → `marketplace add <owner/repo[@ref]>` → `plugin add <名>@<市场>`。
    /// 全程官方 CLI 路径，落盘与状态归 Codex 管。
    pub async fn install_plugin(
        &self,
        url: &str,
        sub_path: Option<&str>,
    ) -> AppResult<PluginSummary> {
        let source = parse_github_url(url)?;
        let tree = fetch_repo_tree(&source).await?;
        let target_sub = sub_path.or(source.sub_path.as_deref());
        let roots: Vec<String> = plugin_roots(&tree.entries)
            .into_iter()
            .filter(|root| root_within(root, target_sub.unwrap_or_default()))
            .collect();
        let root = match roots.as_slice() {
            [root] => root.clone(),
            [] => {
                return Err(app_err!(
                    "仓库里没有找到 {MANIFEST_RELATIVE_PATH}，请确认地址指向 Codex 插件目录"
                ))
            }
            _ => {
                return Err(app_err!(
                    "该地址下有 {} 个插件，请先预览并选择具体插件",
                    roots.len()
                ))
            }
        };
        let relative_manifest = if root.is_empty() {
            MANIFEST_RELATIVE_PATH.to_string()
        } else {
            format!("{root}/{MANIFEST_RELATIVE_PATH}")
        };
        let manifest_bytes = fetch_raw_file(&source, &tree.reference, &relative_manifest).await?;
        let manifest = parse_manifest_text(&String::from_utf8_lossy(&manifest_bytes))?;
        validate_plugin_name(&manifest.name)?;

        let Some(home) = self.paths.agents_home.parent().map(Path::to_path_buf) else {
            return Err(app_err!("无法定位用户主目录"));
        };
        let source_arg = match &source.ref_name {
            Some(reference) => format!("{}/{}@{reference}", source.owner, source.repo),
            None => format!("{}/{}", source.owner, source.repo),
        };
        let selector_name = manifest.name.clone();
        // CLI 调用跑在 blocking 线程池（git 同步可能数十秒）
        let marketplace_name = tauri::async_runtime::spawn_blocking({
            let home = home.clone();
            let source_arg = source_arg.clone();
            move || {
                let output = run_codex_plugin(&home, &["marketplace", "add", &source_arg]);
                match output {
                    Ok(text) => parse_marketplace_name(&text, ""),
                    Err(error) => {
                        // 源可能已添加过（重复安装/升级）：忽略 add 错误，让 plugin add 兜底
                        let _ = error;
                        String::new()
                    }
                }
            }
        })
        .await
        .map_err(|error| app_err!("安装任务失败: {error}"))?;
        let marketplace_name = if marketplace_name.is_empty() {
            source.repo.clone()
        } else {
            marketplace_name
        };

        let selector = format!("{selector_name}@{marketplace_name}");
        tauri::async_runtime::spawn_blocking({
            let home = home.clone();
            let selector = selector.clone();
            move || run_codex_plugin(&home, &["add", &selector])
        })
        .await
        .map_err(|error| app_err!("安装任务失败: {error}"))??;

        {
            let _guard = self
                .operation
                .lock()
                .map_err(|_| app_err!("操作锁已损坏"))?;
            let mut sidecar = load_sidecar(&self.paths.plugin_state);
            sidecar.disabled.remove(&manifest.name);
            sidecar.origins.insert(
                manifest.name.clone(),
                PluginOrigin {
                    source_url: format!("https://github.com/{}/{}", source.owner, source.repo),
                    marketplace: marketplace_name.clone(),
                    version: manifest.version.clone(),
                    installed_at: now_ms() as i64,
                },
            );
            save_sidecar(&self.paths.plugin_state, &sidecar)?;
            let _ = self.database.record_event(
                None,
                "plugin",
                "install",
                Some(&format!("{selector}@{}", tree.reference)),
                &now_ms().to_string(),
            );
        }

        // 从列表里取回安装后的真实状态（版本/路径由 Codex 维护）
        let plugins = self.list_plugins().await?;
        plugins
            .into_iter()
            .find(|item| item.name == manifest.name)
            .ok_or_else(|| {
                app_err!(
                    "安装命令已执行，但列表里没找到「{}」，请刷新查看",
                    manifest.name
                )
            })
    }

    /// 卸载：codex/cgswitch 来源走 `codex plugin remove`；外部条目只摘条目（文件不动）。
    pub async fn uninstall_plugin(&self, name: &str) -> AppResult<()> {
        validate_plugin_name(name)?;
        let plugins = self.list_plugins().await?;
        let Some(plugin) = plugins.iter().find(|item| item.name == name) else {
            return Err(app_err!("没有找到名为「{name}」的插件"));
        };
        if plugin.origin == "official" || plugin.origin == "skill" {
            return Err(app_err!(
                "「{name}」属于 Codex 官方市场 / Skill 注册表，请在 Codex 内管理它"
            ));
        }
        if plugin.origin == "codex" || plugin.origin == "cgswitch" {
            let Some(home) = self.paths.agents_home.parent().map(Path::to_path_buf) else {
                return Err(app_err!("无法定位用户主目录"));
            };
            let marketplace = plugin
                .marketplace
                .clone()
                .ok_or_else(|| app_err!("缺少「{name}」的市场信息，无法调用卸载"))?;
            let selector = format!("{name}@{marketplace}");
            tauri::async_runtime::spawn_blocking({
                let home = home.clone();
                let selector = selector.clone();
                move || run_codex_plugin(&home, &["remove", &selector])
            })
            .await
            .map_err(|error| app_err!("卸载任务失败: {error}"))??;
            let _guard = self
                .operation
                .lock()
                .map_err(|_| app_err!("操作锁已损坏"))?;
            let mut sidecar = load_sidecar(&self.paths.plugin_state);
            sidecar.origins.remove(name);
            sidecar.disabled.remove(name);
            save_sidecar(&self.paths.plugin_state, &sidecar)?;
            let _ = self.database.record_event(
                None,
                "plugin",
                "uninstall",
                Some(name),
                &now_ms().to_string(),
            );
            return Ok(());
        }
        // personal / claude / cursor：摘条目并暂存
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        self.detach_external_entry(name)?;
        let _ = self.database.record_event(
            None,
            "plugin",
            "uninstall",
            Some(name),
            &now_ms().to_string(),
        );
        Ok(())
    }

    /// 禁用/启用：仅家目录条目支持（Codex CLI 未提供启停命令，缓存类插件请在 Codex 内操作）。
    pub fn set_plugin_enabled(&self, name: &str, enabled: bool) -> AppResult<()> {
        let _guard = self
            .operation
            .lock()
            .map_err(|_| app_err!("操作锁已损坏"))?;
        validate_plugin_name(name)?;
        if enabled {
            self.restore_stashed_entry(name)
        } else {
            self.detach_external_entry(name)
        }
    }

    // ==================== 外部条目操作 ====================

    /// 摘除外部条目并暂存（文件不动，可经 set_plugin_enabled 恢复）。
    fn detach_external_entry(&self, name: &str) -> AppResult<()> {
        let Some(home) = self.paths.agents_home.parent() else {
            return Err(app_err!("无法定位用户主目录"));
        };
        for (layout, _, _) in HOME_MARKETPLACE_LAYOUTS {
            let path = home.join(layout);
            if !path.is_file() {
                continue;
            }
            if let Some(entry) = take_entry_from(self, &path, name)? {
                let mut sidecar = load_sidecar(&self.paths.plugin_state);
                sidecar.disabled.insert(
                    name.to_string(),
                    StashedEntry {
                        marketplace: (*layout).to_string(),
                        entry,
                    },
                );
                save_sidecar(&self.paths.plugin_state, &sidecar)?;
                return Ok(());
            }
        }
        Err(app_err!(
            "「{name}」不是家目录 local 条目；Codex 市场插件的启停请在 Codex 内操作"
        ))
    }

    /// 从暂存恢复外部条目到其原属 marketplace。
    fn restore_stashed_entry(&self, name: &str) -> AppResult<()> {
        let Some(home) = self.paths.agents_home.parent() else {
            return Err(app_err!("无法定位用户主目录"));
        };
        let mut sidecar = load_sidecar(&self.paths.plugin_state);
        let Some(stashed) = sidecar.disabled.remove(name) else {
            return Err(app_err!("没有找到「{name}」的可恢复登记条目"));
        };
        let marketplace_path = home.join(&stashed.marketplace);
        if let Some(parent) = marketplace_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| app_err!("无法创建目录 {}: {error}", parent.display()))?;
        }
        restore_entry_to(self, &marketplace_path, stashed.entry)?;
        save_sidecar(&self.paths.plugin_state, &sidecar)
    }
}

/// list_plugins 的同步实现（跑在 blocking 线程池）。
fn list_plugins_sync(
    home: &Path,
    codex_home: &Path,
    state_path: &Path,
) -> AppResult<Vec<PluginSummary>> {
    let sidecar = load_sidecar(state_path);
    let marketplaces = discover_marketplaces(home);
    let mut summaries: Vec<PluginSummary> = Vec::new();
    let mut seen_names: Vec<String> = Vec::new();

    // 1) `codex plugin list`（主源）：覆盖运行时/捆绑/第三方市场，含启停状态
    if find_codex_cli(home).is_some() {
        if let Ok(output) = run_codex_plugin(home, &["list"]) {
            for (name, marketplace, enabled, version, path) in parse_plugin_list_output(&output) {
                let origin = if sidecar.origins.contains_key(&name) {
                    "cgswitch"
                } else if marketplace.starts_with("openai") {
                    "official"
                } else {
                    "codex"
                };
                let plugin_path = if path.is_empty() {
                    codex_home
                        .join(PLUGIN_CACHE_RELATIVE_PATH)
                        .join(&marketplace)
                        .join(&name)
                } else {
                    PathBuf::from(&path)
                };
                let manifest = read_manifest(&plugin_path);
                summaries.push(PluginSummary {
                    version: version.or(manifest.as_ref().and_then(|item| item.version.clone())),
                    display_name: manifest
                        .as_ref()
                        .and_then(|item| item.interface.as_ref())
                        .and_then(|item| item.display_name.clone()),
                    description: manifest.as_ref().and_then(|item| item.description.clone()),
                    category: manifest
                        .as_ref()
                        .and_then(|item| item.interface.as_ref())
                        .and_then(|item| item.category.clone()),
                    capabilities: manifest
                        .as_ref()
                        .and_then(|item| item.interface.as_ref())
                        .map(|item| item.capabilities.clone())
                        .unwrap_or_default(),
                    contains: if plugin_path.is_dir() {
                        store_contains(&plugin_path)
                    } else {
                        Vec::new()
                    },
                    enabled,
                    origin: origin.to_string(),
                    marketplace: Some(marketplace),
                    store_path: plugin_path.display().to_string(),
                    source_url: sidecar
                        .origins
                        .get(&name)
                        .map(|item| item.source_url.clone()),
                    installed_at: sidecar.origins.get(&name).map(|item| item.installed_at),
                    name,
                });
                seen_names.push(summaries.last().unwrap().name.clone());
            }
        } else {
            // CLI 在但 list 失败：回退缓存扫描
            summaries.extend(scan_codex_plugin_cache(codex_home));
        }
    } else {
        summaries.extend(scan_codex_plugin_cache(codex_home));
    }
    for summary in &summaries {
        seen_names.push(summary.name.clone());
    }

    // 2) Skill 注册表（只读）
    for mut skill in read_skill_lock(home) {
        if seen_names.contains(&skill.name) {
            continue;
        }
        seen_names.push(skill.name.clone());
        skill.marketplace = None;
        summaries.push(skill);
    }

    // 3) 家目录 local 条目（不在 Codex 市场里的手动登记）
    for marketplace in &marketplaces {
        let Some(plugins) = marketplace
            .document
            .get("plugins")
            .and_then(Value::as_array)
        else {
            continue;
        };
        for entry in plugins {
            let Some(name) = entry.get("name").and_then(Value::as_str) else {
                continue;
            };
            if seen_names.iter().any(|seen| seen == name) {
                continue;
            }
            let Some(raw_path) = entry_local_path(entry) else {
                continue;
            };
            let Some(path) = resolve_local_path(home, &raw_path) else {
                continue;
            };
            seen_names.push(name.to_string());
            let manifest = read_manifest(&path);
            let mut summary = PluginSummary::from_parts(
                name,
                manifest.as_ref(),
                store_contains(&path),
                marketplace.label,
                &path,
                None,
            );
            summary.enabled = true;
            summaries.push(summary);
        }
    }

    // 4) 暂存的外部条目：文件仍在原处，条目已摘除
    for (name, stashed) in &sidecar.disabled {
        if seen_names.contains(name) {
            continue;
        }
        let label = HOME_MARKETPLACE_LAYOUTS
            .iter()
            .find(|(layout, _, _)| layout == &stashed.marketplace)
            .map(|(_, label, _)| *label)
            .unwrap_or("personal");
        let path = entry_local_path(&stashed.entry).and_then(|raw| resolve_local_path(home, &raw));
        let manifest = path.as_deref().and_then(read_manifest);
        let contains = path.as_deref().map(store_contains).unwrap_or_default();
        let mut summary = PluginSummary::from_parts(
            name,
            manifest.as_ref(),
            contains,
            label,
            path.as_deref().unwrap_or(Path::new("")),
            None,
        );
        summary.enabled = false;
        summaries.push(summary);
    }

    summaries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> TreeEntry {
        TreeEntry {
            path: path.to_string(),
            kind: "blob".into(),
        }
    }

    #[test]
    fn plugin_roots_find_nested_and_root_plugins() {
        let entries = vec![
            entry(".codex-plugin/plugin.json"),
            entry("skills/a/SKILL.md"),
            entry("plugins/foo/.codex-plugin/plugin.json"),
            entry("plugins/foo/skills/b/SKILL.md"),
            entry("plugins/foo/node_modules/x/.codex-plugin/plugin.json"),
        ];
        let roots = plugin_roots(&entries);
        assert_eq!(
            roots,
            vec![
                "".to_string(),
                "plugins/foo".to_string(),
                "plugins/foo/node_modules/x".to_string()
            ]
        );
    }

    #[test]
    fn root_within_matches_self_and_children_only() {
        assert!(root_within("plugins/foo", "plugins"));
        assert!(root_within("plugins/foo", "plugins/foo"));
        assert!(!root_within("plugins/foobar", "plugins/foo"));
        assert!(root_within("anything", ""));
    }

    #[test]
    fn derive_contains_labels_content_types() {
        let files = vec!["skills/a/SKILL.md", ".mcp.json", "hooks.json"];
        assert_eq!(derive_contains(&files), vec!["skills", "mcp", "hooks"]);
    }

    #[test]
    fn safe_join_rejects_traversal() {
        assert!(safe_join(Path::new("/base"), "skills/a.md").is_some());
        assert!(safe_join(Path::new("/base"), "../escape").is_none());
        assert!(safe_join(Path::new("/base"), "a/../../escape").is_none());
        assert!(safe_join(Path::new("/base"), "/absolute").is_none());
    }

    #[test]
    fn resolve_local_path_accepts_prefix_and_cursor_style() {
        let home = Path::new("/home/user");
        assert_eq!(
            resolve_local_path(home, "./plugins/foo"),
            Some(home.join("plugins/foo"))
        );
        assert_eq!(
            resolve_local_path(home, "plugins/foo"),
            Some(home.join("plugins/foo"))
        );
        assert_eq!(resolve_local_path(home, "../escape"), None);
        assert_eq!(resolve_local_path(home, "/absolute"), None);
    }

    #[test]
    fn entry_local_path_supports_string_and_object_forms() {
        let string_form = json!({ "name": "a", "source": "./plugins/a" });
        assert_eq!(
            entry_local_path(&string_form).as_deref(),
            Some("./plugins/a")
        );
        let object_form =
            json!({ "name": "b", "source": { "source": "local", "path": "./plugins/b" } });
        assert_eq!(
            entry_local_path(&object_form).as_deref(),
            Some("./plugins/b")
        );
        let url_form = json!({ "name": "c", "source": { "source": "url", "url": "https://github.com/x/y.git" } });
        assert_eq!(entry_local_path(&url_form), None);
    }

    #[test]
    fn manifest_parses_official_sample() {
        let text = r#"{
            "name": "memory-bank",
            "version": "1.0.0",
            "description": "Memory management",
            "interface": { "displayName": "Memory Bank", "category": "memory", "capabilities": ["read", "write"] }
        }"#;
        let manifest = parse_manifest_text(text).unwrap();
        assert_eq!(manifest.name, "memory-bank");
        assert_eq!(manifest.version.as_deref(), Some("1.0.0"));
        assert_eq!(
            manifest.interface.unwrap().display_name.as_deref(),
            Some("Memory Bank")
        );
    }

    #[test]
    fn plugin_list_output_parses_status_and_versions() {
        let output = "\
Marketplace `openai-bundled`
C:\\users\\\\.codex\\.tmp\\bundled-marketplaces\\openai-bundled\\.agents\\plugins\\marketplace.json

PLUGIN                           STATUS              VERSION       PATH
codex-app-tools@openai-bundled  installed, enabled  0.1.0         C:\\bundle\\plugins\\codex-app-tools
latex@openai-bundled            not installed                     C:\\bundle\\plugins\\latex
ponytail@ponytail               installed, disabled 4.9.0         C:\\cache\\ponytail\\ponytail\\4.9.0
";
        let items = parse_plugin_list_output(output);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0],
            (
                "codex-app-tools".into(),
                "openai-bundled".into(),
                true,
                Some("0.1.0".into()),
                "C:\\bundle\\plugins\\codex-app-tools".into()
            )
        );
        assert!(!items[1].2);
        assert_eq!(items[1].1, "ponytail");
    }

    #[test]
    fn marketplace_name_falls_back_to_repo() {
        assert_eq!(
            parse_marketplace_name("added Marketplace `ponytail` ok", "fallback"),
            "ponytail"
        );
        assert_eq!(
            parse_marketplace_name("no backticks here", "my-repo"),
            "my-repo"
        );
    }

    #[test]
    fn plugin_name_rejects_selector_characters() {
        assert!(validate_plugin_name("memory-bank").is_ok());
        assert!(validate_plugin_name("a@b").is_err());
        assert!(validate_plugin_name("../x").is_err());
    }

    fn context() -> (tempfile::TempDir, AppContext) {
        let home = tempfile::tempdir().unwrap();
        let paths = crate::paths::from_home(home.path()).unwrap();
        paths.ensure().unwrap();
        let context = AppContext::new(paths).unwrap();
        (home, context)
    }

    fn write_external_plugin(home: &Path, name: &str) -> PathBuf {
        let dir = home.join("my-plugins").join(name).join(".codex-plugin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            format!(r#"{{"name":"{name}","version":"2.0.0","description":"外部插件"}}"#),
        )
        .unwrap();
        dir.parent().unwrap().to_path_buf()
    }

    fn write_marketplace(home: &Path, layout: &str, plugins_json: &str) {
        let path = home.join(layout);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            format!(r#"{{"name":"m","plugins":[{plugins_json}]}}"#),
        )
        .unwrap();
    }

    fn relative_posix(home: &Path, path: &Path) -> String {
        path.strip_prefix(home)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/")
    }

    #[tokio::test]
    async fn list_plugins_reads_skill_lock_and_cache_fallback() {
        let (home, context) = context();
        // 无 codex CLI 的环境（CI）：走缓存回退
        std::fs::create_dir_all(home.path().join(".agents/skills/lark-base")).unwrap();
        std::fs::write(
            home.path().join(".agents/.skill-lock.json"),
            r#"{"version":3,"skills":{"lark-base":{"source":"larksuite/cli","sourceType":"github","sourceUrl":"https://github.com/larksuite/cli.git","skillPath":"skills/lark-base/SKILL.md","skillFolderHash":"abc","installedAt":"2026-05-09T10:06:04.288Z"}}}"#,
        )
        .unwrap();
        let cache_dir = home
            .path()
            .join(".codex")
            .join("plugins")
            .join("cache")
            .join("ponytail")
            .join("ponytail")
            .join("4.9.0")
            .join(".codex-plugin");
        std::fs::create_dir_all(&cache_dir).unwrap();
        std::fs::write(
            cache_dir.join("plugin.json"),
            r#"{"name":"ponytail","description":"Ponytail 插件"}"#,
        )
        .unwrap();

        let plugins = context.list_plugins().await.unwrap();
        let skill = plugins
            .iter()
            .find(|item| item.name == "lark-base")
            .unwrap();
        assert_eq!(skill.origin, "skill");
        assert!(skill.enabled);
        assert_eq!(
            skill.source_url.as_deref(),
            Some("https://github.com/larksuite/cli.git")
        );

        let ponytail = plugins.iter().find(|item| item.name == "ponytail").unwrap();
        assert_eq!(ponytail.origin, "codex");
        assert_eq!(ponytail.marketplace.as_deref(), Some("ponytail"));
        assert_eq!(ponytail.version.as_deref(), Some("4.9.0"));
    }

    #[tokio::test]
    async fn readonly_plugins_reject_uninstall() {
        let (home, context) = context();
        std::fs::create_dir_all(home.path().join(".agents/skills")).unwrap();
        std::fs::write(
            home.path().join(".agents/.skill-lock.json"),
            r#"{"version":3,"skills":{"lark-base":{"source":"larksuite/cli","sourceType":"github"}}}"#,
        )
        .unwrap();
        let error = context.uninstall_plugin("lark-base").await.unwrap_err();
        assert!(error.0.contains("官方市场 / Skill 注册表"));
    }

    #[tokio::test]
    async fn list_plugins_discovers_external_entries_across_layouts() {
        let (home, context) = context();
        let external_dir = write_external_plugin(home.path(), "handmade");
        write_marketplace(
            home.path(),
            ".agents/plugins/marketplace.json",
            &format!(
                r#"{{"name":"a","source":{{"source":"local","path":"./{}"}}}}"#,
                relative_posix(home.path(), &external_dir)
            ),
        );
        let claude_dir = write_external_plugin(home.path(), "claude-plugin");
        write_marketplace(
            home.path(),
            ".claude-plugin/marketplace.json",
            &format!(
                r#"{{"name":"c","source":"./{}"}}"#,
                relative_posix(home.path(), &claude_dir)
            ),
        );

        let plugins = context.list_plugins().await.unwrap();
        let handmade = plugins.iter().find(|item| item.name == "handmade").unwrap();
        assert_eq!(handmade.origin, "personal");
        assert!(handmade.enabled);
        assert_eq!(handmade.version.as_deref(), Some("2.0.0"));
        let claude = plugins
            .iter()
            .find(|item| item.name == "claude-plugin")
            .unwrap();
        assert_eq!(claude.origin, "claude");
    }

    #[test]
    fn external_disable_stashes_and_enable_restores() {
        let (home, context) = context();
        let external_dir = write_external_plugin(home.path(), "handmade");
        let relative = relative_posix(home.path(), &external_dir);
        write_marketplace(
            home.path(),
            ".claude-plugin/marketplace.json",
            &format!(r#"{{"name":"handmade","source":"./{relative}"}}"#),
        );

        context.set_plugin_enabled("handmade", false).unwrap();
        // 文件必须原地保留
        assert!(external_dir
            .join(".codex-plugin")
            .join("plugin.json")
            .is_file());
        let marketplace_text =
            std::fs::read_to_string(home.path().join(".claude-plugin/marketplace.json")).unwrap();
        assert!(!marketplace_text.contains("handmade"));

        context.set_plugin_enabled("handmade", true).unwrap();
        let marketplace_text =
            std::fs::read_to_string(home.path().join(".claude-plugin/marketplace.json")).unwrap();
        assert!(marketplace_text.contains("handmade"));
    }

    #[test]
    fn set_enabled_rejects_marketplace_plugins_without_entry() {
        let (_home, context) = context();
        let error = context.set_plugin_enabled("whatever", false).unwrap_err();
        assert!(error.0.contains("家目录 local 条目"));
    }
}
