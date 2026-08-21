import { describe, expect, it } from "vitest";
import { balanceChipClass } from "./presets";

describe("balanceChipClass", () => {
  it("marks a negative balance as danger when usage is unavailable", () => {
    expect(balanceChipClass(null, false, "-1.00")).toBe("chip-danger");
  });

  it("keeps zero and positive balances successful", () => {
    expect(balanceChipClass(null, false, "0.00")).toBe("chip-success");
    expect(balanceChipClass(null, false, "110.00")).toBe("chip-success");
  });

  it("uses usage thresholds before the total balance", () => {
    expect(balanceChipClass(70, false, "110.00")).toBe("chip-warn");
    expect(balanceChipClass(90, false, "110.00")).toBe("chip-danger");
  });
});
