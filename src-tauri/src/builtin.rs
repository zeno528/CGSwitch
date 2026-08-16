use crate::error::{app_err, AppResult};

pub const KIND_DEEPSEEK: &str = "deepseek";
pub const KIND_MINIMAX: &str = "minimax";
pub const KIND_ZHIPU: &str = "zhipu";
pub const KIND_CHATGPT: &str = "chatgpt";

pub const DEEPSEEK_CONFIG: &[u8] = include_bytes!("../assets/builtin/deepseek.toml");
pub const DEEPSEEK_MODELS: &[u8] = include_bytes!("../assets/builtin/deepseek-models.json");
pub const MINIMAX_CONFIG: &[u8] = include_bytes!("../assets/builtin/minimax.toml");
pub const MINIMAX_CATALOG: &[u8] = include_bytes!("../assets/builtin/minimax-catalog.json");
pub const ZHIPU_CONFIG: &[u8] = include_bytes!("../assets/builtin/zhipu.toml");
pub const ZHIPU_MODELS: &[u8] = include_bytes!("../assets/builtin/zhipu-models.json");
pub const CHATGPT_CONFIG: &[u8] = include_bytes!("../assets/builtin/chatgpt.toml");

pub struct BuiltinTemplate {
    pub kind: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    /// 生产 config.toml 的模板原文（字节级）。
    pub config: &'static [u8],
    /// 模板中的密钥占位符，应用时替换为用户填写的密钥。
    pub placeholder: Option<&'static [u8]>,
    /// 相对 ~/.codex 的关联文件路径及内容（deepseek/智谱 各自独立的 models.json、minimax 的 custom-catalog.json）。
    pub catalog: Option<(&'static str, &'static [u8])>,
    /// minimax 需要额外插入 model_catalog_json 行。
    pub insert_catalog_line: bool,
}

pub const BUILTINS: [BuiltinTemplate; 4] = [
    BuiltinTemplate {
        kind: KIND_DEEPSEEK,
        name: "DeepSeek 官方",
        icon: "deepseek",
        config: DEEPSEEK_CONFIG,
        placeholder: Some("<你的 DeepSeek API Key>".as_bytes()),
        catalog: Some(("models.json", DEEPSEEK_MODELS)),
        insert_catalog_line: false,
    },
    BuiltinTemplate {
        kind: KIND_MINIMAX,
        name: "MiniMax 官方",
        icon: "minimax",
        config: MINIMAX_CONFIG,
        placeholder: Some("<MINIMAX_API_KEY>".as_bytes()),
        catalog: Some(("model-catalogs/custom-catalog.json", MINIMAX_CATALOG)),
        insert_catalog_line: true,
    },
    BuiltinTemplate {
        kind: KIND_ZHIPU,
        name: "智谱官方",
        icon: "zhipu",
        config: ZHIPU_CONFIG,
        placeholder: Some("<Your API Key>".as_bytes()),
        catalog: Some(("models.json", ZHIPU_MODELS)),
        insert_catalog_line: false,
    },
    BuiltinTemplate {
        kind: KIND_CHATGPT,
        name: "ChatGPT 官方",
        icon: "openai-chatgpt",
        config: CHATGPT_CONFIG,
        placeholder: None,
        catalog: None,
        insert_catalog_line: false,
    },
];

impl BuiltinTemplate {
    /// 渲染生产 config 原文：仅替换密钥占位符（未填则保留），
    /// minimax 额外在 model_context_window 之后插入 model_catalog_json 行，其余字节不动。
    pub fn render_config(&self, api_key: Option<&str>) -> AppResult<Vec<u8>> {
        let mut bytes = self.config.to_vec();
        if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
            if let Some(placeholder) = self.placeholder {
                let start = find_subslice(&bytes, placeholder)
                    .ok_or_else(|| app_err!("{} 模板缺少密钥占位符", self.name))?;
                bytes.splice(
                    start..start + placeholder.len(),
                    key.as_bytes().iter().copied(),
                );
            }
        }
        if self.insert_catalog_line {
            let needle = b"model_context_window = 1000000\n";
            let start = find_subslice(&bytes, needle)
                .ok_or_else(|| app_err!("{} 模板缺少插入位置", self.name))?;
            let line = b"model_catalog_json = \"~/.codex/model-catalogs/custom-catalog.json\"\n";
            let end = start + needle.len();
            bytes.splice(end..end, line.iter().copied());
        }
        Ok(bytes)
    }
}

pub fn template(kind: &str) -> AppResult<&'static BuiltinTemplate> {
    BUILTINS
        .iter()
        .find(|item| item.kind == kind)
        .ok_or_else(|| app_err!("未知的内置档案类型：{kind}"))
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_configs_match_official_templates_byte_for_byte() {
        assert_eq!(
            DEEPSEEK_CONFIG,
            b"model = \"deepseek-v4-flash\"\nmodel_provider = \"deepseek\"\npreferred_auth_method = \"apikey\"\nforced_login_method = \"api\"\nmodel_reasoning_effort = \"high\"\nmodel_catalog_json = \"~/.codex/models.json\"\n\n[model_providers.deepseek]\nname = \"deepseek\"\nbase_url = \"https://api.deepseek.com/\"\nwire_api = \"responses\"\nexperimental_bearer_token = \"<\xE4\xBD\xA0\xE7\x9A\x84 DeepSeek API Key>\""
        );
        assert_eq!(
            MINIMAX_CONFIG,
            b"model = \"MiniMax-M3\"\nmodel_provider = \"minimax\"\nmodel_context_window = 1000000\n\n[model_providers.minimax]\nname = \"MiniMax\"\nbase_url = \"https://api.minimaxi.com/v1\"\nexperimental_bearer_token = \"<MINIMAX_API_KEY>\"\nwire_api = \"responses\""
        );
        assert_eq!(
            ZHIPU_CONFIG,
            b"model_provider = \"ZAI\"\nmodel = \"glm-5.3\"\nmodel_reasoning_effort = \"max\"\nmodel_catalog_json = \"~/.codex/models.json\"\n\n[model_providers.ZAI]\nname = \"ZAI\"\nbase_url = \"https://open.bigmodel.cn/api/v1\"\nexperimental_bearer_token = \"<Your API Key>\"\nwire_api = \"responses\""
        );
        assert_eq!(
            CHATGPT_CONFIG,
            b"model = \"gpt-5.6\"\nmodel_reasoning_effort = \"medium\"\n"
        );
    }

    #[test]
    fn embedded_catalogs_keep_original_size_and_line_endings() {
        assert_eq!(DEEPSEEK_MODELS.len(), 76215);
        assert_eq!(count(DEEPSEEK_MODELS, b"\r\n"), 137);
        assert_eq!(ZHIPU_MODELS.len(), 2543);
        assert_eq!(count(ZHIPU_MODELS, b"\r\n"), 72);
        assert_eq!(MINIMAX_CATALOG.len(), 953);
        assert_eq!(count(MINIMAX_CATALOG, b"\r\n"), 25);
    }

    #[test]
    fn render_replaces_key_and_inserts_minimax_line() {
        let deepseek = template(KIND_DEEPSEEK).unwrap();
        let rendered = deepseek.render_config(Some("sk-real")).unwrap();
        assert!(rendered.windows(b"sk-real".len()).any(|w| w == b"sk-real"));
        assert!(!rendered
            .windows("<你的 DeepSeek API Key>".len())
            .any(|w| w == "<你的 DeepSeek API Key>".as_bytes()));
        let kept = deepseek.render_config(None).unwrap();
        assert_eq!(kept, DEEPSEEK_CONFIG);

        let minimax = template(KIND_MINIMAX).unwrap();
        let rendered = minimax.render_config(Some("mm-key")).unwrap();
        assert!(rendered
            .windows(b"model_catalog_json = \"~/.codex/model-catalogs/custom-catalog.json\"".len())
            .any(|w| {
                w == b"model_catalog_json = \"~/.codex/model-catalogs/custom-catalog.json\""
            }));
        assert!(rendered.windows(b"mm-key".len()).any(|w| w == b"mm-key"));
    }

    fn count(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|w| *w == needle)
            .count()
    }
}
