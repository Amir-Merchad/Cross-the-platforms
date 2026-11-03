# v1 input acceptance checklist

Run on the real devices (iPad Air M2 on the right, Galaxy S24+ on the left). Quit the spike tools first.
Stop `ctp-engine` with **Enter**; quit the app from the **tray → Quit** (never kill either — the MT7921 Bluetooth chip can wedge).

## Engine (`target\release\ctp-engine.exe`, Task 5)

| Check | Result |
|---|---|
| Both hosts reconnect by themselves (`"connected":true`) | PENDING |
| Right edge → iPad, deliberate push back → Windows | PENDING |
| Left edge → S24+, deliberate push back → Windows | PENDING |
| Ctrl+Alt+1 → iPad, Ctrl+Alt+2 → S24+, Ctrl+Alt+Q → Windows | PENDING |
| Esc ×3 on a device → Windows | PENDING |
| Shift held, iPad Bluetooth off → instant return, no stuck Shift | PENDING |
| Hold Shift on Windows, cross, release on the iPad, come back → Windows Shift not stuck | PENDING |
| Hold Ctrl+Alt+1 for a second → enters once, no flicker | PENDING |
| German layout: AltGr+7 types `{`, AltGr+Q types `@` (Windows and iPad), no switching | PENDING |
| Laptop Bluetooth off → on: re-advertises, devices reconnect by themselves | PENDING |
| Laptop sleep → resume: devices reconnect by themselves | PENDING |

## App (`target\debug\ctp-app.exe`, Tasks 6–7)

| Check | Result |
|---|---|
| Starts to the tray, no window; tray tooltip shows status | PENDING |
| Tray: Switch to <device>, Back to Windows, Settings…, Quit work | PENDING |
| Second launch only opens the existing window | PENDING |
| Layout tab: screens drawn, devices placed on edges; same edge twice disables Save | PENDING |
| Calibration: changing iPad gain takes effect after Save, no restart | PENDING |
| Diagnostics: iPad interval ≈ 15 ms, send avg/max shown while in control | PENDING |
| Reboot laptop → tray icon back, devices reconnect, no action needed | PENDING |
| Quit from tray → Windows input normal | PENDING |
| Log/terminal output has no keystrokes or clipboard text | PENDING |

Note: with an external monitor at a different scaling than the laptop screen, check that both edges still trigger (DPI).
