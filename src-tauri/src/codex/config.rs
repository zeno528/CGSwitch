use std::collections::BTreeMap;
use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::error::{app_err, AppResult};
use crate::models::ProfilePayload;

pub fn parse_document(text: &str) -> AppResult<DocumentMut> {
    text.parse::<DocumentMut>()
        .map_err(|error| app_err!("Codex 配置不是有效 TOML: {error}"))
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
            model_values.insert(key.to_string(), item.to_string());
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
}
