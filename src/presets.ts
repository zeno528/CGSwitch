export interface BuiltinPreset {
  kind: string;
  name: string;
  provider: string | null;
  icon: string;
  base_url: string;
  model: string;
  model_values: Record<string, string>;
  fragment: string;
}

export const builtinPresets: BuiltinPreset[] = [
  {
    kind: "deepseek",
    name: "DeepSeek 官方",
    provider: "deepseek",
    icon: "deepseek",
    base_url: "https://api.deepseek.com/",
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
    name: "MiniMax 官方",
    provider: "minimax",
    icon: "minimax",
    base_url: "https://api.minimaxi.com/v1",
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
    name: "智谱官方",
    provider: "ZAI",
    icon: "zhipu",
    base_url: "https://open.bigmodel.cn/api/v1",
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
    name: "ChatGPT 官方",
    provider: null,
    icon: "openai-chatgpt",
    base_url: "",
    model: "gpt-5.6",
    model_values: {
      model: '"gpt-5.6"',
      model_reasoning_effort: '"medium"',
    },
    fragment: 'model = "gpt-5.6"\nmodel_reasoning_effort = "medium"',
  },
];

export function builtinPresetByKind(kind: string): BuiltinPreset | undefined {
  return builtinPresets.find((preset) => preset.kind === kind);
}
