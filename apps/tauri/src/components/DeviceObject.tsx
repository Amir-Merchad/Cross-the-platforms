import type { ComponentProps } from "react";
import { deviceSize } from "@/lib/desk";
import type { Device } from "@/lib/devices";
import { cn } from "@/lib/utils";

type Props = ComponentProps<"button"> & {
  d: Device; sub: string; off: boolean; selected: boolean; lifted: boolean;
  hotkey: number | null; /** Share of the current send, 0 to 1. */ ring: number | null;
};

/** One device on the map. Its position and scale are written by the spring loop, never by React. */
export default function DeviceObject({ d, sub, off, selected, lifted, hotkey, ring, ...button }: Props) {
  const { w, h } = deviceSize(d.kind);
  const lip = off ? "none" : `0 4px 0 color-mix(in srgb, ${d.colour} 70%, #000)`;
  return (
    <button
      type="button"
      {...button}
      data-device={d.key}
      aria-pressed={selected}
      className={cn(
        "absolute top-0 left-0 flex cursor-grab touch-none flex-col justify-end px-2.5 py-2 text-left text-[13px] leading-tight font-semibold text-[#10202E] select-none focus-visible:outline-offset-[7px]",
        selected && !lifted && "outline-3 outline-offset-3 outline-foreground",
        lifted && "z-10 cursor-grabbing",
      )}
      style={{
        width: w, height: h, borderRadius: d.kind === "Android" ? 18 : 16,
        background: off ? "var(--off)" : d.colour, boxShadow: lifted ? "0 14px 22px rgba(0,0,0,.35)" : lip,
      }}
    >
      {hotkey !== null && <span className="absolute top-2 right-2 rounded-[7px] bg-[#10202E] px-1.5 py-px text-[11px] text-white">{hotkey}</span>}
      {d.waiting > 0 && (
        <span className="absolute -top-2.5 -left-2.5 flex h-[26px] min-w-[26px] items-center justify-center rounded-full bg-primary font-bold shadow-[0_2px_0_var(--lip)]">{d.waiting}</span>
      )}
      {ring !== null && (
        <span
          aria-hidden
          className="pointer-events-none absolute -inset-3 rounded-[28px] p-1"
          style={{
            background: `conic-gradient(var(--foreground) ${ring}turn, transparent 0)`,
            mask: "linear-gradient(#000 0 0) content-box exclude, linear-gradient(#000 0 0)",
          }}
        />
      )}
      <span className="line-clamp-2 w-full break-words">{d.name}</span>
      <span className="w-full truncate text-xs font-normal">{sub}</span>
    </button>
  );
}
