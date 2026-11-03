# v1 acceptance — file transfer + Android app

Devices: Windows laptop (hub, `ctp-app.exe`), Galaxy S24+ (`Bridge` app). Same Wi-Fi.
Fill in **Result** (PASS / FAIL + note). Speeds in MB/s as shown in the Transfer tab or the phone toast.

| # | Check | How | Expected | Result |
|---|---|---|---|---|
| 1 | Pair | PC: Transfer → Show pairing code. Phone: Bridge → Pair with PC → scan | both list each other | PASS (after QR fix 2026-11-03: invite was 472 chars, unscannable) |
| 2 | Pairing survives reboot | reboot phone, then PC; check both lists | still paired | |
| 3 | Code is single use | scan the same QR again after pairing | "pairing refused: used code" | |
| 4 | Code expires | show a code, wait > 5 min, scan | PC shows "Code expired"; phone: "expired code" | |
| 5 | Big file phone → PC | phone: Send files → a ~1 GB video | ≥ 80 MB/s; file in `Downloads\Bridge`, plays | LIMITED BY PC DISK: 2.55 GB arrived intact; ~100 MB/s, stall at ~1 GB, avg ~46–50. The disk alone (no network) sustains 34–37 MB/s with 10–20 s stalls in any folder: Kingston NV2 1 TB (DRAM-less), 51 GB free (94% full). |
| 6 | Big file PC → phone | drop the same file on the S24+ card | ≥ 80 MB/s; notification; file in `Download/Bridge` | PASS (owner: ≥ 80 MB/s, 2.55 GB video) |
| 7 | Share sheet | phone: share a PDF → Bridge | lands in `Downloads\Bridge` | PASS |
| 8 | Explorer Send to | right-click a file → Send to → `Bridge – <phone>` | lands on the phone, notification | PASS |
| 9 | Cut-off | phone Wi-Fi off mid-transfer (step 5) | no complete-looking file on either side; error on both | |
| 10 | Firewall denied | deny the Windows Firewall prompt (or block `ctp-app.exe` inbound), send PC ← phone | still arrives (iroh fallback), slower | |
| 11 | Phone clipboard → PC | copy text on phone, tap "Send clipboard to PC" tile, Ctrl+V on PC | text arrives | FAIL: tile toast "timeout"; PC never received it (no inbound clip in engine.log) — diagnosing |
| 12 | PC clipboard → phone | Ctrl+C text on PC, paste on phone | text arrives | PASS |
| 13 | After phone reboot | reboot, don't open Bridge, send from PC | received; service notification present | |
| 14 | Logs clean | read `%APPDATA%\CrossThePlatforms\engine.log` | sizes/counts only — no file names, no clipboard text | |
