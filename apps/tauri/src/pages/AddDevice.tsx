import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { Button } from "@/components/ui/button";
import { pairIpad } from "@/lib/api";
import { addStep, newLink, stepCount, type AddKind } from "@/lib/addflow";
import { clipNotice } from "@/lib/clip";
import { useHub } from "@/lib/hub";
import { onTransfer, pairInvite } from "@/lib/transfer";
import { RAISED, cn } from "@/lib/utils";

const TITLES: Record<AddKind, string[]> = {
  ipad: ["1. Keyboard and mouse", "2. Files", "3. Clipboard"],
  phone: ["1. Keyboard and mouse", "2. Files and clipboard"],
};
const BT_TEXT: Record<AddKind, string> = {
  ipad: "On the iPad, open Settings, then Bluetooth, and pick this PC.",
  phone: "On the phone, open Settings, then Bluetooth, and pick this PC.",
};
const FILES_TEXT: Record<AddKind, string> = {
  ipad: "Open Bridge on the iPad, tap Pair with PC, and scan this code. Works once.",
  phone: "Open Bridge on the phone, tap Pair with PC, and scan this code. Works once.",
};

type Qr = { img: string; code: string; until: number };

/** Waits for the device lists, so the snapshot of what exists already is complete. */
export default function AddDevice() {
  const { loaded } = useHub();
  return loaded ? <Guide /> : <p role="status" className="text-muted-foreground">Looking for your devices…</p>;
}

function Guide() {
  const { status, peers, clip, settings, devices, addOpts, update, say, select, setTab } = useHub();
  const [kind, setKind] = useState<AddKind>(addOpts.kind);
  const [skipped, setSkipped] = useState<number[]>([]);
  // New means appeared since this page opened.
  const [before] = useState(() => ({ hosts: (status?.hosts ?? []).map((h) => h.id), peers: peers.map((p) => p.id) }));
  const [qr, setQr] = useState<Qr | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [triedCode, setTriedCode] = useState(false);

  const hosts = status?.hosts ?? [];
  const link = newLink(before, hosts, peers, settings.device_links, addOpts, kind);
  const hostId = addOpts.hostId ?? hosts.find((h) => !before.hosts.includes(h.id))?.id;
  const seen = {
    host: !!hostId,
    peer: !!addOpts.peerId || peers.some((p) => !before.peers.includes(p.id) && p.kind === (kind === "ipad" ? "Ipad" : "Android")),
    clip: !!clip?.ipad && typeof clip.ipad === "object" && "Connected" in clip.ipad,
  };
  const seenAt = [seen.host, seen.peer, seen.clip];
  const step = addStep(kind, seen, skipped);
  const count = stepCount(kind);

  // Once both sides are here, remember they are one device. The link then exists, so this does not repeat.
  const linkHost = link?.host, linkPeer = link?.peer;
  useEffect(() => {
    if (linkHost && linkPeer) update((s) => ({ ...s, device_links: { ...s.device_links, [linkHost]: linkPeer } }));
  }, [linkHost, linkPeer, update]);

  useEffect(() => {
    const off = onTransfer((e) => { if (e.type === "Paired") setQr(null); });
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => { off.then((f) => f()); clearInterval(t); };
  }, []);

  const showQr = async () => {
    try {
      const inv = await pairInvite();
      setQr({ img: await QRCode.toDataURL(inv.qr, { margin: 4, width: 576, errorCorrectionLevel: "L" }), code: inv.qr, until: Date.now() + inv.expires_in_s * 1000 });
    } catch { say("Could not make a pairing code. Try again."); }
  };
  // The code appears by itself, once, when the Files step starts. After that the button asks for a new one.
  useEffect(() => {
    if (step !== 1 || triedCode) return;
    setTriedCode(true);
    showQr();
  }, [step, triedCode]); // eslint-disable-line react-hooks/exhaustive-deps

  const pickKind = (k: AddKind) => { setKind(k); setSkipped([]); setQr(null); setTriedCode(false); };
  const skip = () => { if (step === 1) setQr(null); setSkipped((s) => [...s, step]); };
  const left = qr ? Math.max(0, Math.round((qr.until - now) / 1000)) : 0;
  const expired = !!qr && left === 0;
  const notice = kind === "ipad" ? clipNotice(clip?.notice ?? null) : null;

  const device = devices.find((d) => d.hostId === hostId);
  const name = device?.name ?? (kind === "ipad" ? "Your iPad" : "Your phone");
  const toDesk = () => { select(device?.key ?? null); setTab("desk"); };

  const card = (i: number) => {
    const current = i === step;
    const tag = i < step ? (seenAt[i] ? "Done" : "Skipped") : current ? (i === 0 ? `Waiting for the ${kind === "ipad" ? "iPad" : "phone"}` : i === 1 && expired ? "Expired" : "") : "Next";
    return (
      <section key={i} className={cn("flex-none rounded-[18px] border-2 bg-card px-4 py-3.5", current ? "border-primary" : "border-border")}>
        <div className="flex items-center justify-between gap-3">
          <strong className="font-semibold">{TITLES[kind][i]}</strong>
          <span className={cn("font-medium", tag === "Done" ? "text-success" : tag === "Expired" ? "text-destructive" : "text-muted-foreground")}>{tag}</span>
        </div>
        {current && i === 0 && <p className="mt-1.5 text-muted-foreground">{BT_TEXT[kind]}</p>}
        {current && i === 1 && (expired ? <p className="mt-1.5 text-muted-foreground">This code expired after 5 minutes.</p> : qr && (
          <div className="mt-3 flex gap-5">
            <img src={qr.img} alt="Pairing code" className="size-52 flex-none rounded-xl bg-white [image-rendering:pixelated]" />
            <div className="flex flex-col gap-2">
              <span>{FILES_TEXT[kind]} Expires in <span className="tabular-nums">{Math.floor(left / 60)}:{String(left % 60).padStart(2, "0")}</span>.</span>
              <span className="text-muted-foreground">No camera? Copy the code and paste it in Bridge.</span>
              <Button variant="secondary" className={`${RAISED} mt-auto self-start`} onClick={() => navigator.clipboard.writeText(qr.code).catch(() => say("Could not copy the code. Try again."))}>Copy code</Button>
            </div>
          </div>
        ))}
        {current && i === 2 && <p className="mt-1.5 text-muted-foreground">Plug the iPad in by USB once and tap Trust. After that it works over Wi-Fi.</p>}
        {current && (
          <div className="mt-2.5 flex gap-2">
            {i === 1 && (expired || !qr) && <Button className="h-11 rounded-xl px-4 font-semibold" onClick={showQr}>{expired ? "Show a new code" : "Show pairing code"}</Button>}
            {i === 2 && <Button className="h-11 rounded-xl px-4 font-semibold" onClick={() => pairIpad().catch(() => say("Could not pair over USB. Plug the iPad in and try again."))}>Pair over USB</Button>}
            <Button variant="secondary" className="h-11 rounded-xl px-4 font-semibold" onClick={skip}>Skip</Button>
          </div>
        )}
      </section>
    );
  };

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-[760px] flex-1 flex-col gap-2.5 overflow-auto text-[15px]">
      <div className="flex flex-none items-center justify-between">
        <h2 className="text-[26px] font-semibold">{kind === "ipad" ? "Add an iPad" : "Add a phone"}</h2>
        <div role="group" aria-label="Kind of device" className="flex gap-1 rounded-xl bg-muted p-1">
          {(["ipad", "phone"] as const).map((k) => (
            <button
              key={k} type="button" aria-pressed={kind === k} onClick={() => pickKind(k)}
              className={cn("rounded-[9px] px-3.5 py-1 font-medium focus-visible:outline-2 focus-visible:outline-ring", kind === k && "bg-card")}
            >{k === "ipad" ? "iPad" : "Phone"}</button>
          ))}
        </div>
      </div>
      {Array.from({ length: count }, (_, i) => card(i))}
      {notice && <p role="status" className="flex-none px-1 text-muted-foreground">{notice}</p>}
      {step === count && (
        <section className="flex-none rounded-[18px] border-2 border-primary bg-card px-4 py-3.5">
          <strong className="font-semibold">{name} is ready</strong>
          <p className="mt-1.5 text-muted-foreground">Put it next to a screen edge on the Desk.</p>
          <Button className="mt-2.5 h-11 rounded-xl px-4 font-semibold" onClick={toDesk}>Go to the Desk</Button>
        </section>
      )}
    </div>
  );
}
