use std::collections::BTreeMap;
use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

use crate::error::{app_err, AppResult};
use crate::models::ProfilePayload;

pub fn parse_document(text: &str) -> AppResult<DocumentMut> {
    text.parse::<DocumentMut>()
        .map_err(|error| app_err!("Codex 配置不是有效 TOML: {error}"))
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

    if provider_id.is_some() && provider_body.is_none() {
        return Err(app_err!(
            "当前 model_provider 在 model_providers 中不存在，未捕获档案"
        ));
    }

    Ok(ProfilePayload {
        model_values,
        provider_id,
        provider_body,
        builtin: None,
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
            .map_err(|_| app_err!("配置档案中的 provider 数据无效"))?;
        let mut provider = Table::new();
        for (key, item) in parsed_body.as_table() {
            let item = item.clone();
            provider.insert(key, item);
        }
        providers.insert(provider_id, Item::Table(provider));
    }

    Ok(())
}

pub fn matches_profile(document: &DocumentMut, payload: &ProfilePayload) -> AppResult<bool> {
    let current = capture_from_document(document)?;
    if current.provider_id != payload.provider_id {
        return Ok(false);
    }

    let mut current_values = BTreeMap::new();
    for (key, raw) in &current.model_values {
        current_values.insert(key.clone(), parse_value(raw)?.to_string());
    }
    let mut payload_values = BTreeMap::new();
    for (key, raw) in &payload.model_values {
        payload_values.insert(key.clone(), parse_value(raw)?.to_string());
    }
    if current_values != payload_values {
        return Ok(false);
    }

    Ok(normalize_provider(&current.provider_body)? == normalize_provider(&payload.provider_body)?)
}

/// 宽松匹配：档案的模型键必须是 live 配置的子集且值一致，
/// 允许 live 配置在使用过程中累计额外的模型键（如 model_catalog_json）。
pub fn subset_match(document: &DocumentMut, payload: &ProfilePayload) -> AppResult<bool> {
    let current = capture_from_document(document)?;
    if current.provider_id != payload.provider_id {
        return Ok(false);
    }
    for (key, raw) in &payload.model_values {
        let Some(live_raw) = current.model_values.get(key) else {
            return Ok(false);
        };
        if parse_value(live_raw)?.to_string() != parse_value(raw)?.to_string() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn is_model_key(key: &str) -> bool {
    key == "model" || (key.starts_with("model_") && key != "model_providers")
}

fn parse_value(raw: &str) -> AppResult<Value> {
    raw.trim()
        .parse::<Value>()
        .map_err(|_| app_err!("配置档案中的模型值无效"))
}

fn normalize_provider(body: &Option<String>) -> AppResult<BTreeMap<String, String>> {
    let Some(body) = body else {
        return Ok(BTreeMap::new());
    };
    let document: DocumentMut = body
        .parse()
        .map_err(|_| app_err!("配置档案中的 provider 数据无效"))?;
    Ok(document
        .as_table()
        .iter()
        .map(|(key, item)| (key.to_string(), item.to_string()))
        .collect())
}

pub fn update_provider_body(
    body: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> AppResult<String> {
    let mut document: DocumentMut = body
        .parse()
        .map_err(|_| app_err!("配置档案中的 provider 数据无效"))?;
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
        assert!(matches_profile(&document, &payload).unwrap());
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
