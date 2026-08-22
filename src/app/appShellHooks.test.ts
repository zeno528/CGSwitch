import { describe, expect, it } from "vitest";
import { indicatorTop } from "./appShellHooks";

describe("appShellHooks", () => {
  it("calculates the sidebar indicator top relative to the navigation", () => {
    expect(indicatorTop({ top: 140 }, { top: 100 })).toBe(48);
  });
});
