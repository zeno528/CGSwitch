import { describe, expect, it } from "vitest";
import { webInvoke } from "./web-mock";

describe("web mock", () => {
  it("round-trips MCP env entries", async () => {
    const fragment = await webInvoke<string>("get_mcp_server_toml", { name: "github" });
    const spec = await webInvoke<{ env: Record<string, string> }>("parse_mcp_fragment", { toml: fragment });

    expect(spec.env).toEqual({ GITHUB_PERSONAL_ACCESS_TOKEN: "ghp_demo" });
  });

  it("rejects MCP server names containing dots", async () => {
    await expect(
      webInvoke("parse_mcp_fragment", { toml: '[mcp_servers.a.b]\nurl = "https://example.com"' }),
    ).rejects.toThrow("片段中没有服务器");
  });

  it("passes format_toml input through in web mode", async () => {
    const text = 'model = "demo"';
    await expect(webInvoke("format_toml", { text })).resolves.toBe(text);
  });
});
