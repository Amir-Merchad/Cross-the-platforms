import { describe, expect, it } from "vitest";
import { clipNotice, ipadLine, needsRepair } from "./clip";

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

describe("clipNotice", () => {
  it("passes the two fixed progress phrases", () => {
    expect(clipNotice("Pairing… confirm on the iPad if it asks")).toBe("Pairing… confirm on the iPad if it asks");
    expect(clipNotice("iPad paired")).toBe("iPad paired");
  });
  it("turns a failure into a plain line, never the raw text", () => {
    expect(clipNotice("Pairing failed: lockdown error 0x1 at C:/x")).toBe("Pairing did not work. Plug the iPad in by USB and try again.");
  });
  it("hides anything else", () => {
    expect(clipNotice("something raw")).toBeNull();
    expect(clipNotice(null)).toBeNull();
  });
});
