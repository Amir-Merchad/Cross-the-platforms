import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { Button } from "@/components/ui/button";
import { useHub } from "@/lib/hub";
import { onTransfer, pairInvite } from "@/lib/transfer";
import { RAISED } from "@/lib/utils";

// Temporary: Task 7 moves this into Add a device and deletes the file.
export default function Transfer() {
  const { say } = useHub();
  const [qr, setQr] = useState<{ img: string; code: string; until: number } | null>(null);
  const [now, setNow] = useState(() => Date.now());

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
  const left = qr ? Math.max(0, Math.round((qr.until - now) / 1000)) : 0;

  return (
    <section className="flex-none rounded-[20px] bg-card px-4 py-3.5">
      <div className="font-semibold">Pair a phone</div>
      <p className="mb-3 text-muted-foreground">Open Bridge on the phone and scan this code. It works once and for 5 minutes.</p>
      {qr && left > 0 ? (
        <div className="flex items-center gap-6">
          <img src={qr.img} alt="Pairing code" className="size-72 rounded-xl bg-white [image-rendering:pixelated]" />
          <div className="flex flex-col gap-2">
            <p className="text-[13px] text-muted-foreground tabular-nums">Expires in {Math.floor(left / 60)}:{String(left % 60).padStart(2, "0")}</p>
            <Button variant="secondary" className={RAISED} onClick={() => navigator.clipboard.writeText(qr.code).catch(() => say("Could not copy the code. Try again."))}>Copy code</Button>
          </div>
        </div>
      ) : <Button variant="secondary" className={RAISED} onClick={showQr}>{qr ? "Code expired. Show a new one" : "Show pairing code"}</Button>}
    </section>
  );
}
