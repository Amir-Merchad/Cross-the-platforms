// Files dragged in from Explorer. Names stay in the window: they are shown, never logged.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { plainError } from "@/lib/activity";
import { fileTarget } from "@/lib/desk";
import { useHub } from "@/lib/hub";
import { sendFiles } from "@/lib/transfer";
import { reducedMotion } from "@/components/useDeskMotion";

const label = (paths: string[]) => (paths.length === 1 ? paths[0].split(/[\\/]/).pop()! : `${paths.length} files`);

const deviceEls = () => [...document.querySelectorAll<HTMLElement>("[data-device]")];

function targetAt(x: number, y: number): string | null {
  const rects = deviceEls().map((el) => {
    const { left, top, right, bottom } = el.getBoundingClientRect();
    return { key: el.dataset.device!, left, top, right, bottom };
  });
  return fileTarget({ x, y }, rects);
}

export function useFileDrop() {
  const hub = useHub();
  const [over, setOver] = useState<string | null>(null);
  const [chip, setChip] = useState<{ x: number; y: number; text: string } | null>(null);
  const chipEl = useRef<HTMLDivElement>(null);
  const latest = useRef(hub);
  useLayoutEffect(() => { latest.current = hub; });

  useEffect(() => {
    let text = "", target: string | null = null;
    const hover = (next: string | null) => {
      if (next === target) return;
      target = next;
      setOver(next);
      const d = latest.current.devices.find((x) => x.key === next);
      latest.current.say(d ? `Release to send ${text} to ${d.name}.` : "");
    };
    const clear = () => { hover(null); setChip(null); };

    /** The chip flies into the device while the send starts. */
    const fly = (key: string) => {
      const el = chipEl.current, to = deviceEls().find((x) => x.dataset.device === key);
      if (!el || !to || reducedMotion()) { setChip(null); return; }
      const a = el.getBoundingClientRect(), b = to.getBoundingClientRect();
      const dx = b.left + b.width / 2 - a.left - a.width / 2, dy = b.top + b.height / 2 - a.top - a.height / 2;
      el.animate([{ transform: "none" }, { transform: `translate(${dx}px,${dy}px) scale(.2)`, opacity: 0.3 }], { duration: 300, easing: "cubic-bezier(.5,0,.3,1)" }).onfinish = () => setChip(null);
    };

    const off = getCurrentWebview().onDragDropEvent(({ payload: p }) => {
      if (latest.current.tab !== "desk") return;
      if (p.type === "leave") { clear(); return; }
      if (p.type === "enter") text = label(p.paths);
      const x = p.position.x / window.devicePixelRatio, y = p.position.y / window.devicePixelRatio;
      if (p.type !== "drop") {
        hover(targetAt(x, y));
        setChip({ x, y, text });
        return;
      }
      const hub = latest.current;
      const key = targetAt(x, y);
      const d = hub.devices.find((v) => v.key === key);
      target = null;
      setOver(null);
      if (!d || !p.paths.length) { hub.say(""); setChip(null); return; }
      hub.select(d.key);
      if (!d.peerId) { hub.say("Not paired for files."); setChip(null); return; }
      hub.say("");
      // Drops always try: reachability can be a minute stale, and the result sets the message.
      sendFiles(d.peerId, p.paths).catch((e) => hub.say(plainError(String(e), false)));
      fly(d.key);
    });
    return () => { off.then((f) => f()); };
  }, []);

  return { over, chip, chipEl };
}
