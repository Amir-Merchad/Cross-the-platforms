import { describe, expect, it } from "vitest";
import { entryTitle, whenLabel } from "./history";

const NOW = new Date(2026, 10, 5, 15, 0).getTime();
const at = (d: number, h: number, m: number) => new Date(2026, 10, d, h, m).getTime();

describe("whenLabel", () => {
  it("just now under a minute", () => expect(whenLabel(NOW - 20_000, NOW)).toBe("Just now"));
  it("minutes", () => expect(whenLabel(NOW - 5 * 60_000, NOW)).toBe("5 min ago"));
  it("earlier today shows the time", () => expect(whenLabel(at(5, 9, 5), NOW)).toBe("9:05"));
  it("yesterday", () => expect(whenLabel(at(4, 22, 30), NOW)).toBe("Yesterday"));
  it("older shows the date", () => expect(whenLabel(at(1, 8, 0), NOW)).toBe("1 Nov"));
});

describe("entryTitle", () => {
  const base = { id: 1, kind: "text" as const, text: "hello", width: 0, height: 0, bytes: 5, from: "This PC", at: 0, pinned: false };
  it("shows the first line of text", () => expect(entryTitle({ ...base, text: "first\nsecond" })).toBe("first"));
  it("blank text shows a plain name", () => expect(entryTitle({ ...base, text: "  \n " })).toBe("Blank text"));
  it("images show their size", () => expect(entryTitle({ ...base, kind: "image", text: null, width: 800, height: 600 })).toBe("Image, 800 × 600"));
});
