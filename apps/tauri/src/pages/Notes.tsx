import { useCallback, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { type NotesView, notesList, notesOpen, onNotesChanged } from "@/lib/api";
import { useHub } from "@/lib/hub";
import { RAISED } from "@/lib/utils";

const CARD = "flex-none rounded-[20px] bg-card";
const ICON = "flex-none rounded-lg px-2.5 py-1.5 text-[13px] font-medium text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring";

export default function Notes() {
  const { say } = useHub();
  const [notes, setNotes] = useState<NotesView | null>(null);
  const load = useCallback(() => notesList().then(setNotes).catch(() => say("Could not load your notes. Try again.")), [say]);
  useEffect(() => {
    void load();
    const off = onNotesChanged(() => void load());
    return () => { void off.then((f) => f()); };
  }, [load]);
  const copy = (text: string) => navigator.clipboard.writeText(text).then(() => say("Copied."), () => say("Could not copy that. Try again."));

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto text-[15px] data-[hidden]:hidden">
      <section className={`${CARD} flex items-center justify-between gap-3 px-5 py-[18px]`}>
        <span>
          <span className="block text-xl font-semibold">Quick notes</span>
          <span className="block text-muted-foreground">Text you send to notes from the phone or iPad lands here, newest first, in Documents\Bridge\Inbox.md.</span>
        </span>
        <Button variant="secondary" className={RAISED} disabled={!notes?.notes.length} onClick={() => notesOpen().catch(() => say("Could not open the notes file. Try again."))}>Open file</Button>
      </section>
      {notes?.unsaved && (
        <section role="status" className={`${CARD} flex items-center justify-between gap-3 px-5 py-3.5`}>
          <span className="text-destructive">Some notes could not be added to Inbox.md, so Bridge kept them in Inbox-unsaved.md. Nothing was lost.</span>
          <Button variant="secondary" className={RAISED} onClick={() => notesOpen(true).catch(() => say("Could not open the file. Try again."))}>Open unsaved notes</Button>
        </section>
      )}
      <section className={`${CARD} px-5 py-3`}>
        {notes && notes.notes.length === 0 && <p className="py-4 text-muted-foreground">Nothing yet. Use Send to notes on the phone or iPad.</p>}
        {notes?.notes.map((n, i) => (
          <div key={`${n.stamp}-${i}`} className="flex items-start justify-between gap-3 border-t border-border py-3 first:border-t-0">
            <div className="min-w-0 flex-1">
              <div className="text-[13px] text-muted-foreground">{n.stamp} · {n.device}</div>
              <p className="whitespace-pre-wrap break-words">{n.text}</p>
            </div>
            <button type="button" className={ICON} aria-label={`Copy the note from ${n.device} at ${n.stamp}`} onClick={() => void copy(n.text)}>Copy</button>
          </div>
        ))}
      </section>
    </div>
  );
}
