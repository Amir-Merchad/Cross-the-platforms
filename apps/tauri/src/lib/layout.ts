import type { Attachment } from "./api";

/** Same rule as Rust `layout::validate`, so Save is disabled before the backend would reject it. */
export function clashes(attachments: Attachment[]): string | null {
  for (let i = 0; i < attachments.length; i++) {
    for (const b of attachments.slice(i + 1)) {
      const a = attachments[i];
      if (a.monitor_id === b.monitor_id && a.edge === b.edge) return `two devices on the ${a.edge} edge of ${a.monitor_id}`;
      if (a.device_id === b.device_id) return `device ${a.device_id} is attached twice`;
    }
  }
  return null;
}
