import { useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { pairIpad, setClipboardSync, type ClipStatus } from "@/lib/api";
import { ipadLine } from "@/lib/clip";

export default function Clipboard({ clip }: { clip: ClipStatus | null }) {
  const [error, setError] = useState<string | null>(null);
  const connected = clip?.ipad != null && typeof clip.ipad === "object" && "Connected" in clip.ipad;
  const toggle = (on: boolean) => setClipboardSync(on).then(() => setError(null), (e) => setError(String(e)));

  return (
    <Card className="mt-2">
      <CardHeader>
        <CardTitle>Clipboard</CardTitle>
        <CardDescription>Copy on Windows, paste on the iPad, and back. Text and images. Contents are never logged.</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <label className="flex items-center gap-3 text-sm">
          <Switch checked={clip?.enabled ?? false} onCheckedChange={toggle} disabled={!clip} />
          Sync clipboard
        </label>
        <div className="flex items-center gap-3 text-sm">
          <span className="text-muted-foreground">iPad</span>
          <Badge variant={connected ? "default" : "secondary"}>{ipadLine(clip?.ipad ?? null)}</Badge>
          <Button size="sm" variant="outline" onClick={() => pairIpad().catch((e) => setError(String(e)))}>Pair iPad (USB)</Button>
        </div>
        <p className="text-sm tabular-nums text-muted-foreground">
          Synced {clip?.synced ?? 0} · Skipped (too large) {clip?.skipped_too_large ?? 0}
        </p>
        {(error ?? clip?.notice) && <Alert><AlertDescription>{error ?? clip?.notice}</AlertDescription></Alert>}
      </CardContent>
    </Card>
  );
}
