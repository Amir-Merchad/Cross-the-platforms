# Base UI check

Fill in **Result** (PASS, or FAIL with a note). Compare with the Claude Design canvas "Bridge UI redesign".

## Windows hub

| # | State or case | How | Expected | Result |
|---|---|---|---|---|
| 1 | Look | Open Bridge from the tray | Matches the Desk board, in the system's light or dark mode | |
| 2 | Appearance | Settings: try Light, Dark, System | Changes at once and survives a restart | |
| 3 | Link existing devices | Select each device on the Desk | Asks "Is this also your ...?" unless the names match. After choosing, one device shows all three rows | |
| 4 | Drag and place | Drag a device toward a screen edge | Edges light, it pulls in, settles, status line confirms | |
| 5 | Swap | Drop a device on a taken edge | The two swap, and pushing across each edge still enters the right device | |
| 6 | Keyboard placing | Tab to a device, press arrow keys, then Delete | Places, then unplaces | |
| 7 | Covered edge | With two monitors, drag toward an edge touching the other monitor | That edge is not offered | |
| 8 | Switch | Press Switch, then Back to Windows | Pointer moves, In control shows, edge glows, then returns | |
| 9 | Unplace the device in control | While on the iPad, press Delete on it from the hub by keyboard | Input returns to Windows first | |
| 10 | File drop | Drag a file from Explorer onto a device | Device grows, ring shows progress, file arrives | |
| 11 | Send files button | Select a device, press Send files | File picker opens and the file sends | |
| 12 | Files wait | Close Bridge on the iPad and drop 2 files | Count badge on the device, Waiting on Transfers | |
| 13 | Waiting after restart | Quit from the tray, reopen | Waiting still shows. Opening Bridge on the iPad sends them | |
| 14 | Failed send | Turn off the phone's Wi-Fi and send to it | Plain failure with Send again, and Send again works once it is back | |
| 15 | Device offline | Turn off Bluetooth on the phone | Device loses its colour and reads Offline | |
| 16 | Bluetooth off | Turn off Windows Bluetooth | Banner appears, Open Bluetooth settings works, files still send | |
| 17 | Monitor unplugged | Unplug a monitor with a device on it | Device moves to the tray as Screen not connected, returns when plugged in | |
| 18 | Hold to forget | Hold and release early, then hold fully, on a test pairing | Early release cancels. Full hold forgets and reminds you about Bluetooth | |
| 19 | Add a device | Add a device, iPad and Phone | Steps move on by themselves, skip works, expired code offers a new one | |
| 20 | Clipboard page | Toggle sync, press Pair over USB | Switch works, pairing shows progress and the result | |
| 21 | Settings | Change edge hold, Start with Windows | Saves at once, no Save button anywhere | |
| 22 | Quick changes | Change the received folder, then drag a device at once | Both stick after a restart | |
| 23 | Explorer Send to | Right-click a file, Send to, Bridge | Shows in Transfers | |
| 24 | Reduced motion | Windows: turn Animation effects off | Everything still works, with no movement | |
| 25 | Quit | Tray, Quit | Exits cleanly, input back on Windows | |
