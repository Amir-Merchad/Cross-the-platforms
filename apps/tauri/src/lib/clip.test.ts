import { describe, expect, it } from "vitest";
import { ipadLine, needsRepair } from "./clip";

describe("ipadLine", () => {
  it("connected over Wi-Fi", () => expect(ipadLine({ Connected: { wifi: true } })).toBe("Connected over Wi-Fi"));
  it("connected over USB", () => expect(ipadLine({ Connected: { wifi: false } })).toBe("Connected over USB"));
  it("not paired", () => expect(ipadLine("NotPaired")).toBe("Not paired"));
  it("unavailable never shows the raw reason", () => expect(ipadLine({ Unavailable: "Developer disk image not mounted yet" })).toBe("Not connected"));
  it("pair verify failure asks to pair again", () => expect(ipadLine({ Unavailable: "pair verify failed" })).toBe("Needs pairing again"));
  it("no status yet", () => expect(ipadLine(null)).toBe("Starting…"));
});

describe("needsRepair", () => {
  it("true when the pair verify step failed", () => expect(needsRepair({ Unavailable: "lockdown: pair verify failed" })).toBe(true));
  it("false for other unavailable reasons", () => expect(needsRepair({ Unavailable: "Developer disk image not mounted yet" })).toBe(false));
  it("false for every other status", () => {
    expect(needsRepair(null)).toBe(false);
    expect(needsRepair("NotPaired")).toBe(false);
    expect(needsRepair({ Connected: { wifi: true } })).toBe(false);
  });
});
