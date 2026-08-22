import { describe, expect, it } from "vitest";
import { SettingsAbout, SettingsAdvanced, SettingsGeneral, backupTitle, formatSize, formatTimestamp } from "./SettingsSections";

describe("SettingsSections", () => {
  it("formats backup titles", () => {
    expect(backupTitle("cg-backup-20260822-120000-000.db")).toBe("20260822-120000-000");
    expect(backupTitle("cgswitch-export-demo.db")).toBe("demo");
  });

  it("formats backup sizes", () => {
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(1024)).toBe("1.0 KB");
    expect(formatSize(1024 * 1024)).toBe("1.00 MB");
  });

  it("formats backup timestamps", () => {
    expect(formatTimestamp(0)).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/);
  });

  it("exports the three settings section components", () => {
    expect(typeof SettingsGeneral).toBe("function");
    expect(typeof SettingsAdvanced).toBe("function");
    expect(typeof SettingsAbout).toBe("function");
  });
});
