import type { HistoryEntry } from "@/lib/api";

/** "Just now", "5 min ago", the time today, "Yesterday", else the date. */
export function whenLabel(at: number, now: number): string {
  const mins = Math.floor((now - at) / 60_000);
  if (mins < 1) return "Just now";
  if (mins < 60) return `${mins} min ago`;
  const d = new Date(at), n = new Date(now);
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((day(n) - day(d)) / 86_400_000);
  if (days === 0) return `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (days === 1) return "Yesterday";
  return `${d.getDate()} ${d.toLocaleString("en", { month: "short" })}`;
}

/** One line for the list: the first line of the text, or the image size. */
export function entryTitle(e: HistoryEntry): string {
  if (e.kind === "image") return `Image, ${e.width} × ${e.height}`;
  return (e.text ?? "").split("\n").find((l) => l.trim())?.trim() || "Blank text";
}
