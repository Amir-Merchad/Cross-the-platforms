import type { IpadStatus } from "@/lib/api";

/** The iPad no longer trusts this PC: the USB pairing has to be done again. */
export const needsRepair = (s: IpadStatus | null): boolean =>
  s !== null && typeof s === "object" && "Unavailable" in s && /pair verify/i.test(s.Unavailable);

/** One line for the iPad clipboard link, in the owner's words. Never the engine's own text. */
export function ipadLine(s: IpadStatus | null): string {
  if (s === null) return "Starting…";
  if (s === "NotPaired") return "Not paired";
  if (s === "Connecting") return "Connecting…";
  if ("Connected" in s) return s.Connected.wifi ? "Connected over Wi-Fi" : "Connected over USB";
  return needsRepair(s) ? "Needs pairing again" : "Not connected";
}

/** The engine's pairing notice as a fixed phrase. Anything it does not recognise is hidden: it can carry raw errors. */
export function clipNotice(notice: string | null): string | null {
  if (notice === "Pairing… confirm on the iPad if it asks" || notice === "iPad paired") return notice;
  return notice?.startsWith("Pairing failed") ? "Pairing did not work. Plug the iPad in by USB and try again." : null;
}
