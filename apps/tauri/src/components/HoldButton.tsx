import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

const HOLD_MS = 900;

/** Runs `onDone` after a 900 ms hold, by pointer or by Enter/Space. Letting go early cancels. */
export default function HoldButton({ onDone, children, holding }: { onDone: () => void; children: ReactNode; holding: ReactNode }) {
  const [held, setHeld] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout>>(undefined);
  // State can change during the hold (a hotkey switch, say), and the hold must act on what is true when it ends.
  const done = useRef(onDone);
  useLayoutEffect(() => { done.current = onDone; });

  const start = () => {
    clearTimeout(timer.current);
    setHeld(true);
    timer.current = setTimeout(() => { setHeld(false); done.current(); }, HOLD_MS);
  };
  const stop = () => { clearTimeout(timer.current); setHeld(false); };
  useEffect(() => () => clearTimeout(timer.current), []);

  const isKey = (key: string) => key === "Enter" || key === " ";
  return (
    <button
      type="button"
      className="relative mt-auto h-11 w-full flex-none overflow-hidden rounded-xl border-2 border-border font-semibold text-destructive"
      onPointerDown={(e) => { if (e.button === 0) start(); }}
      onPointerUp={stop} onPointerLeave={stop} onPointerCancel={stop} onBlur={stop}
      onKeyDown={(e) => { if (isKey(e.key)) { e.preventDefault(); if (!e.repeat) start(); } }}
      onKeyUp={(e) => { if (isKey(e.key)) stop(); }}
    >
      <span
        className="absolute inset-0 origin-left bg-destructive opacity-[.22]"
        style={{ transform: `scaleX(${held ? 1 : 0})`, transition: `transform ${held ? HOLD_MS : 150}ms linear` }}
      />
      <span className="relative">{held ? holding : children}</span>
    </button>
  );
}
