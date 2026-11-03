import { describe, expect, it } from "vitest";
import { ipadLine } from "./clip";

describe("ipadLine", () => {
  it("connected over Wi-Fi", () => expect(ipadLine({ Connected: { wifi: true } })).toBe("Connected over Wi-Fi"));
  it("connected over USB", () => expect(ipadLine({ Connected: { wifi: false } })).toBe("Connected over USB"));
  it("not paired", () => expect(ipadLine("NotPaired")).toBe("Not paired — plug the iPad in by USB and press Pair"));
  it("unavailable shows the reason", () => expect(ipadLine({ Unavailable: "Developer disk image not mounted yet" })).toBe("Developer disk image not mounted yet"));
  it("no status yet", () => expect(ipadLine(null)).toBe("Starting…"));
});
