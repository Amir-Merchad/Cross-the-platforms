import { describe, expect, it } from "vitest";
import { formatBytes, formatRate, progressPercent, queuedLine } from "./transfer";

describe("progressPercent", () => {
  it("nothing to send is 0, not NaN", () => expect(progressPercent(0, 0)).toBe(0));
  it("half way", () => expect(progressPercent(50, 100)).toBe(50));
  it("never above 100", () => expect(progressPercent(150, 100)).toBe(100));
  it("never below 0", () => expect(progressPercent(-5, 100)).toBe(0));
});

describe("formatRate", () => {
  it("MB/s", () => expect(formatRate(80_000_000, 1000)).toBe("80.0 MB/s"));
  it("KB/s for slow links", () => expect(formatRate(500_000, 1000)).toBe("500 KB/s"));
  it("no time yet", () => expect(formatRate(1000, 0)).toBe("—"));
});

describe("formatBytes", () => {
  it("bytes", () => expect(formatBytes(512)).toBe("512 B"));
  it("MB", () => expect(formatBytes(3_500_000)).toBe("3.5 MB"));
  it("GB", () => expect(formatBytes(2_000_000_000)).toBe("2.00 GB"));
});

describe("queuedLine", () => {
  it("one file", () => expect(queuedLine("iPad", 1)).toBe("1 file waiting — it goes when Bridge opens on iPad"));
  it("several", () => expect(queuedLine("iPad", 3)).toBe("3 files waiting — they go when Bridge opens on iPad"));
});
