# Base UI check

Fill in **Result** (PASS, or FAIL with a note). Compare with the Claude Design canvas "Bridge UI redesign".

**Status:** Windows row 1 is the only Windows row with a result so far. Windows rows 2 to 28, all Android rows and all iPad rows are not yet run on devices.

## Windows hub

| # | State or case | How | Expected | Result |
|---|---|---|---|---|
| 1 | Look | Open Bridge from the tray | Matches the Desk board, in the system's light or dark mode | PASS (owner: "it looks amazing") |
| 2 | Appearance | Settings: try Light, Dark, System | Changes at once and survives a restart | |
| 3 | Link existing devices | Select each device on the Desk | Asks "Is this also your ...?" unless the names match. After choosing, one device shows all three rows | |
| 4 | Drag and place | Drag a device toward a screen edge | Edges light, it pulls in, settles, status line confirms | |
| 5 | Swap | Drop a device on a taken edge | The two swap, and pushing across each edge still enters the right device | |
| 6 | Keyboard placing | Tab to a device, press arrow keys, then Delete | Places, then unplaces | |
| 7 | Covered edge | With two monitors, drag toward an edge touching the other monitor | That edge is not offered | |
| 8 | Switch | Press Switch, then Back to Windows | Pointer moves, In control shows, edge glows, then returns | |
| 9 | Unplace the device in control | While on the iPad, use touch on the PC, or a second mouse or keyboard not shared with it, to run Hold to forget in the hub. Delete cannot do this: while the iPad has the keyboard, Delete goes to the iPad | Input returns to Windows first | |
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
| 26 | Honest Sent | Send a file and cut the receiver off at the very end (close Bridge there as the bar fills). Then send a 0-byte file | The first row reads as failed, never Sent. The 0-byte row ends Sent and does not stay moving | |
| 27 | Bluetooth off at launch | Turn off Windows Bluetooth, then open Bridge | Placed and linked devices stay on the Desk as No Bluetooth, with the Bluetooth off banner | |
| 28 | Failures in plain words | Send a file that was just deleted (Send again on an old row). Send a folder from Explorer Send to | Each says what to do: the file could not be read, and folders can't be sent, pick the files inside | |

## Android

Phone states are named as on the Mobile board. Check each in the phone's light and dark mode.

| # | State or case | How | Expected | Result |
|---|---|---|---|---|
| 1 | notPaired | Forget the PC (or fresh install), open Bridge | Top reads "Not paired", card "No PC yet", primary "Pair with PC", short instruction. No Recent, no Send clipboard | |
| 2 | pair | Press Pair with PC | Camera in an amber-bordered square, "Point the camera at the code on your PC". Scanning a code from Add a device pairs and returns Home | |
| 3 | pairNoCamera | Deny the camera, or revoke it in Settings | "The camera is off for Bridge", Paste code works with a copied code. A bad code says it did not work and the scanner re-arms | |
| 4 | home | Open Bridge when paired | Top reads "Paired" (never "Connected"), card shows the PC and "Ready", Send files amber, Send clipboard grey, Recent, Received files, Sharing from other apps | |
| 5 | Look | Compare with the Mobile board | Rubik, amber `#FFC531` with dark text and a 3 px lip, follows the system light or dark | |
| 6 | sending | Send a large file with Send files | PC card "Receiving your files", progress card with name, rate, bar, "You can leave Bridge, sending keeps going." Row moves to Recent as "Sent, size" | |
| 7 | receiving | Send a large file from the PC | PC card "Sending to you", progress card, then "Received, size" in Recent | |
| 8 | failed | Turn the PC off or close Bridge there, then send | Top reads "PC not reachable", card "Looking for your PC", warning says to send it again from the app you started in. No Send again button. Next good send returns to "Paired" | |
| 9 | Send clipboard | Copy text, press Send clipboard | Text arrives on the PC. Tile still works too | |
| 10 | shareWaiting | Share 5 photos from Gallery to Bridge | Screen stays open while copying ("5 files are ready. Sending to ... now."), then closes, files arrive | |
| 11 | sharePick | Pair two PCs, share files | "Send N files to", PCs as buttons, Cancel closes | |
| 12 | Hold to forget | Tap once, then hold the button | A tap does nothing. Holding fills and forgets after about 1 second | |
| 13 | Service, tile, boot | Reboot the phone | Notification returns, tile and receiving still work | |
| 14 | A swipe that starts on the button does not forget | Put a finger on Hold to forget and scroll the page, or slide off the button, before 1 second | Nothing is forgotten | |
| 15 | Several PCs | Pair two PCs | A chooser row appears on Home. Send files, Send clipboard and Forget act on the chosen PC. The tile still sends to all | |

## iPad

Same state names as the Mobile board. Check each in the iPad's light and dark mode.

| # | State or case | How | Expected | Result |
|---|---|---|---|---|
| 1 | home | Open Bridge when paired | Top reads "Paired", PC card shows "Ready", Send files, Send clipboard, Recent, Sharing from other apps | |
| 2 | sending | Send a large file | PC card "Receiving your files", progress card with name, rate and bar. Row moves to Recent as "Sent, size" | |
| 3 | receiving | Send a large file from the PC | PC card "Sending to you", progress card, then "Received, size" in Recent | |
| 4 | notPaired | Fresh install or forget the PC | Top reads "Not paired", card "No PC yet", primary "Pair with PC". No Recent, no Send clipboard | |
| 5 | pair | Press Pair with PC | Camera square with the instruction. Scanning a code from Add a device pairs and returns Home | |
| 6 | pairNoCamera | Deny or revoke the camera | Camera-off message, Paste code works, a bad code says it did not work | |
| 7 | failed | Close Bridge on the PC, then send | Top reads "PC not reachable", plain warning, next good send returns to "Paired" | |
| 8 | Fonts | Look at headings, body and buttons | Rubik shows, with visibly different weights | |
| 9 | Hold to forget, short press | Tap once, or scroll the page starting on the button | Nothing is forgotten. Holding fully forgets | |
| 10 | Several PCs | Pair two PCs, if available | PC chooser appears. Send files, Send clipboard and Forget act on the chosen PC | |
| 11 | Clipboard sent | Copy text, press Send clipboard | A plain card, not the warning colour, says Clipboard sent and goes away after a few seconds | |
