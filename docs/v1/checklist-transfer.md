# v1 acceptance — file transfer + Android app

Devices: Windows laptop (hub, `ctp-app.exe`), Galaxy S24+ (`Bridge` app). Same Wi-Fi.
Fill in **Result** (PASS / FAIL + note). Speeds in MB/s as shown in the Transfer tab or the phone toast.

| # | Check | How | Expected | Result |
|---|---|---|---|---|
| 1 | Pair | PC: Transfer → Show pairing code. Phone: Bridge → Pair with PC → scan | both list each other | |
| 2 | Pairing survives reboot | reboot phone, then PC; check both lists | still paired | |
| 3 | Code is single use | scan the same QR again after pairing | "pairing refused: used code" | |
| 4 | Code expires | show a code, wait > 5 min, scan | PC shows "Code expired"; phone: "expired code" | |
| 5 | Big file phone → PC | phone: Send files → a ~1 GB video | ≥ 80 MB/s; file in `Downloads\Bridge`, plays | |
| 6 | Big file PC → phone | drop the same file on the S24+ card | ≥ 80 MB/s; notification; file in `Download/Bridge` | |
| 7 | Share sheet | phone: share a PDF → Bridge | lands in `Downloads\Bridge` | |
| 8 | Explorer Send to | right-click a file → Send to → `Bridge – <phone>` | lands on the phone, notification | |
| 9 | Cut-off | phone Wi-Fi off mid-transfer (step 5) | no complete-looking file on either side; error on both | |
| 10 | Firewall denied | deny the Windows Firewall prompt (or block `ctp-app.exe` inbound), send PC ← phone | still arrives (iroh fallback), slower | |
| 11 | Phone clipboard → PC | copy text on phone, tap "Send clipboard to PC" tile, Ctrl+V on PC | text arrives | |
| 12 | PC clipboard → phone | Ctrl+C text on PC, paste on phone | text arrives | |
| 13 | After phone reboot | reboot, don't open Bridge, send from PC | received; service notification present | |
| 14 | Logs clean | read `%APPDATA%\CrossThePlatforms\engine.log` | sizes/counts only — no file names, no clipboard text | |
