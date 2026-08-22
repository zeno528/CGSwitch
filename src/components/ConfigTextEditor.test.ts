import packageJson from "../../package.json";
import profileEditSource from "../features/profiles/ProfileEdit.tsx?raw";
import { describe, expect, it } from "vitest";

describe("ConfigTextEditor runtime", () => {
  it("uses the native CodeMirror runtime instead of a duplicate wrapper runtime", () => {
    const dependencies = packageJson.dependencies as Record<string, string>;
    expect(dependencies["@uiw/react-codemirror"]).toBeUndefined();
    expect(dependencies.codemirror).toBeUndefined();
    expect(dependencies["@codemirror/state"]).toBeDefined();
    expect(dependencies["@codemirror/view"]).toBeDefined();
  });

  it("binds diagnostic focus to every profile editor variant", () => {
    expect(profileEditSource.match(/<ConfigTextEditor[^>]*ref=\{editorRef\}/g)).toHaveLength(3);
  });
});
