# v1 acceptance — iPad app (Bridge)

Devices: Windows laptop (hub, `ctp-app.exe`), iPad Air (`Bridge` app). Same Wi-Fi.
Fill in **Result** (PASS / FAIL + note). Speeds in MB/s as shown in the Progress card.

## Install

1. On the iPad, remove "CTP Spike" first. Free-account app slots are limited.
2. Download the build: `gh run download --name Bridge-ipa` on branch `v1/ios`.
3. In iloader, choose **Import IPA** and select the downloaded file.
4. The Apple ID password is typed only by the owner, only into iloader. Never into chat, docs, or a script.

## Checklist

**Run 2026-11-04** on CI build 41753021701 (commit 32f08d8), hub rebuilt from the same commit. The owner sideloaded the app, went through the list and reported that everything looks right. This was one overall report, so there are no per-row notes: the MB/s for rows 4 and 5 were not recorded.

| # | Check | How | Expected | Result |
|---|---|---|---|---|
| 1 | Pair by QR | PC: Transfer → Show pairing code. iPad: Bridge → Pair with PC → scan | Both list each other | PASS (owner) |
| 2 | Paste code with camera denied | Deny camera for Bridge. iPad: Pair with PC → paste the code | Pairs without camera | PASS (owner) |
| 3 | PC → iPad file lands in Files → Bridge | PC: send a file to the iPad | File appears under Bridge in Files and opens | PASS (owner) |
| 4 | iPad → PC file, MB/s noted | iPad: send a file to the PC from Bridge | File lands in `Downloads\Bridge` intact; note MB/s | PASS (owner) |
| 5 | Big video both ways | Send a ~1 GB video PC → iPad, then iPad → PC | Both arrive intact and play; note MB/s | PASS (owner) |
| 6 | PC sends while Bridge is closed | Close Bridge on iPad. PC: send a file | PC says waiting. Open Bridge → file arrives | PASS (owner) |
| 7 | Same, PC restarted in between | Repeat row 6, restarting `ctp-app.exe` on PC before opening Bridge | File still arrives | PASS (owner) |
| 8 | Queued file deleted | Queue 2+ files PC → iPad with Bridge closed. Delete the first queued file before it sends | Failure shown for the deleted file; next batch still arrives | PASS (owner) |
| 9 | Background mid-transfer | Start a short and a long transfer. Switch away from Bridge on the iPad during both | Short file finishes. Long file fails on both sides. Resend works | PASS (owner) |
| 10 | Share sheet (5 files, same names) | Share 5 files with the same names from Files or Photos to Bridge | All 5 arrive, none overwritten | PASS (owner) |
| 11 | Send clipboard to PC | iPad: copy text, tap Send clipboard to PC. PC: Ctrl+V | Text arrives on PC | PASS (owner) |
| 12 | iPad clipboard sync with Bridge paired | With Bridge paired, copy text on iPad, then on PC, paste each into the other | Clipboard sync still works both ways | PASS (owner) |
| 13 | App survives 7-day re-sign | After re-signing with iloader, open Bridge | Data and pairing kept; transfers still work | Not run: needs a real 7-day re-sign |
| 14 | Logs have no file names | Read `%APPDATA%\CrossThePlatforms\engine.log` on PC | Sizes/counts only. No file names, no clipboard text | PASS (owner) |
| 15 | Sharing from other apps status | Open Bridge on iPad and read the "Sharing from other apps" line | Shows `Sharing from other apps: Ready`. If it shows `unavailable (<bundle id>)`, write that text in Result | PASS (owner) |
| 16 | Camera prompt on first Pair | Fresh install or reset camera permission. First Pair with PC | Camera permission prompt appears | PASS (owner) |
| 17 | Live Photo in share set | Share a Live Photo from Photos to Bridge | Arrives as a still, full size (not a thumbnail) | PASS (owner) |
| 18 | iCloud Drive file not downloaded | In Files, pick an iCloud Drive file that is not on the device and send it to PC | Sends successfully | PASS (owner) |
| 19 | Open in Bridge | In Files, select a PDF → Share → Open in Bridge | PDF is sent to PC | PASS (owner) |
