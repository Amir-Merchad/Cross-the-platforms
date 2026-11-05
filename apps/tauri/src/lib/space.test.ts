import { expect, test } from "vitest";
import type { Monitor, Settings } from "./api";
import { besideMap, effectiveLayout, layoutValid, placeBeside, placedIds, slotRect, syncLayout, touching } from "./space";

const M1: Monitor = { id: "M1", rect: { left: 0, top: 0, right: 1920, bottom: 1080 } };
const settings = (o: Partial<Settings> = {}): Settings => ({
  version: 1, attachments: [], calibration: {}, entry_hold_ms: 0, start_with_windows: false, remote_width: {},
  clipboard_sync: true, receive_dir: null, device_links: {}, device_colors: {}, appearance: "system", bluetooth_enabled: true, retired_hosts: [], layout: [], ...o,
});
const ipadRight = settings({ attachments: [{ device_id: "ipad", monitor_id: "M1", edge: "Right" }], remote_width: { phone: 400 } });

test("made-up placements match core: the whole monitor edge", () => {
  expect(effectiveLayout(ipadRight, [M1])).toEqual([{ device_id: "ipad", rect: { left: 1920, top: 0, right: 3286, bottom: 1080 } }]);
  expect(effectiveLayout(ipadRight, [])).toEqual([]);
});

test("touching needs a shared span", () => {
  expect(touching(M1.rect, { left: 1920, top: 0, right: 2000, bottom: 10 })).toBe("Right");
  expect(touching(M1.rect, { left: 1920, top: 1080, right: 2000, bottom: 1200 })).toBeNull();
});

test("a device beside a device writes the layout and keeps the first device where it was", () => {
  const s = placeBeside(ipadRight, [M1], "phone", "ipad", "Right")!;
  expect(s.layout).toEqual([
    { device_id: "ipad", rect: { left: 1920, top: 0, right: 3286, bottom: 1080 } },
    { device_id: "phone", rect: { left: 3286, top: 0, right: 3686, bottom: 1080 } },
  ]);
  expect(placedIds(s)).toEqual(["ipad", "phone"]);
  expect(besideMap(s, [M1]).get("phone")).toEqual({ anchor: "ipad", edge: "Right" });
  expect(besideMap(s, [M1]).has("ipad")).toBe(false);
});

test("a drop that would overlap is refused", () => {
  const s = placeBeside(ipadRight, [M1], "phone", "ipad", "Left");
  expect(s).toBeNull();
  expect(placeBeside(ipadRight, [M1], "ipad", "ipad", "Right")).toBeNull();
});

test("moving the anchor at a screen edge keeps the layout in step, and refuses an overlap", () => {
  const s = placeBeside(ipadRight, [M1], "phone", "ipad", "Right")!;
  const moved = { ...s, attachments: [{ device_id: "ipad", monitor_id: "M1", edge: "Left" as const }] };
  const synced = syncLayout(moved, s.attachments, [M1])!;
  expect(synced.layout.find((p) => p.device_id === "ipad")!.rect.right).toBe(0);
  expect(synced.layout.find((p) => p.device_id === "phone")).toEqual(s.layout[1]);
  const clash = { ...s, attachments: [...s.attachments, { device_id: "x", monitor_id: "M1", edge: "Right" as const }], layout: [s.layout[1]] };
  expect(syncLayout(clash, [], [M1])).toBeNull();
  expect(syncLayout(ipadRight, [], [M1])).toBe(ipadRight);
});

test("unplacing at a screen edge drops it from the layout too", () => {
  const s = placeBeside(ipadRight, [M1], "phone", "ipad", "Right")!;
  const gone = syncLayout({ ...s, attachments: [] }, s.attachments, [M1])!;
  expect(gone.layout.map((p) => p.device_id)).toEqual(["phone"]);
});

test("slots above and below use the 4:3 depth", () => {
  expect(slotRect(M1.rect, "Top", 1000)).toEqual({ left: 0, right: 1920, top: -750, bottom: 0 });
  expect(layoutValid([{ device_id: "a", rect: slotRect(M1.rect, "Top", 1000) }], [M1])).toBe(true);
});
