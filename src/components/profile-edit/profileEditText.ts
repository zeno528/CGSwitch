export interface ProviderFields {
  base_url: string;
  experimental_bearer_token: string;
  found: boolean;
  tokenMasked: boolean;
}

// 读取 [model_providers.*] 段里的 base_url / 密钥，供编辑器回填表单。
export function readProviderFields(text: string): ProviderFields {
  const values: ProviderFields = {
    base_url: "",
    experimental_bearer_token: "",
    found: false,
    tokenMasked: false,
  };
  const lines = text.split("\n");
  let providerId: string | null = null;
  for (const line of lines) {
    const match = /^model_provider\s*=\s*"([^"]+)"/.exec(line.trim());
    if (match) {
      providerId = match[1];
      break;
    }
  }
  let inProvider = false;
  let done = false;
  for (const line of lines) {
    const trimmed = line.trim();
    if (/^\[.+\]$/.test(trimmed)) {
      const section = /^\[model_providers\.(.+)\]$/.exec(trimmed);
      if (section && !done) {
        // 只处理 model_provider 指向的段；无 model_provider 时退化为第一段。
        inProvider = providerId === null || section[1] === providerId;
        if (inProvider) {
          done = true;
          values.found = true;
        }
      } else {
        inProvider = false;
      }
      continue;
    }
    if (!inProvider) continue;
    const match =
      /^(base_url|experimental_bearer_token)\s*=\s*(?:(['"])(.*?)\2|([^\s]+))/.exec(trimmed);
    if (!match) continue;
    const field = match[1] as "base_url" | "experimental_bearer_token";
    const value = match[3] ?? match[4] ?? "";
    values[field] = value;
    if (field === "experimental_bearer_token") values.tokenMasked = /^[•*]+$/.test(value);
  }
  return values;
}

// 把表单里的地址/密钥写回编辑器 provider 段；缺失的行在段尾补上。
export function patchProviderFields(text: string, baseUrl: string, apiKey: string): string {
  const escape = (value: string, quote: string) =>
    value.replace(/\\/g, "\\\\").replace(new RegExp(quote, "g"), "\\" + quote);
  const base = baseUrl.trim();
  const key = apiKey.trim();
  const lines = text.split("\n");
  let providerId: string | null = null;
  for (const line of lines) {
    const match = /^model_provider\s*=\s*"([^"]+)"/.exec(line.trim());
    if (match) {
      providerId = match[1];
      break;
    }
  }
  let inProvider = false;
  let done = false;
  let replacedBase = false;
  let replacedKey = false;
  const out: string[] = [];
  const flushMissing = () => {
    if (!inProvider) return;
    if (base && !replacedBase) out.push(`base_url = "${escape(base, '"')}"`);
    if (key && !replacedKey) {
      out.push(`experimental_bearer_token = "${escape(key, '"')}"`);
    }
    inProvider = false;
  };
  for (const line of lines) {
    const trimmed = line.trim();
    if (/^\[.+\]$/.test(trimmed)) {
      flushMissing();
      const section = /^\[model_providers\.(.+)\]$/.exec(trimmed);
      if (section && !done) {
        inProvider = providerId === null || section[1] === providerId;
        if (inProvider) done = true;
      } else {
        inProvider = false;
      }
      replacedBase = false;
      replacedKey = false;
      out.push(line);
      continue;
    }
    if (!inProvider) {
      out.push(line);
      continue;
    }
    const match = /^(base_url|experimental_bearer_token)\s*=\s*(['"]?)(.*?)\2\s*$/.exec(
      trimmed,
    );
    if (!match) {
      out.push(line);
      continue;
    }
    const field = match[1];
    const quote = match[2] || '"';
    const value = field === "base_url" ? base : key;
    const indent = line.slice(0, line.length - line.trimStart().length);
    if (field === "base_url") replacedBase = true;
    else replacedKey = true;
    out.push(`${indent}${field} = ${quote}${escape(value, quote)}${quote}`);
  }
  flushMissing();
  return out.join("\n");
}

export function withMcpSection(base: string, mcpSection: string): string {
  return mcpSection ? `${base.trimEnd()}\n\n${mcpSection.trimEnd()}\n` : base;
}
