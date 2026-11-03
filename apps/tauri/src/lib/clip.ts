import type { IpadStatus } from "@/lib/api";

/** One line for the iPad clipboard link, in the owner's words. */
export function ipadLine(s: IpadStatus | null): string {
  if (s === null) return "Starting…";
  if (s === "NotPaired") return "Not paired — plug the iPad in by USB and press Pair";
  if (s === "Connecting") return "Connecting…";
  if ("Connected" in s) return s.Connected.wifi ? "Connected over Wi-Fi" : "Connected over USB";
  return s.Unavailable;
}
