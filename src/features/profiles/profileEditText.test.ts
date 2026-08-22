import { describe, expect, it } from "vitest";
import { patchProviderFields, readProviderFields, withMcpSection } from "./profileEditText";

describe("profileEditText", () => {
  it("reads the provider selected by model_provider", () => {
    const text = [
      'model_provider = "target"',
      "",
      "[model_providers.other]",
      'base_url = "https://other.example"',
      "",
      "[model_providers.target]",
      'base_url = "https://target.example"',
      'experimental_bearer_token = "secret"',
    ].join("\n");

    expect(readProviderFields(text)).toEqual({
      base_url: "https://target.example",
      experimental_bearer_token: "secret",
      found: true,
      tokenMasked: false,
    });
  });

  it("marks redacted provider tokens so the form keeps its current key", () => {
    const text = '[model_providers.target]\nexperimental_bearer_token = "••••••••"';

    expect(readProviderFields(text)).toMatchObject({
      experimental_bearer_token: "••••••••",
      found: true,
      tokenMasked: true,
    });
  });

  it("patches modeled fields while preserving unmodeled lines", () => {
    const text = [
      'model_provider = "target"',
      "",
      "[model_providers.target]",
      "# keep this comment",
      'wire_api = "responses"',
      'base_url = "https://old.example"',
      "",
      "[other]",
      'value = "unchanged"',
    ].join("\n");

    const patched = patchProviderFields(text, "https://new.example", "new-secret");
    expect(patched).toContain(
      '# keep this comment\nwire_api = "responses"\nbase_url = "https://new.example"\n\nexperimental_bearer_token = "new-secret"',
    );
    expect(patched).toContain('[other]\nvalue = "unchanged"');
  });

  it("does not clear fields when the referenced provider is unmatched", () => {
    const text = 'model_provider = "missing"\n\n[model_providers.other]\nbase_url = "https://other.example"';
    expect(readProviderFields(text).found).toBe(false);
    expect(patchProviderFields(text, "https://new.example", "new-secret")).toBe(text);
  });

  it("appends a normalized MCP section only when present", () => {
    expect(withMcpSection("base = true\n", '[mcp_servers.demo]\nurl = "https://x"'))
      .toBe('base = true\n\n[mcp_servers.demo]\nurl = "https://x"\n');
    expect(withMcpSection("base = true\n", "")).toBe("base = true\n");
  });
});
