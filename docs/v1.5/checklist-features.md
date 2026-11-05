# Feature device checks

Rows the owner ticks on real devices. Nothing here is claimed to work until ticked.

| Feature | Check | Done |
|---|---|---|
| Keep formatting | Copy bold text, a list and a link on Windows (browser or Word), paste into Notes on the iPad. For now the iPad receives plain text only, so expect the words without the formatting, the same in a plain field and in Bridge's iPad app. | [ ] |
| Keep formatting | Formatted paste to the iPad waits on the spike: with Bridge quit, run `ipad-input htmlset` (spikes/ipad-input), then paste into Notes (bold and italic expected, with é and 👋 intact) and into a plain field (the sample text expected, also with é and 👋). It sends one pasteboard item with public.html and the text types together; if it works, Bridge can send both. | [ ] |
| Keep formatting | Copy formatted text in Notes on the iPad, paste into Word on Windows: bold, list and link kept. Pasting into Notepad gives plain text. | [ ] |
| Keep formatting | Copy formatted text with accents, Arabic or emoji in Notes on the iPad: it arrives in Word on Windows with the same characters. | [ ] |
| Keep formatting | Copy a range from Word and from Excel on Windows: pasting back into Word keeps styles, and Bridge stays responsive (Excel copies carry large HTML). | [ ] |
| Keep formatting | The same copy reaches the phone as plain text only. | [ ] |
| Clipboard history | Copy three things on Windows, open History: all three are listed, newest first, and clicking one puts it back on the clipboard. | [ ] |
| Clipboard history | Copy a password in a password manager (KeePass, 1Password or Bitwarden): it is not in History, not on the iPad or phone, and not in Win+V either. This checks the `CanIncludeInClipboardHistory` and `ExcludeClipboardContentFromMonitorProcessing` formats. | [ ] |
| Clipboard history | Copy formatted text: its history entry pastes back with bold, list and link. | [ ] |
| Notes | Send text from the phone: it lands in Documents\Bridge\Inbox.md and the Notes tab shows it. Copy again on the PC: the note is not overwritten. | [ ] |
| Notes | On the phone, share text from any app and pick "Send to notes" in the share sheet: it goes to notes, not to the normal send flow. Try the notes tile too. | [ ] |
| Notes | Open Inbox.md in an editor that locks it, send a note: it lands in Inbox-unsaved.md and the Notes tab says so. | [ ] |
| Send to the device | Control the iPad, copy text on Windows first, click into a text field on the iPad, press Ctrl+Alt+V: the text is pasted there (Cmd+V). Then the same with an image. | [ ] |
| Send to the device | Control the phone, click into a text field, press Ctrl+Alt+V: the text is pasted there (Ctrl+V). If it says the device did not answer, or nothing appears, note which and tell Claude. An image gives "Phones take text only" and nothing is sent. | [ ] |
| Send to the device | Copy a password in a password manager, control the iPad, press Ctrl+Alt+V: it says the clipboard is empty or private, and nothing reaches the iPad. | [ ] |
| Send to the device | Select two files in the front Explorer window, with a second tab open on another folder. Control the iPad (or phone), press Ctrl+Alt+F: those two files arrive, from the tab in front. | [ ] |
| Send to the device | With no device in control press Ctrl+Alt+V and Ctrl+Alt+F; with a device in control press Ctrl+Alt+F with no file selected, or a browser in front: the tray and the top bar of the hub say why, and nothing is sent. | [ ] |
| Drop on the Desk | Drag a file from Explorer onto the iPad on the Desk: it arrives on the iPad. Drop onto the phone: it arrives there. | [ ] |
