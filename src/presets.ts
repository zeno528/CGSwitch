export interface BuiltinPreset {
  kind: string;
  name: string;
  provider: string | null;
  icon: string;
  base_url: string;
  admin_url: string | null;
  model: string;
  model_values: Record<string, string>;
  fragment: string;
}

export const customConfigTemplate = `model = "your-model"
model_provider = "your-provider"
model_reasoning_effort = "medium"
model_catalog_json = "~/.codex/models.json"

[model_providers.your-provider]
name = "your-provider"
base_url = "https://api.example.com/v1"
wire_api = "responses"
experimental_bearer_token = "<你的 API Key>"`;

export const customCatalogTemplate = `{
  "models": [
    {
      "id": "your-model",
      "name": "Your Model"
    }
  ]
}`;

export const customAuthTemplate = `{
  "auth_mode": "api_key",
  "OPENAI_API_KEY": "your-api-key"
}`;

export const builtinPresets: BuiltinPreset[] = [
  {
    kind: "custom",
    name: "自定义",
    provider: null,
    icon: "custom",
    base_url: "https://api.example.com/v1",
    admin_url: null,
    model: "自定义",
    model_values: { model_catalog_json: '"~/.codex/models.json"' },
    fragment: customConfigTemplate,
  },
  {
    kind: "deepseek",
    name: "DeepSeek",
    provider: "deepseek",
    icon: "deepseek",
    base_url: "https://api.deepseek.com/",
    admin_url: "https://platform.deepseek.com",
    model: "deepseek-v4-flash",
    model_values: {
      model: '"deepseek-v4-flash"',
      model_reasoning_effort: '"high"',
      model_catalog_json: '"~/.codex/models.json"',
    },
    fragment: [
      'model = "deepseek-v4-flash"',
      'model_provider = "deepseek"',
      'preferred_auth_method = "apikey"',
      'forced_login_method = "api"',
      'model_reasoning_effort = "high"',
      'model_catalog_json = "~/.codex/models.json"',
      "",
      "[model_providers.deepseek]",
      'name = "deepseek"',
      'base_url = "https://api.deepseek.com/"',
      'wire_api = "responses"',
      'experimental_bearer_token = "<你的 DeepSeek API Key>"',
    ].join("\n"),
  },
  {
    kind: "minimax",
    name: "MiniMax",
    provider: "minimax",
    icon: "minimax",
    base_url: "https://api.minimaxi.com/v1",
    admin_url: "https://platform.minimaxi.com",
    model: "MiniMax-M3",
    model_values: {
      model: '"MiniMax-M3"',
      model_reasoning_effort: '"high"',
      model_catalog_json: '"~/.codex/model-catalogs/custom-catalog.json"',
    },
    fragment: [
      'model = "MiniMax-M3"',
      'model_provider = "minimax"',
      "model_context_window = 1000000",
      'model_catalog_json = "~/.codex/model-catalogs/custom-catalog.json"',
      "",
      "[model_providers.minimax]",
      'name = "MiniMax"',
      'base_url = "https://api.minimaxi.com/v1"',
      'experimental_bearer_token = "<MINIMAX_API_KEY>"',
      'wire_api = "responses"',
    ].join("\n"),
  },
  {
    kind: "zhipu",
    name: "智谱",
    provider: "ZAI",
    icon: "zhipu",
    base_url: "https://open.bigmodel.cn/api/v1",
    admin_url: "https://open.bigmodel.cn",
    model: "glm-5.3",
    model_values: {
      model: '"glm-5.3"',
      model_reasoning_effort: '"max"',
      model_catalog_json: '"~/.codex/models.json"',
    },
    fragment: [
      'model_provider = "ZAI"',
      'model = "glm-5.3"',
      'model_reasoning_effort = "max"',
      'model_catalog_json = "~/.codex/models.json"',
      "",
      "[model_providers.ZAI]",
      'name = "ZAI"',
      'base_url = "https://open.bigmodel.cn/api/v1"',
      'experimental_bearer_token = "<Your API Key>"',
      'wire_api = "responses"',
    ].join("\n"),
  },
  {
    kind: "chatgpt",
    name: "ChatGPT",
    provider: null,
    icon: "openai-chatgpt",
    base_url: "",
    admin_url: "https://openai.com/chatgpt/pricing",
    model: "gpt-5.6",
    model_values: {
      model: '"gpt-5.6"',
      model_reasoning_effort: '"medium"',
    },
    fragment: 'model = "gpt-5.6"\nmodel_reasoning_effort = "medium"',
  },
  {
    kind: "opencode",
    name: "OpenCode",
    provider: "opencode-go",
    icon: "opencode",
    base_url: "https://opencode.ai/zen/go/v1",
    admin_url: null,
    model: "deepseek-v4-flash",
    model_values: {
      model: '"deepseek-v4-flash"',
      model_reasoning_effort: '"high"',
      model_catalog_json: '"~/.codex/models.json"',
    },
    fragment: [
      'model = "deepseek-v4-flash"',
      'model_provider = "opencode-go"',
      'preferred_auth_method = "apikey"',
      'forced_login_method = "api"',
      'model_reasoning_effort = "high"',
      'model_catalog_json = "~/.codex/models.json"',
      "",
      "[model_providers.opencode-go]",
      'name = "OpenCode Go"',
      'base_url = "https://opencode.ai/zen/go/v1"',
      'wire_api = "responses"',
      'experimental_bearer_token = "<你的 OpenCode API Key>"',
    ].join("\n"),
  },
];

export function builtinPresetByKind(kind: string): BuiltinPreset | undefined {
  return builtinPresets.find((preset) => preset.kind === kind);
}
