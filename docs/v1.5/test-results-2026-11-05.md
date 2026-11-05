# Device test results, 2026-11-05

Owner's report after the 15-minute session on the laptop, iPad and phone. One overall report, so no timings.

| Test | Result |
|---|---|
| iPad input service over Wi-Fi (`ipad-input-check`) | PASS |
| Bluetooth off in Settings, Desk shows the iPad | PASS |
| Real use on the iPad over Wi-Fi | PASS |
| Wi-Fi drop returns input to Windows, reconnects | PASS |
| Clipboard both ways after the new pairing | PASS |
| Free layout | **Buggy UI**, details needed |

## Bugs to fix

1. Free layout: buggy UI on the Desk. Ask the owner what looked wrong.
2. Turning Bluetooth on made the iPad's Wi-Fi link disconnect by itself.
3. With Bluetooth on, two iPads show: one Wi-Fi, one Bluetooth. Migration should have retired the Bluetooth host.
