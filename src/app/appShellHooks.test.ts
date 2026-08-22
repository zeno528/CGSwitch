import { describe, expect, it } from "vitest";
import { indicatorTop, useActivationRefresh, useAppState, useCodexPolling, useSidebarIndicator, useThemeMode } from "./appShellHooks";

describe("appShellHooks", () => {
  it("calculates the sidebar indicator top relative to the navigation", () => {
    expect(indicatorTop({ top: 140 }, { top: 100 })).toBe(48);
  });

  it("exports the application shell hooks", () => {
    expect(typeof useAppState).toBe("function");
    expect(typeof useThemeMode).toBe("function");
    expect(typeof useCodexPolling).toBe("function");
    expect(typeof useActivationRefresh).toBe("function");
    expect(typeof useSidebarIndicator).toBe("function");
  });
});
