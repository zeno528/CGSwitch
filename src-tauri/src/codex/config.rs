use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use toml_edit::{Decor, DocumentMut, Item, Table, Value};

use crate::error::{app_err, AppResult};
use crate::models::ProfilePayload;

pub fn parse_document(text: &str) -> AppResult<DocumentMut> {
    text.parse::<DocumentMut>()
        .map_err(|error| app_err!("Codex 配置不是有效 TOML: {error}"))
}

#[derive(Debug, Clone, Serialize)]
pub struct TomlDiagnostic {
    pub from: usize,
    pub to: usize,
    pub message: String,
}

fn utf16_offset(text: &str, byte_offset: usize) -> usize {
    text.get(..byte_offset)
        .unwrap_or(text)
        .encode_utf16()
        .count()
}

/// 校验 TOML 文本，返回全部语法错误的 UTF-16 偏移区间（CodeMirror 以 UTF-16 定位文档）。
/// taplo 解析器自带错误恢复，一次解析即可拿全所有错误；上限 100 条防御异常输入刷屏。
pub fn validate_document(text: &str) -> Vec<TomlDiagnostic> {
    let errors: Vec<_> = taplo::parser::parse(text)
        .errors
        .into_iter()
        .take(100)
        .map(|error| {
            (
                usize::from(error.range.start()).min(text.len()),
                usize::from(error.range.end()).min(text.len()),
                error.message,
            )
        })
        .collect();

    // 相接/重叠的错误链合并为一条：单点错误（如字符串缺闭合引号）会让恢复式解析
    // 在后续文本上报一串连锁错误；独立错误之间必有间隙，不受影响。
    let mut merged: Vec<(usize, usize, String)> = Vec::new();
    for (start, end, message) in errors {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 + 1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end, message));
    }

    merged
        .into_iter()
        .map(|(from, to, message)| TomlDiagnostic {
            from: utf16_offset(text, from),
            to: utf16_offset(text, to),
            message,
        })
        .collect()
}

/// 格式化 TOML 文本；taplo 会跳过含语法错误的区间，坏文档也能尽量保持可格式化。
pub fn format_document(text: &str) -> String {
    taplo::formatter::format(text, taplo::formatter::Options::default())
}

pub fn patch_context_override(text: &str, enabled: bool) -> AppResult<String> {
    let mut document = parse_document(text)?;
    if enabled {
        document.as_table_mut().insert(
            "model_context_window",
            Item::Value(Value::from(1_000_000_i64)),
        );
        document.as_table_mut().insert(
            "model_auto_compact_token_limit",
            Item::Value(Value::from(900_000_i64)),
        );
    } else {
        document.as_table_mut().remove("model_context_window");
        document
            .as_table_mut()
            .remove("model_auto_compact_token_limit");
    }
    Ok(document.to_string())
}

pub fn read_profile(path: &Path) -> AppResult<ProfilePayload> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| app_err!("无法读取 {}: {error}", path.display()))?;
    capture_from_document(&parse_document(&text)?)
}

pub fn capture_from_document(document: &DocumentMut) -> AppResult<ProfilePayload> {
    let mut model_values = BTreeMap::new();
    for (key, item) in document.as_table().iter() {
        if is_model_key(key) && item.is_value() {
            // 清掉源码装饰（前导空格、行内注释等）只留纯 "value" 形式：
            // apply 侧 parse_value 依赖它 re-parse，前端显示依赖 stripTomlQuotes 剥引号
            let mut value = item.as_value().expect("is_value 已检查").clone();
            *value.decor_mut() = Decor::default();
            model_values.insert(key.to_string(), value.to_string());
        }
    }

    let provider_id = document
        .as_table()
        .get("model_provider")
        .and_then(Item::as_str)
        .map(str::to_string);
    let provider_body = provider_id.as_deref().and_then(|id| {
        document
            .as_table()
            .get("model_providers")
            .and_then(Item::as_table)
            .and_then(|providers| providers.get(id))
            .and_then(Item::as_table)
            .map(Table::to_string)
    });

    Ok(ProfilePayload {
        model_values,
        provider_id,
        provider_body,
        builtin: None,
        ..Default::default()
    })
}

pub fn apply_to_document(document: &mut DocumentMut, payload: &ProfilePayload) -> AppResult<()> {
    let stale_keys: Vec<String> = document
        .as_table()
        .iter()
        .filter(|(key, item)| is_model_key(key) && item.is_value())
        .map(|(key, _)| key.to_string())
        .collect();
    for key in stale_keys {
        document.as_table_mut().remove(&key);
    }

    for (key, raw) in &payload.model_values {
        let value = parse_value(raw)?;
        document.as_table_mut().insert(key, Item::Value(value));
    }

    if let (Some(provider_id), Some(provider_body)) = (&payload.provider_id, &payload.provider_body)
    {
        let providers = document
            .as_table_mut()
            .entry("model_providers")
            .or_insert_with(|| Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| app_err!("model_providers 不是 TOML table"))?;
        providers.remove(provider_id);

        let parsed_body: DocumentMut = provider_body
            .parse()
            .map_err(|_| app_err!("供应商配置中的 provider 数据无效"))?;
        let mut provider = Table::new();
        for (key, item) in parsed_body.as_table() {
            let item = item.clone();
            provider.insert(key, item);
        }
        providers.insert(provider_id, Item::Table(provider));
    }

    Ok(())
}

fn is_model_key(key: &str) -> bool {
    key == "model" || (key.starts_with("model_") && key != "model_providers")
}

fn parse_value(raw: &str) -> AppResult<Value> {
    raw.trim()
        .parse::<Value>()
        .map_err(|_| app_err!("供应商配置中的模型值无效"))
}

pub fn update_provider_body(
    body: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> AppResult<String> {
    let mut document: DocumentMut = body
        .parse()
        .map_err(|_| app_err!("供应商配置中的 provider 数据无效"))?;
    if let Some(value) = base_url {
        set_table_value(&mut document, "base_url", value);
    }
    if let Some(value) = api_key {
        set_table_value(&mut document, "experimental_bearer_token", value);
    }
    Ok(document.to_string())
}

/// 在已解析的 live 配置文档中就地更新 provider 表的 base_url / 密钥字段。
pub fn update_provider_in_document(
    document: &mut DocumentMut,
    provider_id: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> AppResult<()> {
    let providers = document
        .as_table_mut()
        .entry("model_providers")
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| app_err!("model_providers 不是 TOML table"))?;
    let provider = providers
        .entry(provider_id)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| app_err!("model_providers.{provider_id} 不是 TOML table"))?;

    let set = |table: &mut Table, key: &str, value: Option<&str>| {
        if let Some(value) = value {
            if value.trim().is_empty() {
                table.remove(key);
            } else {
                table.insert(key, Item::Value(Value::from(value.trim().to_string())));
            }
        }
    };
    set(provider, "base_url", base_url);
    set(provider, "experimental_bearer_token", api_key);
    Ok(())
}

fn set_table_value(document: &mut DocumentMut, key: &str, value: &str) {
    if value.trim().is_empty() {
        document.remove(key);
    } else {
        let parsed = Value::from(value.trim().to_string());
        document.insert(key, Item::Value(parsed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_provider_body_sets_and_removes_fields() {
        let body = r#"
name = "ZAI"
base_url = "https://old.example"
experimental_bearer_token = "old"
"#;
        let updated =
            update_provider_body(body, Some("https://api.z.ai"), Some("new-key")).unwrap();
        assert!(updated.contains(r#"base_url = "https://api.z.ai""#));
        assert!(updated.contains(r#"experimental_bearer_token = "new-key""#));

        let cleared = update_provider_body(&updated, None, Some("")).unwrap();
        assert!(!cleared.contains("experimental_bearer_token"));
        assert!(cleared.contains(r#"base_url = "https://api.z.ai""#));
    }

    #[test]
    fn patch_context_override_updates_only_the_two_root_keys() {
        let source = r#"
model = "gpt-5.6"
model_context_window = 272000
model_auto_compact_token_limit = 200000

[features]
# keep this comment
goals = true
"#;

        let enabled = patch_context_override(source, true).unwrap();
        assert!(enabled.contains("model_context_window = 1000000"));
        assert!(enabled.contains("model_auto_compact_token_limit = 900000"));
        assert!(enabled.contains("# keep this comment"));
        assert_eq!(enabled.matches("model_context_window").count(), 1);
        assert_eq!(enabled.matches("model_auto_compact_token_limit").count(), 1);

        let disabled = patch_context_override(&enabled, false).unwrap();
        assert!(!disabled.contains("model_context_window"));
        assert!(!disabled.contains("model_auto_compact_token_limit"));
        assert!(disabled.contains("# keep this comment"));
    }

    const SOURCE: &str = r#"
model = "glm-5.3"
model_provider = "ZAI"
model_reasoning_effort = "high"
model_catalog_json = "zai.json"

[features]
# user comment stays
goals = true

[mcp_servers.test]
command = "node"

[model_providers.ZAI]
name = "ZAI"
base_url = "https://api.z.ai"
wire_api = "responses"
experimental_bearer_token = "secret"

[model_providers.Old]
name = "Old"
"#;

    #[test]
    fn capture_and_apply_preserves_unrelated_configuration() {
        let mut document = parse_document(SOURCE).unwrap();
        let payload = capture_from_document(&document).unwrap();
        document.as_table_mut().remove("model_catalog_json");

        apply_to_document(&mut document, &payload).unwrap();
        let text = document.to_string();

        assert!(text.contains("# user comment stays"));
        assert!(text.contains("glm-5.3"));
        assert!(text.contains("model_catalog_json"));
        assert!(text.contains("[mcp_servers.test]"));
        assert!(text.contains("[model_providers.Old]"));
        assert_eq!(text.matches("experimental_bearer_token").count(), 1);
    }

    #[test]
    fn apply_removes_stale_model_keys() {
        let mut document = parse_document(SOURCE).unwrap();
        document.as_table_mut().remove("model_catalog_json");
        let payload = capture_from_document(&document).unwrap();
        document
            .as_table_mut()
            .insert("model_stale", Item::Value("yes".into()));

        apply_to_document(&mut document, &payload).unwrap();

        assert!(!document.to_string().contains("model_stale"));
    }

    #[test]
    fn validate_document_reports_invalid_toml_range() {
        let source = "name = \"ZAI\"\n[features]\ngoals =\n";
        let diagnostics = validate_document(source);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].from <= diagnostics[0].to);
        assert!(diagnostics[0].to <= source.len());
        assert!(!diagnostics[0].message.is_empty());
    }

    #[test]
    fn validate_document_uses_utf16_offsets_for_editor() {
        let source = "# 🦄\nname = @\n";
        let diagnostics = validate_document(source);
        let prefix = "# 🦄\nname = ";

        assert_eq!(diagnostics[0].from, prefix.encode_utf16().count());
    }

    #[test]
    fn validate_document_reports_multiple_invalid_toml_ranges() {
        let source = "first =\nsecond = @\n";
        let diagnostics = validate_document(source);

        // taplo 错误恢复式解析：两处错误都被定位（`first =` 行尾 7、`@` 字符 17）
        assert!(diagnostics.len() >= 2, "{diagnostics:?}");
        assert!(
            diagnostics.iter().any(|d| d.from == 7),
            "应定位到第一个错误：{diagnostics:?}"
        );
        assert!(
            diagnostics.iter().any(|d| d.from == 17),
            "应定位到 @ 字符：{diagnostics:?}"
        );
    }

    #[test]
    fn validate_document_does_not_repeat_one_toml_error() {
        let source = "items = [\"a\" \"b\"]\n";
        let diagnostics = validate_document(source);

        let mut ranges = diagnostics
            .iter()
            .map(|d| (d.from, d.to))
            .collect::<Vec<_>>();
        ranges.sort_unstable();
        ranges.dedup();
        assert_eq!(ranges.len(), diagnostics.len(), "同一区间不应重复报告");
        assert!(diagnostics.len() <= 10, "单点错误不应刷屏：{diagnostics:?}");
    }

    #[test]
    fn validate_document_missing_quote_does_not_cascade() {
        // 真实样本：仅第一个 trusted_hash 缺结尾引号，其余两个 section 完整
        let source = concat!(
            r#"[hooks.state."ponytail@ponytail:hooks/claude-codex-hooks.json:session_start:0:0"]"#,
            "\n",
            r#"trusted_hash = "sha256:5f81d38f47448a1581c08ec877e044d9e04dd6f814dce3f2671f7a8edadd719b"#,
            "\n\n",
            r#"[hooks.state."ponytail@ponytail:hooks/claude-codex-hooks.json:user_prompt_submit:0:0"]"#,
            "\n",
            r#"trusted_hash = "sha256:6a6f42bc3b58d6262db38bfd74d7f340fcca2b09cdb134aad365063f0bfefca4""#,
            "\n\n",
            r#"[hooks.state."ponytail@ponytail:hooks/claude-codex-hooks.json:subagent_start:0:0"]"#,
            "\n",
            r#"trusted_hash = "sha256:1423b56c1322f96c8f74c51c1e7ae9a047b904c1fa43ee9165d462fd7a6e70ef""#,
            "\n",
        );
        let diagnostics = validate_document(source);

        // 单点标点缺失只报本地错误：恢复式解析不应让后续合法 section 级联报错
        assert!(
            diagnostics.len() <= 3,
            "缺一个引号不应级联刷屏：{diagnostics:?}"
        );
        assert!(
            diagnostics
                .iter()
                .all(|d| d.from < source.len() && d.to <= source.len()),
            "位置应落在文档内：{diagnostics:?}"
        );
    }

    #[test]
    fn capture_model_values_strips_decor_and_roundtrips() {
        // 行内注释/前导空格属于源码装饰，不能进入 model_values（apply re-parse 与前端显示都会被污染）
        let source = r#"model = "glm-5.3" # 主模型
model_catalog_json = "~/.codex/a.json" # 目录
model_reasoning_effort = "high""#;
        let payload = capture_from_document(&parse_document(source).unwrap()).unwrap();
        assert_eq!(
            payload.model_values.get("model_catalog_json").unwrap(),
            "\"~/.codex/a.json\""
        );
        assert_eq!(payload.model_values.get("model").unwrap(), "\"glm-5.3\"");

        // round-trip：capture 的值必须能被 parse_value 重新解析（apply 回写路径）
        for raw in payload.model_values.values() {
            parse_value(raw).unwrap_or_else(|error| panic!("{} 无法 re-parse: {error}", raw));
        }
    }

    #[test]
    fn format_document_normalizes_spacing_and_survives_errors() {
        let formatted = format_document("a =1\n[table]\nb= 2\n");
        assert!(formatted.contains("a = 1"), "{formatted}");
        assert!(formatted.contains("b = 2"), "{formatted}");

        // 含语法错误时不 panic：taplo 跳过错误区间仍产出文本
        assert!(!format_document("a = @\n").is_empty());
    }
}
